//! The local replacements for whole-document command work (shared structural
//! durations, proved intermediate scopes, reused binding proofs, merged
//! diffs, lineage closure and the binding byte bound) must be exact: random
//! edit sequences produce identical transactions, results, durations and
//! refusals with and without them (`with_reference_command_work`).

use std::collections::BTreeMap;
use std::sync::Arc;

use deadpan_core::*;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn hold(frames: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(frames).unwrap(),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}

fn timing(revision: &str, ordinal: u32) -> AudioTimingId {
    AudioTimingId {
        allocation: RevisionId::new(revision).unwrap(),
        ordinal,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    /// Background/Silence Holds only.
    Holds,
    /// Some root beats are linked Sources of one Original with audio.
    Sources,
    /// A catalog sound placed at the root and on a beat, with Hold allowances.
    Sounds,
}

fn span(start: i64, end: i64, rate: u32) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, rate).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn step(document: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
    let edit = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        },
    )
    .unwrap();
    edit.forward.apply(document).unwrap()
}

/// Replace some root Holds with Sources of one Original carrying audio.
fn with_sources(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["original"] = serde_json::to_value(AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(span(0, 240, 24)),
        audio: Some(span(0, 480_000, 48_000)),
        still_image: false,
        frame_count: None,
        source_qualification: None,
    })
    .unwrap();
    let mut start = 0;
    for name in ["b01", "b03", "b06", "b08"] {
        let NodeKind::Hold { recipe } = &document.nodes()[&id(name)].kind else {
            unreachable!("fixture Hold")
        };
        let frames = recipe.duration.frames();
        wire["nodes"][name]["kind"] = serde_json::to_value(NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: recipe.duration,
                video: SourceVideo::Stream {
                    asset: AssetId::new("original").unwrap(),
                    span: span(start, start + frames, 24),
                },
                audio: Some(SourceAudio {
                    asset: AssetId::new("original").unwrap(),
                    span: span(start * 2000, (start + frames) * 2000, 48_000),
                }),
                link: LinkRelation::Independent,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        })
        .unwrap();
        start += frames + 3;
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn catalog_span() -> SourceSpan {
    span(0, 48_000, 48_000)
}

fn sound_source() -> SourceAudio {
    SourceAudio {
        asset: AssetId::new("catalog").unwrap(),
        span: catalog_span(),
    }
}

fn root_sound(document: &ProjectDocument, offset: i64, gain: i32) -> SoundEvent {
    SoundEvent {
        owner: document.root().clone(),
        label: "Effect".into(),
        source: sound_source(),
        mapping: SourceAudioMapping::natural_rate(
            catalog_span(),
            document.presentation_basis().frame_rate,
        )
        .unwrap(),
        offset: AudioSample(offset),
        gain_millidecibels: gain,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Automatic,
        overflow: SoundOverflowPolicy::Reject,
    }
}

fn beat_sound(document: &ProjectDocument, offset: i64) -> BeatSound {
    // One frame of the catalog sound fits every beat of two or more frames.
    let selected = span(0, 2000, 48_000);
    BeatSound {
        label: "Beat effect".into(),
        source: SourceAudio {
            asset: AssetId::new("catalog").unwrap(),
            span: selected,
        },
        mapping: SourceAudioMapping::natural_rate(
            selected,
            document.presentation_basis().frame_rate,
        )
        .unwrap(),
        offset: AudioSample(offset),
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Automatic,
        overflow: SoundOverflowPolicy::Reject,
    }
}

/// Register a catalog sound, place it at the root and on a beat, and allow
/// it through one silent Hold.
fn with_sounds(document: &ProjectDocument) -> ProjectDocument {
    let document = step(
        document,
        "r3",
        Command::ImportSource {
            id: AssetId::new("catalog").unwrap(),
            asset: AssetRecord {
                label: "Effect".into(),
                content_hash: "c".repeat(64),
                video: None,
                audio: Some(catalog_span()),
                still_image: false,
                frame_count: None,
                source_qualification: Some(SourceQualificationId::new("d".repeat(64)).unwrap()),
            },
            insertion: None,
            primary: None,
        },
    );
    let event = root_sound(&document, 137, 0);
    let document = step(
        &document,
        "r4",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event,
        },
    );
    let event = beat_sound(&document, 11);
    let document = step(
        &document,
        "r5",
        Command::SetBeatSound {
            owner: id("b02"),
            id: SoundId::new("beat-effect").unwrap(),
            event,
        },
    );
    step(
        &document,
        "r6",
        Command::SetSoundAllowance {
            sound: SoundId::new("effect").unwrap(),
            issuer: SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: id("b05"),
                    repeats: Vec::new(),
                },
            },
            allowed: true,
        },
    )
}

/// Root Holds and one nested group, with retained clocks on every Hold.
fn initial(seed: u64, variant: Variant) -> ValidatedDocument {
    let document = ProjectDocument::new(
        ProjectId::new("command-work").unwrap(),
        RevisionId::new("r0").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(24, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut random = Lcg(seed);
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    for index in 0..10 {
        let node = id(&format!("b{index:02}"));
        let frames = 2 + random.below(6) as i64;
        nodes.insert(node.clone(), BeatNode::hold("Beat", hold(frames)));
        children.push(node);
    }
    let inner: Vec<NodeId> = (0..3).map(|index| id(&format!("g{index}"))).collect();
    for node in &inner {
        nodes.insert(node.clone(), BeatNode::hold("Grouped", hold(3)));
    }
    nodes.insert(id("group"), BeatNode::sequence("Group", inner));
    children.insert(4, id("group"));
    nodes.insert(id("beats"), BeatNode::sequence("Beats", children));
    let edit = apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("r1").unwrap(),
            command: Command::Insert {
                parent: id("root"),
                index: 0,
                subtree: Subtree {
                    root: id("beats"),
                    nodes,
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        },
    )
    .unwrap();
    let document = edit.forward.apply(&document).unwrap();
    let edit = apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("r2").unwrap(),
            command: Command::Ungroup { node: id("beats") },
        },
    )
    .unwrap();
    let document = edit.forward.apply(&document).unwrap();
    let document = match variant {
        Variant::Holds => document,
        Variant::Sources => with_sources(&document),
        Variant::Sounds => with_sounds(&document),
    };
    ValidatedDocument::new(Arc::new(document)).unwrap()
}

fn subtree_size(document: &ProjectDocument, node: &NodeId) -> usize {
    let mut pending = vec![node.clone()];
    let mut count = 0;
    while let Some(next) = pending.pop() {
        count += 1;
        pending.extend(document.children(&next).cloned());
    }
    count
}

fn random_command(
    document: &ValidatedDocument,
    random: &mut Lcg,
    revision: &str,
    fresh: &mut usize,
) -> Command {
    let durations = document.durations();
    let nodes: Vec<NodeId> = document.nodes().keys().cloned().collect();
    let pick = |random: &mut Lcg| nodes[random.below(nodes.len())].clone();
    let new_id = |fresh: &mut usize| {
        *fresh += 1;
        id(&format!("n{fresh}"))
    };
    let total = durations[document.root()].frames();
    let frames = |node: &NodeId| durations.get(node).map_or(0, |value| value.frames());
    match random.below(15) {
        0..=2 => {
            let at = random.below(usize::try_from(total).unwrap() + 1) as i64;
            let needed = document
                .insert_time_target(ProjectFrame(at))
                .ok()
                .and_then(|target| target.split)
                .map_or(0, |split| split.required_ids);
            Command::InsertTime {
                at: ProjectFrame(at),
                hold: hold(1 + random.below(3) as i64),
                id: new_id(fresh),
                identities: SplitIdentities {
                    nodes: (0..needed).map(|_| new_id(fresh)).collect(),
                },
                timing: timing(revision, 0),
            }
        }
        3 | 4 => {
            // Often split a beat that hosts an occurrence mark, which needs
            // Split's anchor index.
            let hosts: Vec<NodeId> = document
                .marks()
                .values()
                .flat_map(|mark| mark.bindings())
                .filter_map(|binding| match binding.coordinate {
                    Anchor::Occurrence { instance, .. } => Some(instance.node),
                    _ => None,
                })
                .filter(|node| document.nodes().contains_key(node))
                .collect();
            let node = if !hosts.is_empty() && random.chance(50) {
                hosts[random.below(hosts.len())].clone()
            } else {
                pick(random)
            };
            let span = frames(&node).max(1);
            let at = random.below(usize::try_from(span).unwrap() + 1) as i64;
            let count = subtree_size(document, &node) + 4;
            Command::Split {
                node,
                at: FrameDuration::new(at).unwrap(),
                identities: SplitIdentities {
                    nodes: (0..count).map(|_| new_id(fresh)).collect(),
                },
            }
        }
        5 => Command::WrapRepeat {
            node: pick(random),
            id: new_id(fresh),
            plays: 1 + random.below(3) as u32,
            gap: random.chance(40).then(|| hold(1 + random.below(2) as i64)),
            anchor_policy: WrapAnchorPolicy::First,
        },
        6 => Command::DeleteRipple {
            node: pick(random),
            timing: timing(revision, 0),
        },
        7 => {
            let node = pick(random);
            Command::SetHoldDuration {
                node,
                duration: FrameDuration::new(1 + random.below(6) as i64).unwrap(),
            }
        }
        8 => {
            let node = pick(random);
            let span = frames(&node);
            let coordinate = if random.chance(50) {
                // A concrete occurrence; Split relocates these through its
                // anchor index when the target contains the host.
                Anchor::Occurrence {
                    instance: InstancePath {
                        node: node.clone(),
                        repeats: Vec::new(),
                    },
                    position: ExactRatio::integer(
                        random.below(usize::try_from(span).unwrap() + 1) as i64
                    ),
                }
            } else if random.chance(30) {
                Anchor::Sequence {
                    frame: ProjectFrame(random.below(usize::try_from(total).unwrap() + 1) as i64),
                }
            } else {
                Anchor::Local {
                    node: node.clone(),
                    position: ExactRatio::integer(
                        random.below(usize::try_from(span).unwrap() + 1) as i64
                    ),
                }
            };
            *fresh += 1;
            Command::SetMark {
                id: MarkId::new(format!("m{fresh}")).unwrap(),
                owner: if random.chance(50) {
                    node
                } else {
                    document.root().clone()
                },
                label: "Mark".into(),
                boundary: BoundaryAnchor {
                    coordinate,
                    bias: if random.chance(50) {
                        InsertionBias::Left
                    } else {
                        InsertionBias::Right
                    },
                },
                loss_policy: if random.chance(50) {
                    AnchorLossPolicy::KeepUnresolved
                } else {
                    AnchorLossPolicy::DeleteOwned
                },
            }
        }
        9 => {
            let repeats: Vec<NodeId> = document
                .nodes()
                .iter()
                .filter(|(_, node)| matches!(node.kind, NodeKind::Repeat { .. }))
                .map(|(id, _)| id.clone())
                .collect();
            if repeats.is_empty() {
                return Command::Ungroup { node: pick(random) };
            }
            Command::SetRepeatPlays {
                node: repeats[random.below(repeats.len())].clone(),
                plays: 1 + random.below(4) as u32,
                timing: timing(revision, 0),
            }
        }
        10 => {
            let parent = document.root().clone();
            let count = document.children(&parent).count();
            let start = random.below(count);
            let end = (start + 1 + random.below(3)).min(count);
            if random.chance(50) {
                Command::Group {
                    parent,
                    start,
                    end,
                    id: new_id(fresh),
                    label: "Grouped".into(),
                }
            } else {
                let groups: Vec<NodeId> = document
                    .nodes()
                    .iter()
                    .filter(|(id, node)| {
                        *id != document.root() && matches!(node.kind, NodeKind::Sequence { .. })
                    })
                    .map(|(id, _)| id.clone())
                    .collect();
                Command::Ungroup {
                    node: if groups.is_empty() {
                        pick(random)
                    } else {
                        groups[random.below(groups.len())].clone()
                    },
                }
            }
        }
        12 => {
            let id = SoundId::new("effect").unwrap();
            let event = root_sound(
                document,
                random.below(2000) as i64,
                -(random.below(6000) as i32),
            );
            // A routed recipe is replaced explicitly, clearing its route.
            if document.sound_routes().contains_key(&id) {
                Command::ReplaceSound { id, event }
            } else {
                Command::SetSound { id, event }
            }
        }
        13 => Command::SetSoundAllowance {
            sound: SoundId::new("effect").unwrap(),
            issuer: SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: pick(random),
                    repeats: Vec::new(),
                },
            },
            allowed: random.chance(70),
        },
        14 => {
            let owner = pick(random);
            if random.chance(70) {
                Command::SetBeatSound {
                    owner,
                    id: SoundId::new("beat-effect").unwrap(),
                    event: beat_sound(document, random.below(500) as i64),
                }
            } else {
                Command::DeleteBeatSound {
                    owner,
                    id: SoundId::new("beat-effect").unwrap(),
                }
            }
        }
        _ => Command::WrapRetime {
            node: pick(random),
            id: new_id(fresh),
            duration: FrameDuration::new(1 + random.below(6) as i64).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
    }
}

type Outcome = Result<
    (
        EditTransaction,
        ProjectDocument,
        BTreeMap<NodeId, FrameDuration>,
    ),
    EditError,
>;

fn outcome(head: &ValidatedDocument, request: &CommandRequest) -> Outcome {
    apply_validated(head, request).map(|(edit, result)| {
        result.check_against_complete_validation().unwrap();
        (
            edit,
            ProjectDocument::clone(&result),
            result.durations().clone(),
        )
    })
}

#[derive(Default)]
struct Coverage {
    committed: BTreeMap<&'static str, usize>,
    refused: usize,
    with_lineage: usize,
    with_marks: usize,
    with_bindings: usize,
    with_sounds: usize,
    with_sources: usize,
    /// Committed Splits whose target held a bound occurrence mark.
    occurrence_splits: usize,
    refusals: BTreeMap<String, usize>,
}

fn occurrence_inside(document: &ProjectDocument, target: &NodeId) -> bool {
    let hosts: Vec<NodeId> = document
        .marks()
        .values()
        .flat_map(|mark| mark.bindings())
        .filter(|binding| binding.state == MarkState::Bound)
        .filter_map(|binding| match binding.coordinate {
            Anchor::Occurrence { instance, .. } => Some(instance.node),
            _ => None,
        })
        .collect();
    let mut pending = vec![target.clone()];
    while let Some(node) = pending.pop() {
        if hosts.contains(&node) {
            return true;
        }
        pending.extend(document.children(&node).cloned());
    }
    false
}

fn kind(command: &Command) -> &'static str {
    match command {
        Command::InsertTime { .. } => "pause",
        Command::Split { .. } => "split",
        Command::WrapRepeat { .. } => "wrap",
        Command::DeleteRipple { .. } => "delete",
        Command::SetHoldDuration { .. } => "hold duration",
        Command::SetMark { .. } => "mark",
        Command::SetRepeatPlays { .. } => "repeat plays",
        Command::Group { .. } => "group",
        Command::Ungroup { .. } => "ungroup",
        Command::WrapRetime { .. } => "retime",
        Command::SetSound { .. } | Command::ReplaceSound { .. } => "sound",
        Command::SetSoundAllowance { .. } => "allowance",
        Command::SetBeatSound { .. } | Command::DeleteBeatSound { .. } => "beat sound",
        _ => "other",
    }
}

fn run(seed: u64, steps: usize, variant: Variant, coverage: &mut Coverage) {
    let mut random = Lcg(seed);
    let mut head = initial(seed, variant);
    let mut fresh = 0;
    for step in 0..steps {
        let revision = format!("s{seed}-{step}");
        let command = random_command(&head, &mut random, &revision, &mut fresh);
        let request = CommandRequest {
            project_id: head.project_id().clone(),
            expected_revision: head.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        };
        let local = outcome(&head, &request);
        let reference = with_reference_command_work(|| outcome(&head, &request));
        assert_eq!(
            local, reference,
            "seed {seed} step {step}: {:?}",
            request.command
        );
        // The unscoped entry point takes the same local paths.
        let unscoped = apply_with_result(&head, &request);
        let unscoped_reference = with_reference_command_work(|| apply_with_result(&head, &request));
        assert_eq!(
            unscoped, unscoped_reference,
            "seed {seed} step {step} unscoped"
        );
        if let (Ok(_), Command::Split { node, .. }) = (&local, &request.command)
            && occurrence_inside(&head, node)
        {
            coverage.occurrence_splits += 1;
        }
        match local {
            Ok((edit, result, _)) => {
                assert_eq!(edit.forward.apply(&head).unwrap(), result);
                // The copy-free restore check equals applying and comparing,
                // including guard conflicts and unequal results.
                for (patch, from, to) in [
                    (&edit.forward, &*head, &result),
                    (&edit.inverse, &result, &*head),
                    (&edit.forward, &result, &*head),
                    (&edit.inverse, &*head, &result),
                    (&edit.inverse, &result, &result),
                ] {
                    let local = patch.restores(from, to);
                    let reference = with_reference_command_work(|| patch.restores(from, to));
                    assert_eq!(local, reference, "seed {seed} step {step} restores");
                }
                assert_eq!(edit.forward.restores(&head, &result), Ok(true));
                assert_eq!(edit.inverse.restores(&result, &head), Ok(true));
                *coverage
                    .committed
                    .entry(kind(&request.command))
                    .or_default() += 1;
                coverage.with_lineage += usize::from(!result.audio_lineage().is_empty());
                coverage.with_marks += usize::from(!result.marks().is_empty());
                coverage.with_bindings += usize::from(!result.audio_bindings().is_empty());
                coverage.with_sounds += usize::from(
                    !result.sounds().is_empty()
                        || !result.beat_sounds().is_empty()
                        || !result.sound_allowances().is_empty(),
                );
                coverage.with_sources += usize::from(
                    result
                        .nodes()
                        .values()
                        .any(|node| matches!(node.kind, NodeKind::Source { .. })),
                );
                head = ValidatedDocument::new(Arc::new(result)).unwrap();
            }
            Err(error) => {
                coverage.refused += 1;
                *coverage
                    .refusals
                    .entry(format!("{:?}", error.code))
                    .or_default() += 1;
            }
        }
    }
}

#[test]
fn local_command_work_equals_the_reference_on_random_edit_sequences() {
    let mut coverage = Coverage::default();
    for seed in 1..=48 {
        let variant = [Variant::Holds, Variant::Sources, Variant::Sounds][(seed % 3) as usize];
        run(seed, 40, variant, &mut coverage);
    }
    // Every command kind commits, refusals occur, and the results carry the
    // lineage, marks and retained clocks the replaced steps read.
    for kind in [
        "pause",
        "split",
        "wrap",
        "delete",
        "hold duration",
        "mark",
        "repeat plays",
        "group",
        "ungroup",
        "retime",
        "sound",
        "allowance",
        "beat sound",
    ] {
        assert!(
            coverage.committed.get(kind).copied().unwrap_or(0) >= 5,
            "{kind}: {:?}",
            coverage.committed
        );
    }
    assert!(coverage.refused > 50, "only {} refusals", coverage.refused);
    assert!(
        coverage.with_lineage > 100 && coverage.with_marks > 100 && coverage.with_bindings > 100
    );
    // Sound, beat-sound and allowance captures take the unoptimized branch;
    // Sources exercise linked audio and lineage; occurrence marks inside a
    // Split target take the anchor-index branch.
    assert!(coverage.with_sounds > 100, "{}", coverage.with_sounds);
    assert!(coverage.with_sources > 100, "{}", coverage.with_sources);
    assert!(
        coverage.occurrence_splits >= 5,
        "{}",
        coverage.occurrence_splits
    );
    // Refusals arise for distinct reasons, not one repeated failure.
    for code in [
        "InvalidCommand",
        "SelectionUnavailable",
        "WrongNodeKind",
        "SourceRangeInvalid",
    ] {
        assert!(
            coverage.refusals.get(code).copied().unwrap_or(0) >= 3,
            "{code}: {:?}",
            coverage.refusals
        );
    }
}

#[test]
fn merged_diff_matches_key_union_lookup() {
    // `DocumentPatch` maps come from the merged diff: compare against an
    // independent key-union computation on random maps.
    let mut random = Lcg(99);
    for _ in 0..200 {
        let mut before = BTreeMap::new();
        let mut after = BTreeMap::new();
        for key in 0..random.below(40) {
            if random.chance(70) {
                before.insert(key, random.below(3));
            }
            if random.chance(70) {
                after.insert(key, random.below(3));
            }
        }
        let expected: Vec<(usize, Option<usize>, Option<usize>)> = before
            .keys()
            .chain(after.keys())
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|key| before.get(key) != after.get(key))
            .map(|key| (key, before.get(&key).copied(), after.get(&key).copied()))
            .collect();
        let local = diff_for_tests(&before, &after);
        let reference = with_reference_command_work(|| diff_for_tests(&before, &after));
        assert_eq!(local, expected);
        assert_eq!(reference, expected);
    }
}

#[test]
fn binding_byte_check_equals_to_json_on_both_sides_of_the_structural_bound() {
    // A small state fits the structural bound; 12,000 newly bound Holds
    // exceed it (each binding's bound is about 5.6 KB), so the check
    // counts the state exactly. Both must agree with `to_json`.
    for beats in [12usize, 12_000] {
        let document = ProjectDocument::new(
            ProjectId::new("byte-bound").unwrap(),
            RevisionId::new("r0").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(24, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let mut nodes = BTreeMap::new();
        let mut children = Vec::new();
        for index in 0..beats {
            let node = id(&format!("hold-{index:06}"));
            nodes.insert(node.clone(), BeatNode::hold("Beat", hold(2)));
            children.push(node);
        }
        nodes.insert(id("beats"), BeatNode::sequence("Beats", children));
        let document = step(
            &document,
            "r1",
            Command::Insert {
                parent: id("root"),
                index: 0,
                subtree: Subtree {
                    root: id("beats"),
                    nodes,
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        );
        let state = capture_unbound_audio_bindings(&document, timing("capture", 0)).unwrap();
        let (bound, outcome) = binding_wire_check_for_tests(&state);
        assert_eq!(outcome, state.to_json().map(|_| ()));
        let exact = state.to_json().unwrap().len();
        let bound = bound.unwrap();
        assert!(exact <= bound);
        assert_eq!(bound > MAX_DOCUMENT_JSON_BYTES, beats == 12_000, "{bound}");
        let reference = with_reference_command_work(|| binding_wire_check_for_tests(&state).1);
        assert_eq!(outcome, reference);
    }
}
