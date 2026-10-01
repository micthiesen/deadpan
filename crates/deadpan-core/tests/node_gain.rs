use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::{Value, json};

#[path = "node_gain/legacy.rs"]
mod legacy;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn gain(value: i32) -> GainDb {
    GainDb::new(value).unwrap()
}
fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Pause",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    )
}
fn unity() -> AudioTreatments {
    AudioTreatments::from_clip_gain(ClipGain::default())
}
fn treatment(curve: GainCurve) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(
            gain(-3_000),
            false,
            vec![
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    GainRange::new(ratio(1, 3), ratio(23, 3)).unwrap(),
                    gain(-6_000),
                    vec![GainSegment::new(ratio(23, 3), gain(12_000), curve).unwrap()],
                )
                .unwrap(),
            ],
            vec![GainRange::new(ratio(11, 2), ratio(17, 3)).unwrap()],
        )
        .unwrap(),
    )
}
fn document(nodes: BTreeMap<NodeId, BeatNode>) -> ProjectDocument {
    ProjectDocument::from_json(&wire(nodes).to_string()).unwrap()
}
fn wire(nodes: BTreeMap<NodeId, BeatNode>) -> Value {
    let empty = ProjectDocument::new(
        ProjectId::new("node-gain").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire
}
fn fixture() -> ProjectDocument {
    document(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("hold")])),
        (id("hold"), hold(8)),
    ]))
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
        command,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let request = request(document, command);
    let request = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(transaction.forward.inverse(), transaction.inverse);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&serde_json::to_string(&transaction).unwrap())
            .unwrap(),
        transaction
    );
    (after, transaction)
}
fn set(document: &ProjectDocument, target: &str, treatments: AudioTreatments) -> ProjectDocument {
    edit(
        document,
        Command::SetAudioTreatments {
            node: id(target),
            treatments,
        },
    )
    .0
}
fn split(document: &ProjectDocument, target: &str, at: i64, prefix: &str) -> ProjectDocument {
    edit(
        document,
        Command::Split {
            node: id(target),
            at: frames(at),
            identities: SplitIdentities {
                nodes: (0..document.nodes().len() + 4)
                    .map(|n| id(&format!("{prefix}-{n}")))
                    .collect(),
            },
        },
    )
    .0
}

#[test]
fn treatment_is_authored_intent_with_guarded_reversible_node_patches() {
    let before = fixture();
    assert_eq!(before.schema_version(), 37);
    assert!(
        serde_json::to_value(&before).unwrap()["nodes"]["hold"]
            .get("audio_treatments")
            .is_none()
    );
    let requested = request(
        &before,
        Command::SetAudioTreatments {
            node: id("hold"),
            treatments: unity(),
        },
    );
    let transaction = apply(&before, &requested).unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    assert!(!after.nodes()[&id("hold")].audio_treatments.is_empty());
    assert_eq!(transaction.duration_delta, 0);
    assert_eq!(transaction.changed_ids, vec![id("hold")]);
    assert!(transaction.forward.audio_lineage.is_empty());
    assert!(transaction.forward.audio_bindings.is_none());
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        apply(&after, &requested).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    assert!(transaction.forward.apply(&after).is_err());
    let cleared = set(&after, "hold", AudioTreatments::default());
    assert!(cleared.nodes()[&id("hold")].audio_treatments.is_empty());
    let mut wire = serde_json::to_value(&requested).unwrap();
    wire["command"]["treatments"] = Value::Null;
    assert!(serde_json::from_value::<CommandRequest>(wire).is_err());
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["hold"]["audio_treatments"] = Value::Null;
    assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
}

// An independent structural clock walk is sufficient here: it never normalizes
// gain coordinates and accumulates each surviving provider-to-root factor once.
fn evaluated(document: &ProjectDocument, owner: &NodeId, local: ExactRatio) -> EvaluatedGain {
    let node = &document.nodes()[owner];
    let own = node.audio_treatments.evaluate(local).unwrap();
    let child = match &node.kind {
        NodeKind::Sequence { children } => {
            let durations = document.durations().unwrap();
            let mut remaining = local;
            let mut selected = None;
            for child in children {
                if remaining.compare_integer(durations[child].frames()).is_lt() {
                    selected = Some(evaluated(document, child, remaining));
                    break;
                }
                remaining = remaining
                    .checked_sub(ExactRatio::integer(durations[child].frames()))
                    .unwrap();
            }
            selected.unwrap_or(EvaluatedGain::UNITY)
        }
        NodeKind::Retime {
            child,
            mapping,
            duration,
            ..
        } => {
            let child_local = local
                .checked_mul(ratio(
                    i128::from(mapping.duration().frames()),
                    i128::from(duration.frames()),
                ))
                .unwrap()
                .checked_add(ExactRatio::integer(mapping.start().0))
                .unwrap();
            evaluated(document, child, child_local)
        }
        NodeKind::Hold { .. } | NodeKind::Source { .. } => EvaluatedGain::UNITY,
        NodeKind::Repeat { .. } => panic!("repeat is outside this oracle fixture"),
    };
    EvaluatedGain {
        millidecibels: own.millidecibels.checked_add(child.millidecibels).unwrap(),
        muted: own.muted || child.muted,
    }
}

#[test]
fn split_and_refinement_keep_exact_full_owner_envelope_clocks() {
    for curve in [
        GainCurve::Step,
        GainCurve::Linear,
        GainCurve::Smoothstep,
        GainCurve::Cubic {
            control1: gain(21_000),
            control2: gain(-24_000),
        },
    ] {
        let before = set(&fixture(), "hold", treatment(curve));
        let first = split(&before, "hold", 3, "first");
        let second = split(&first, "first-1", 2, "second");
        let root = split(&second, "root", 4, "root-cut");
        for tick in 0..192 {
            let local = ratio(tick, 24);
            let expected = evaluated(&before, before.root(), local);
            for after in [&first, &second, &root] {
                assert_eq!(
                    evaluated(after, after.root(), local),
                    expected,
                    "at {local:?}"
                );
            }
        }
        // Adding gain to the partition itself makes it an owner whose origin
        // must survive the next cut instead of being discarded by refinement.
        let treated_partition = set(&first, "first-1", treatment(curve));
        let refined = split(&treated_partition, "first-1", 2, "treated");
        assert_eq!(
            refined.nodes()[&id("first-1")].audio_treatments,
            treatment(curve)
        );
        for tick in 0..192 {
            let local = ratio(tick, 24);
            assert_eq!(
                evaluated(&refined, refined.root(), local),
                evaluated(&treated_partition, treated_partition.root(), local)
            );
        }
    }
}

#[test]
fn copies_and_isolated_repeat_occurrences_keep_independent_gain_intent() {
    let before = set(&fixture(), "hold", treatment(GainCurve::Smoothstep));
    let copies = split(&before, "hold", 3, "copy");
    let changed = set(&copies, "hold", unity());
    assert_eq!(
        changed.nodes()[&id("copy-2")].audio_treatments,
        treatment(GainCurve::Smoothstep)
    );
    let repeated = edit(
        &before,
        Command::WrapRepeat {
            node: id("hold"),
            id: id("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
    )
    .0;
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let selected = iterations.at(1).unwrap();
    let (isolated, transaction) = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("hold"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: selected.clone(),
                }],
            },
            edit: OccurrenceEdit::SetAudioTreatments {
                treatments: unity(),
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("isolated")],
                marks: vec![],
            },
        },
    );
    assert_eq!(
        isolated.nodes()[&id("hold")].audio_treatments,
        treatment(GainCurve::Smoothstep)
    );
    assert_eq!(isolated.nodes()[&id("isolated")].audio_treatments, unity());
    assert_eq!(
        isolated.overrides()[&id("repeat")].get(&selected),
        Some(&id("isolated"))
    );
    assert_eq!(transaction.duration_delta, 0);
    let resized = edit(
        &isolated,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 5,
            gap: None,
        },
    )
    .0;
    assert_eq!(resized.nodes()[&id("isolated")].audio_treatments, unity());
}

#[test]
fn duration_changes_and_internal_insertions_retain_fixed_keys_and_hidden_suffix() {
    let recipe = treatment(GainCurve::Smoothstep);
    let before = set(&fixture(), "hold", recipe.clone());
    let shorter = edit(
        &before,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: frames(2),
        },
    )
    .0;
    let restored = edit(
        &shorter,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: frames(8),
        },
    )
    .0;
    assert_eq!(shorter.nodes()[&id("hold")].audio_treatments, recipe);
    assert_eq!(restored.nodes()[&id("hold")].audio_treatments, recipe);
    assert_eq!(
        evaluated(&restored, restored.root(), ratio(19, 3)),
        evaluated(&before, before.root(), ratio(19, 3))
    );
    let grouped = edit(
        &fixture(),
        Command::Group {
            parent: id("root"),
            start: 0,
            end: 1,
            id: id("group"),
            label: "Gain owner".into(),
        },
    )
    .0;
    let grouped = set(&grouped, "group", recipe.clone());
    let inserted = edit(
        &grouped,
        Command::Insert {
            parent: id("group"),
            index: 0,
            subtree: Subtree {
                root: id("inserted"),
                nodes: BTreeMap::from([(id("inserted"), hold(2))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0;
    assert_eq!(inserted.nodes()[&id("group")].audio_treatments, recipe);
    for doc in [&grouped, &inserted] {
        assert!(
            apply(doc, &request(doc, Command::Ungroup { node: id("group") }))
                .unwrap_err()
                .message
                .contains("audio treatments")
        );
    }
    let unity_group = set(&grouped, "group", unity());
    assert!(
        apply(
            &unity_group,
            &request(&unity_group, Command::Ungroup { node: id("group") })
        )
        .is_err()
    );
}

#[test]
fn postmapping_gain_preserves_raw_lineage_and_preserve_sample_bindings() {
    let retimed = edit(
        &fixture(),
        Command::WrapRetime {
            node: id("hold"),
            id: id("preserve"),
            duration: frames(11),
            pitch: PitchPolicy::Preserve,
        },
    )
    .0;
    let split = split(&retimed, "preserve", 5, "part");
    assert!(!split.audio_lineage().is_empty());
    let bindings = capture_unbound_audio_bindings(
        &split,
        AudioTimingId {
            allocation: RevisionId::new("timing").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    assert!(!bindings.is_empty());
    let mut wire = serde_json::to_value(&split).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let (after, transaction) = edit(
        &before,
        Command::SetAudioTreatments {
            node: id("preserve"),
            treatments: treatment(GainCurve::Linear),
        },
    );
    assert_eq!(after.audio_lineage(), before.audio_lineage());
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert_eq!(
        FrozenAudioLayout::capture(&after).unwrap(),
        FrozenAudioLayout::capture(&before).unwrap()
    );
    assert!(transaction.forward.audio_lineage.is_empty());
    assert!(transaction.forward.audio_bindings.is_none());
}

fn dense() -> AudioTreatments {
    let curve = GainCurve::Cubic {
        control1: gain(-12_000),
        control2: gain(9_000),
    };
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ExactRatio::ZERO, ExactRatio::integer(64)).unwrap(),
        gain(0),
        (1..=64)
            .map(|n| GainSegment::new(ExactRatio::integer(n), gain(0), curve).unwrap())
            .collect(),
    )
    .unwrap();
    AudioTreatments::from_clip_gain(
        ClipGain::new(gain(0), false, vec![envelope; MAX_GAIN_ENVELOPES], vec![]).unwrap(),
    )
}
fn dense_nodes(count: usize) -> BTreeMap<NodeId, BeatNode> {
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    for n in 0..count {
        let child = id(&format!("dense-{n}"));
        let mut node = hold(8);
        node.audio_treatments = dense();
        nodes.insert(child.clone(), node);
        children.push(child);
    }
    nodes.insert(id("dense-root"), BeatNode::sequence("Dense", children));
    nodes
}

#[test]
fn aggregate_gain_admission_bounds_documents_subtrees_and_both_patch_sides() {
    let count = MAX_GAIN_RECORDS / dense().record_count() + 1;
    let nodes = dense_nodes(count);
    let before = fixture();
    let subtree = Subtree {
        root: id("dense-root"),
        nodes: nodes.clone(),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
    };
    for command in [
        Command::Insert {
            parent: id("root"),
            index: 1,
            subtree: subtree.clone(),
        },
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("root"),
                repeats: vec![],
            },
            edit: OccurrenceEdit::Insert {
                index: 1,
                subtree: subtree.clone(),
            },
            identities: OccurrenceIdentities::default(),
        },
    ] {
        let requested = request(&before, command);
        assert_eq!(
            apply(&before, &requested).unwrap_err().code,
            EditErrorCode::LimitExceeded
        );
        assert!(
            serde_json::from_str::<CommandRequest>(&serde_json::to_string(&requested).unwrap())
                .is_err()
        );
    }
    let mut document_nodes = nodes.clone();
    document_nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("dense-root")]),
    );
    let error = ProjectDocument::from_json(&wire(document_nodes).to_string()).unwrap_err();
    assert!(error.message.contains("gain record limit"));
    let (_, transaction) = edit(
        &before,
        Command::Rename {
            node: id("hold"),
            label: "Changed".into(),
        },
    );
    for before_side in [true, false] {
        let mut patch = transaction.forward.clone();
        patch.nodes = nodes
            .iter()
            .map(|(id, node)| {
                (
                    id.clone(),
                    ValueChange {
                        before: before_side.then(|| node.clone()),
                        after: (!before_side).then(|| node.clone()),
                    },
                )
            })
            .collect();
        assert!(patch.apply(&before).unwrap_err().message.contains("gain"));
        assert!(
            serde_json::from_str::<DocumentPatch>(&serde_json::to_string(&patch).unwrap()).is_err()
        );
    }
    // A bounded payload can still cross the document aggregate after combining
    // with existing owners, and that combined state must be rejected as well.
    let mut allowed = dense_nodes(count - 1);
    allowed.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("dense-root")]),
    );
    let allowed = document(allowed);
    let mut extra = hold(1);
    extra.audio_treatments = dense();
    assert!(
        apply(
            &allowed,
            &request(
                &allowed,
                Command::Insert {
                    parent: id("root"),
                    index: 1,
                    subtree: Subtree {
                        root: id("extra"),
                        nodes: BTreeMap::from([(id("extra"), extra)]),
                        overrides: BTreeMap::new(),
                        gap_overrides: BTreeMap::new(),
                    }
                }
            )
        )
        .is_err()
    );
}

#[test]
fn explicit_unity_owners_count_toward_sixteen_active_layers() {
    let chain = |count: usize| {
        let mut nodes = BTreeMap::new();
        for n in 0..count {
            let current = if n == 0 {
                id("root")
            } else {
                id(&format!("layer-{n}"))
            };
            let mut node = if n + 1 == count {
                hold(8)
            } else {
                BeatNode::sequence("Layer", vec![id(&format!("layer-{}", n + 1))])
            };
            node.audio_treatments = unity();
            nodes.insert(current, node);
        }
        nodes
    };
    assert!(ProjectDocument::from_json(&wire(chain(MAX_GAIN_LAYERS)).to_string()).is_ok());
    assert!(ProjectDocument::from_json(&wire(chain(MAX_GAIN_LAYERS + 1)).to_string()).is_err());
    let mut nodes = chain(MAX_GAIN_LAYERS + 1);
    nodes.get_mut(&id("layer-16")).unwrap().audio_treatments = AudioTreatments::default();
    let before = document(nodes);
    assert!(
        apply(
            &before,
            &request(
                &before,
                Command::SetAudioTreatments {
                    node: id("layer-16"),
                    treatments: unity()
                }
            )
        )
        .is_err()
    );
}

fn insert_pause(before: &ProjectDocument, at: i64, name: &str) -> ProjectDocument {
    let new_revision = RevisionId::new(format!("{}x", before.revision_id())).unwrap();
    let NodeKind::Hold { recipe } = hold(1).kind else {
        unreachable!()
    };
    edit(
        before,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: recipe,
            id: id(name),
            identities: SplitIdentities {
                nodes: (0..before.nodes().len() + 4)
                    .map(|index| id(&format!("{name}-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: new_revision,
                ordinal: 0,
            },
        },
    )
    .0
}

#[test]
fn repeated_pause_insertion_preserves_treated_partition_origins() {
    let original = set(&fixture(), "hold", treatment(GainCurve::Smoothstep));
    let partitioned = split(&original, "hold", 3, "split");
    let before = set(
        &partitioned,
        "split-1",
        treatment(GainCurve::Cubic {
            control1: gain(18_000),
            control2: gain(-24_000),
        }),
    );
    let once = insert_pause(&before, 5, "pause-one");
    let twice = insert_pause(&once, 7, "pause-two");
    assert_eq!(twice.duration().unwrap(), frames(10));
    assert_eq!(
        twice.nodes()[&id("split-1")].audio_treatments,
        before.nodes()[&id("split-1")].audio_treatments
    );
    for tick in 0..192 {
        let old = ratio(tick, 24);
        let shifted = if old.compare_integer(6).is_ge() {
            old.checked_add(ExactRatio::integer(2)).unwrap()
        } else if old.compare_integer(5).is_ge() {
            old.checked_add(ExactRatio::ONE).unwrap()
        } else {
            old
        };
        assert_eq!(
            evaluated(&twice, twice.root(), shifted),
            evaluated(&before, before.root(), old)
        );
    }
}

#[test]
fn replacement_and_isolation_bound_transient_gain_without_rejecting_valid_final_inventory() {
    let count = MAX_GAIN_RECORDS / dense().record_count();
    let mut nodes = dense_nodes(count);
    let dense_root = nodes.remove(&id("dense-root")).unwrap();
    nodes.insert(id("root"), dense_root);
    let before = document(nodes);
    let repeated = edit(
        &before,
        Command::WrapRepeat {
            node: id("dense-0"),
            id: id("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
    )
    .0;
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let iteration = iterations.at(0).unwrap();
    let isolated = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("dense-0"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: iteration.clone(),
                }],
            },
            edit: OccurrenceEdit::SetAudioTreatments {
                treatments: AudioTreatments::default(),
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("cleared-copy")],
                marks: vec![],
            },
        },
    )
    .0;
    assert!(
        isolated.nodes()[&id("cleared-copy")]
            .audio_treatments
            .is_empty()
    );
    assert_eq!(isolated.nodes()[&id("dense-0")].audio_treatments, dense());

    // Fill the entire budget with the old override, then replace that subtree
    // with the same recipes under fresh identities in one transaction.
    let mut nodes = BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("repeat")])),
        (id("plain"), hold(8)),
        (
            id("repeat"),
            BeatNode {
                label: "Repeat".into(),
                framing: None,
                audio_edges: Default::default(),
                audio_treatments: Default::default(),
                kind: NodeKind::Repeat {
                    child: id("plain"),
                    iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
                    gap: None,
                },
            },
        ),
    ]);
    nodes.extend(dense_nodes(count));
    let iteration = IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal: 0,
    };
    let mut before_wire = wire(nodes);
    before_wire["overrides"] = json!({"repeat": PlayOverrides::try_from(vec![PlayOverride { iteration: iteration.clone(), root: id("dense-root") }]).unwrap()});
    let before = ProjectDocument::from_json(&before_wire.to_string()).unwrap();
    let mut replacements = dense_nodes(count)
        .into_iter()
        .map(|(old, mut node)| {
            if let NodeKind::Sequence { children } = &mut node.kind {
                for child in children {
                    *child = id(&format!("new-{child}"));
                }
            }
            (id(&format!("new-{old}")), node)
        })
        .collect::<BTreeMap<_, _>>();
    // Change one recipe too, proving a real replacement rather than a no-op.
    replacements
        .get_mut(&id("new-dense-0"))
        .unwrap()
        .audio_treatments = unity();
    let after = edit(
        &before,
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration,
            subtree: Subtree {
                root: id("new-dense-root"),
                nodes: replacements,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0;
    assert!(!after.nodes().contains_key(&id("dense-root")));
    assert_eq!(after.nodes()[&id("new-dense-0")].audio_treatments, unity());
}
