use super::*;
use deadpan_core::{IterationId, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId};

const MAX_ADDRESS_STEPS: usize = MAX_DOCUMENT_NODES * 64;
const MAX_ADDRESS_BYTES: usize = 64 * 1024 * 1024;

/// One budget for the whole command/batch, checked before cloning addresses.
pub(in crate::generation_preparations) struct AddressBudget {
    steps: usize,
    bytes: usize,
}

impl Default for AddressBudget {
    fn default() -> Self {
        Self {
            steps: MAX_ADDRESS_STEPS,
            bytes: MAX_ADDRESS_BYTES,
        }
    }
}

impl AddressBudget {
    /// Isolation may grow every node name to the identity limit. Reserve that
    /// upper bound before the proof API clones and remaps a retained address.
    pub(super) fn clone_steps(&mut self, depth: usize) -> Result<(), StoreError> {
        if depth > MAX_DOCUMENT_DEPTH {
            return Err(invalid("preparation address exceeds its depth limit"));
        }
        let bytes = std::mem::size_of::<ScopedNodeTarget>()
            + deadpan_core::MAX_IDENTITY_BYTES
            + depth
                * (std::mem::size_of::<RepeatEditStep>() + 2 * deadpan_core::MAX_IDENTITY_BYTES);
        self.charge(depth + 1, bytes)
    }

    fn charge(&mut self, steps: usize, bytes: usize) -> Result<(), StoreError> {
        if steps > self.steps || bytes > self.bytes {
            return Err(invalid(
                "preparation scoped addresses exceed their aggregate work or byte limit",
            ));
        }
        self.steps -= steps;
        self.bytes -= bytes;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Address {
    scope: Option<usize>,
    hold: bool,
}

struct Scope<'a> {
    outer: Option<usize>,
    repeat: &'a NodeId,
    iteration: Option<&'a IterationId>,
    depth: usize,
    bytes: usize,
}

/// Borrowed authored ownership, with one shared link per Repeat edge. A wide
/// subtree never copies its ancestors. Only explicitly requested Hold addresses
/// are materialized; existing addresses can be compared without any allocation.
pub(in crate::generation_preparations) struct CanonicalTargets<'a> {
    nodes: BTreeMap<&'a NodeId, Address>,
    scopes: Vec<Scope<'a>>,
}

impl<'a> CanonicalTargets<'a> {
    pub(in crate::generation_preparations) fn new(
        document: &'a ProjectDocument,
        budget: &mut AddressBudget,
    ) -> Result<Self, StoreError> {
        budget.charge(document.nodes().len(), 0)?;
        let mut result = Self {
            nodes: BTreeMap::new(),
            scopes: Vec::new(),
        };
        let mut work = vec![(document.root(), None)];
        while let Some((node, scope)) = work.pop() {
            let kind = &document.nodes()[node].kind;
            if result.nodes.len() >= MAX_DOCUMENT_NODES
                || result
                    .nodes
                    .insert(
                        node,
                        Address {
                            scope,
                            hold: matches!(kind, NodeKind::Hold { .. }),
                        },
                    )
                    .is_some()
            {
                return Err(invalid(
                    "preparation ownership exceeds its bound or repeats a node",
                ));
            }
            match kind {
                NodeKind::Sequence { children } => {
                    work.extend(children.iter().map(|child| (child, scope)))
                }
                NodeKind::Retime { child, .. } => work.push((child, scope)),
                NodeKind::Repeat { child, .. } => {
                    let next = result.scope(scope, node, None)?;
                    work.push((child, Some(next)));
                    for (iteration, child) in document
                        .overrides()
                        .get(node)
                        .into_iter()
                        .chain(document.gap_overrides().get(node))
                        .flat_map(|entries| entries.iter())
                    {
                        let next = result.scope(scope, node, Some(iteration))?;
                        work.push((child, Some(next)));
                    }
                }
                _ => {}
            }
        }
        Ok(result)
    }

    fn scope(
        &mut self,
        outer: Option<usize>,
        repeat: &'a NodeId,
        iteration: Option<&'a IterationId>,
    ) -> Result<usize, StoreError> {
        let depth = outer.map_or(0, |index| self.scopes[index].depth) + 1;
        if depth > MAX_DOCUMENT_DEPTH || self.scopes.len() >= MAX_DOCUMENT_NODES {
            return Err(invalid("preparation Repeat ownership exceeds its bound"));
        }
        let bytes = outer.map_or(0, |index| self.scopes[index].bytes)
            + std::mem::size_of::<RepeatEditStep>()
            + repeat.as_str().len()
            + iteration.map_or(0, |value| value.allocation.as_str().len());
        let index = self.scopes.len();
        self.scopes.push(Scope {
            outer,
            repeat,
            iteration,
            depth,
            bytes,
        });
        Ok(index)
    }

    fn hold(&self, node: &NodeId) -> Option<Address> {
        self.nodes.get(node).copied().filter(|address| address.hold)
    }

    pub(in crate::generation_preparations) fn get(
        &self,
        node: &NodeId,
        budget: &mut AddressBudget,
    ) -> Result<Option<ScopedNodeTarget>, StoreError> {
        let Some(address) = self.hold(node) else {
            return Ok(None);
        };
        let depth = address.scope.map_or(0, |index| self.scopes[index].depth);
        let bytes = address.scope.map_or(0, |index| self.scopes[index].bytes)
            + std::mem::size_of::<ScopedNodeTarget>()
            + node.as_str().len();
        budget.charge(depth + 1, bytes)?;
        let mut repeats = Vec::with_capacity(depth);
        let mut current = address.scope;
        while let Some(index) = current {
            let scope = &self.scopes[index];
            repeats.push(RepeatEditStep {
                repeat: scope.repeat.clone(),
                branch: scope
                    .iteration
                    .map_or(RepeatEditBranch::Default, |iteration| {
                        RepeatEditBranch::Play {
                            iteration: iteration.clone(),
                        }
                    }),
            });
            current = scope.outer;
        }
        repeats.reverse();
        Ok(Some(ScopedNodeTarget {
            node: node.clone(),
            repeats,
        }))
    }

    pub(in crate::generation_preparations) fn matches(
        &self,
        target: &ScopedNodeTarget,
        budget: &mut AddressBudget,
    ) -> Result<bool, StoreError> {
        let Some(address) = self.hold(&target.node) else {
            return Ok(false);
        };
        let depth = address.scope.map_or(0, |index| self.scopes[index].depth);
        budget.charge(depth + 1, 0)?;
        if depth != target.repeats.len() {
            return Ok(false);
        }
        let mut current = address.scope;
        for step in target.repeats.iter().rev() {
            let scope = &self.scopes[current.expect("matching depth")];
            let branch = match (&step.branch, scope.iteration) {
                (RepeatEditBranch::Default, None) => true,
                (RepeatEditBranch::Play { iteration }, Some(expected)) => iteration == expected,
                _ => false,
            };
            if step.repeat != *scope.repeat || !branch {
                return Ok(false);
            }
            current = scope.outer;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{
        BeatNode, ColorPolicy, FrameRate, HoldAudio, HoldRecipe, IterationOrder, PresentationBasis,
        Subtree,
    };

    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }
    fn revision(value: &str) -> RevisionId {
        RevisionId::new(value).unwrap()
    }

    fn nested_wide(holds: usize, depth: usize) -> ProjectDocument {
        let empty = ProjectDocument::new(
            ProjectId::new("wide-addresses").unwrap(),
            revision("empty"),
            PresentationBasis {
                width: 512,
                height: 320,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let recipe = HoldRecipe {
            duration: FrameDuration::new(1).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        };
        let mut nodes = BTreeMap::new();
        let mut children = Vec::with_capacity(holds);
        for index in 0..holds {
            let node = id(&format!("hold-{index}"));
            children.push(node.clone());
            nodes.insert(node, BeatNode::hold("Pause", recipe.clone()));
        }
        let mut child = id("wide");
        nodes.insert(child.clone(), BeatNode::sequence("Wide", children));
        for index in 0..depth {
            let node = id(&format!("r{index:03}{}", "r".repeat(124)));
            let mut repeat = BeatNode::sequence("Repeat", vec![]);
            repeat.kind = NodeKind::Repeat {
                child,
                iterations: IterationOrder::new(revision("plays"), 1).unwrap(),
                gap: None,
                escalation: None,
            };
            nodes.insert(node.clone(), repeat);
            child = node;
        }
        let request = CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: revision("fixture"),
            command: Command::Insert {
                parent: id("root"),
                index: 0,
                subtree: Subtree {
                    root: child,
                    nodes,
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        };
        deadpan_core::apply(&empty, &request)
            .unwrap()
            .forward
            .apply(&empty)
            .unwrap()
    }

    #[test]
    fn wide_nested_index_shares_ancestry_and_only_allocates_requested_targets() {
        let document = nested_wide(99_000, 250);
        let mut budget = AddressBudget::default();
        let ownership = CanonicalTargets::new(&document, &mut budget).unwrap();
        assert_eq!(ownership.nodes.len(), 99_252);
        assert_eq!(
            ownership.scopes.len(),
            250,
            "one link per authored Repeat edge"
        );
        assert_eq!(
            budget.bytes, MAX_ADDRESS_BYTES,
            "indexing clones no target addresses"
        );
        assert_eq!(MAX_ADDRESS_STEPS - budget.steps, document.nodes().len());
        let first = ownership.get(&id("hold-0"), &mut budget).unwrap().unwrap();
        assert_eq!(first.repeats.len(), 250);
        let charged = MAX_ADDRESS_BYTES - budget.bytes;
        assert!(
            charged < 64 * 1024,
            "one address, not all 99,000: {charged}"
        );
        let steps = budget.steps;
        assert!(ownership.matches(&first, &mut budget).unwrap());
        assert_eq!(steps - budget.steps, 251);
        assert_eq!(
            MAX_ADDRESS_BYTES - budget.bytes,
            charged,
            "matching never allocates"
        );

        // Failure consumes no capacity and occurs before constructing a Vec
        // or cloning even the first identifier of the refused address.
        let mut bytes = AddressBudget {
            steps: MAX_ADDRESS_STEPS,
            bytes: charged - 1,
        };
        assert!(ownership.get(&id("hold-0"), &mut bytes).is_err());
        assert_eq!(bytes.bytes, charged - 1);
        assert_eq!(bytes.steps, MAX_ADDRESS_STEPS);
        let mut work = AddressBudget {
            steps: 250,
            bytes: MAX_ADDRESS_BYTES,
        };
        assert!(ownership.get(&id("hold-0"), &mut work).is_err());
        assert_eq!(work.bytes, MAX_ADDRESS_BYTES);
        assert_eq!(work.steps, 250);

        // Aggregate bounds apply even when callers drop each previous result.
        let mut batch = AddressBudget {
            steps: MAX_ADDRESS_STEPS,
            bytes: charged * 2,
        };
        ownership.get(&id("hold-0"), &mut batch).unwrap().unwrap();
        ownership.get(&id("hold-1"), &mut batch).unwrap().unwrap();
        assert_eq!(batch.bytes, 0);
        assert!(ownership.get(&id("hold-2"), &mut batch).is_err());
    }

    #[test]
    fn index_budget_is_checked_before_scanning_and_missing_targets_do_not_allocate() {
        let document = nested_wide(8, 3);
        let mut exhausted = AddressBudget {
            steps: document.nodes().len() - 1,
            bytes: 0,
        };
        assert!(CanonicalTargets::new(&document, &mut exhausted).is_err());
        let mut budget = AddressBudget::default();
        let ownership = CanonicalTargets::new(&document, &mut budget).unwrap();
        let bytes = budget.bytes;
        assert!(
            ownership
                .get(&id("missing"), &mut budget)
                .unwrap()
                .is_none()
        );
        assert!(
            ownership
                .get(document.root(), &mut budget)
                .unwrap()
                .is_none()
        );
        assert_eq!(bytes, budget.bytes);
        let mut clone = AddressBudget {
            steps: MAX_ADDRESS_STEPS,
            bytes: 1,
        };
        assert!(clone.clone_steps(3).is_err());
        assert_eq!(clone.bytes, 1);
        assert_eq!(clone.steps, MAX_ADDRESS_STEPS);
        assert!(budget.clone_steps(MAX_DOCUMENT_DEPTH + 1).is_err());
    }

    #[test]
    fn nested_shared_play_extension_follows_only_the_selected_isolation() {
        use deadpan_core::{
            AcceptedGeneration, AssetId, AssetRecord, BridgeInterpolation, BridgeSamplingMap,
            GeneratedContentId, GeneratedObjectRef, InstancePath, OccurrenceIdentities,
            RepeatInstance, SourceSpan, SourceTimeBase, SourceTimestamp,
        };
        let mut wire = serde_json::to_value(nested_wide(1, 2)).unwrap();
        let object = |digit: char| {
            GeneratedObjectRef::new(
                GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
                100,
            )
            .unwrap()
        };
        let artifact = GeneratedArtifact {
            sampled_asset: AssetId::new("sampled").unwrap(),
            sampled_object: object('a'),
            native_asset: AssetId::new("native").unwrap(),
            native_object: object('b'),
            provenance: object('c'),
            sampling: BridgeSamplingMap::new(
                FrameRate::new(30, 1).unwrap(),
                FrameRate::new(24, 1).unwrap(),
                FrameDuration::new(5).unwrap(),
                FrameDuration::new(4).unwrap(),
                BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
            )
            .unwrap(),
            content_aspect: Some([512, 320]),
        };
        let time_base = SourceTimeBase::new(1, 1000).unwrap();
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 10_000,
                time_base,
            },
        )
        .unwrap();
        for (asset, reference, count) in [
            (&artifact.sampled_asset, &artifact.sampled_object, 4),
            (&artifact.native_asset, &artifact.native_object, 5),
        ] {
            wire["assets"][asset.as_str()] = serde_json::to_value(AssetRecord {
                label: "generated".into(),
                content_hash: reference.content().to_string(),
                video: Some(span),
                audio: None,
                still_image: false,
                frame_count: Some(FrameDuration::new(count).unwrap()),
                source_qualification: None,
            })
            .unwrap();
        }
        wire["nodes"]["hold-0"]["kind"]["recipe"]["video"] =
            serde_json::to_value(HoldVideo::Generated {
                accepted: Box::new(AcceptedGeneration {
                    artifact,
                    fallback: HoldFallback::Background,
                }),
            })
            .unwrap();
        let repeat_ids: Vec<_> = (0..2)
            .rev()
            .map(|index| id(&format!("r{index:03}{}", "r".repeat(124))))
            .collect();
        for repeat in &repeat_ids {
            wire["nodes"][repeat.as_str()]["kind"]["iterations"] =
                serde_json::to_value(IterationOrder::new(revision(repeat.as_str()), 2).unwrap())
                    .unwrap();
        }
        let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let instance = InstancePath {
            node: id("hold-0"),
            repeats: repeat_ids
                .iter()
                .map(|repeat| {
                    let NodeKind::Repeat { iterations, .. } = &before.nodes()[repeat].kind else {
                        panic!()
                    };
                    RepeatInstance {
                        node: repeat.clone(),
                        iteration: iterations.at(1).unwrap(),
                    }
                })
                .collect(),
        };
        let request = CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("extend-selected"),
            command: Command::EditOccurrence {
                instance,
                edit: OccurrenceEdit::SetHoldDuration {
                    duration: FrameDuration::new(6).unwrap(),
                },
                identities: OccurrenceIdentities {
                    nodes: (0..16)
                        .map(|index| id(&format!("isolated-{index}")))
                        .collect(),
                    marks: vec![],
                },
            },
        };
        let edit = deadpan_core::apply(&before, &request).unwrap();
        let after = edit.forward.apply(&before).unwrap();
        let births = super::super::derive(&before, &request, &after).unwrap();
        assert_eq!(births.len(), 1);
        let target = &births[0].target;
        assert_ne!(target.node, id("hold-0"));
        assert_eq!(target.repeats.len(), 2);
        assert!(
            target
                .repeats
                .iter()
                .all(|step| matches!(step.branch, RepeatEditBranch::Play { .. }))
        );
        assert_eq!(births[0].duration, FrameDuration::new(6).unwrap());
        let mut budget = AddressBudget::default();
        assert!(
            CanonicalTargets::new(&after, &mut budget)
                .unwrap()
                .matches(target, &mut budget)
                .unwrap()
        );
        assert!(matches!(&after.nodes()[&id("hold-0")].kind,
            NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. })));
        assert_eq!(edit.inverse.apply(&after).unwrap(), before);
    }
}
