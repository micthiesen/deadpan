//! Current command assertions extracted from the obsolete schema 10..15 matrices.
use super::*;
use deadpan_core::*;
use deadpan_store::generation::{ContextObservation, RelevanceObservation, RelevancePlan};

fn relevance(store: &ProjectStore, next: &RevisionId) -> Result<RelevancePlan> {
    Ok(RelevancePlan {
        from_revision: store.snapshot()?.revision_id().clone(),
        to_revision: next.clone(),
        observations: store
            .current_generation_requests()?
            .into_iter()
            .map(|request| RelevanceObservation {
                request_id: request.request_id,
                after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
                binding: request.binding,
            })
            .collect(),
    })
}

#[test]
fn current_mapping_and_edge_edits_preserve_independent_audio_and_operational_state() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut wire: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/current-source_selection.json"))?;
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 30)?,
        },
        SourceTimestamp {
            ticks: 30,
            time_base: SourceTimeBase::new(1, 30)?,
        },
    )?;
    wire["assets"]["audio"]["video"] = serde_json::to_value(span)?;
    wire["nodes"]["source"]["kind"]["source"]["video"] =
        serde_json::to_value(SourceVideo::Stream {
            asset: AssetId::new("audio")?,
            span,
        })?;
    let path = current_fixture(scratch.path(), &wire.to_string(), None)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let initial = store.snapshot()?;
    let hold = initial
        .nodes()
        .iter()
        .find_map(|(id, node)| match &node.kind {
            NodeKind::Hold { recipe } => Some((id.clone(), recipe.duration)),
            _ => None,
        })
        .unwrap();
    store.allocate_generation_request(deadpan_store::generation::GenerationRequestInput {
        request_id: deadpan_jobs::RequestId::new("retained-request")?,
        expected_revision: initial.revision_id().clone(),
        hold_id: hold.0,
        context_sha256: deadpan_jobs::Sha256::new("a".repeat(64))?,
        constraints: deadpan_jobs::HoldConstraints {
            video: deadpan_jobs::VideoSpec::new(
                hold.1,
                initial.presentation_basis().frame_rate,
                512,
                320,
            )?,
            conditioning: deadpan_jobs::ConditioningMode::Bridge,
            motion: deadpan_jobs::MotionAmount::Still,
            instructions: None,
            region_target: None,
        },
        provider: deadpan_jobs::ProviderSelection {
            pack_id: deadpan_jobs::ProviderPackId::new("pack")?,
            pack_version: deadpan_jobs::ProviderPackVersion::new("v1")?,
            runtime_id: deadpan_jobs::RuntimeId::new("runtime")?,
            runtime_version: deadpan_jobs::RuntimeVersion::new("v1")?,
            seed: 1,
        },
    })?;
    let requests = store.current_generation_requests()?;
    let node = NodeId::new("source")?;
    let commands = [
        Command::SetSourceAudioMapping {
            node: node.clone(),
            mapping: SourceAudioMapping::Duration {
                frames: ExactRatio::new(60000, 1001)?,
            },
            offset: AudioSample(-137),
        },
        Command::SetSourceVideoMapping {
            node: node.clone(),
            mapping: SourceVideoMapping::Duration {
                frames: ExactRatio::new(120000, 1001)?,
                endpoints: EndpointPolicy::Reject,
            },
        },
        Command::SetSourceVideoMapping {
            node: node.clone(),
            mapping: SourceVideoMapping::Placement {
                start: ExactRatio::new(2, 3)?,
                frames: ExactRatio::new(28750, 1001)?,
                endpoints: EndpointPolicy::HoldAdjacent,
            },
        },
        Command::EditOccurrence {
            instance: InstancePath {
                node: node.clone(),
                repeats: Vec::new(),
            },
            edit: OccurrenceEdit::SetSourceAudioMapping {
                mapping: SourceAudioMapping::Placement {
                    start: ExactRatio::new(-1, 147)?,
                    frames: ExactRatio::new(60000, 1001)?,
                },
                offset: AudioSample(-137),
            },
            identities: OccurrenceIdentities {
                nodes: Vec::new(),
                marks: Vec::new(),
            },
        },
        Command::SetAudioEdge {
            node: node.clone(),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
    ];
    for (index, command) in commands.into_iter().enumerate() {
        let baseline = store.snapshot()?;
        let next = RevisionId::new(format!("current-authoring-{index}"))?;
        let request = CommandRequest {
            project_id: baseline.project_id().clone(),
            expected_revision: baseline.revision_id().clone(),
            new_revision: next.clone(),
            command,
        };
        let outcome = store.commit_reconciled(&request, &relevance(&store, &next)?)?;
        let changed = store.snapshot()?;
        assert_eq!(outcome.edit.duration_delta, 0);
        assert_eq!(changed.duration()?, baseline.duration()?);
        assert_eq!(changed.marks(), baseline.marks());
        assert_eq!(changed.assets(), baseline.assets());
        assert_eq!(changed.presentation_basis(), baseline.presentation_basis());
        assert_eq!(changed.basis_state(), baseline.basis_state());
        assert_eq!(store.current_generation_requests()?, requests);
        let NodeKind::Source { source } = &changed.nodes()[&node].kind else {
            unreachable!()
        };
        if index == 1 {
            assert_eq!(
                source.video_mapping,
                SourceVideoMapping::Duration {
                    frames: ExactRatio::new(120000, 1001)?,
                    endpoints: EndpointPolicy::Reject
                }
            );
            assert_eq!(
                source.audio_mapping,
                SourceAudioMapping::Duration {
                    frames: ExactRatio::new(60000, 1001)?
                }
            );
            assert_eq!(source.audio_offset, AudioSample(-137));
        }
        if index == 4 {
            assert_eq!(
                changed.nodes()[&node].audio_edges.node_start,
                AudioEdgePolicy::Hard
            );
        }
        let undo = RevisionId::new(format!("undo-current-{index}"))?;
        store.undo_reconciled(
            changed.revision_id(),
            undo.clone(),
            &relevance(&store, &undo)?,
        )?;
        assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
        assert_eq!(store.snapshot()?.marks(), baseline.marks());
        let redo = RevisionId::new(format!("redo-current-{index}"))?;
        store.redo_reconciled(&undo, redo.clone(), &relevance(&store, &redo)?)?;
        assert_eq!(store.snapshot()?.nodes(), changed.nodes());
        assert_eq!(store.snapshot()?.marks(), changed.marks());
    }
    let expected = store.snapshot()?;
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?, expected);
    assert_eq!(reopened.current_generation_requests()?, requests);
    reopened.validate()?;
    Ok(())
}
