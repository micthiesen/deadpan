use super::*;
use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn sound() -> SoundId {
    SoundId::new("event").unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn hold() -> BeatNode {
    BeatNode::hold(
        "Silent picture",
        HoldRecipe {
            duration: FrameDuration::new(4).unwrap(),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

fn fixture() -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("clock-cache").unwrap(),
        revision("before"),
        PresentationBasis {
            width: 64,
            height: 64,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    let mut children = vec![id("owner-a"), id("owner-b")];
    children.extend((0..24).map(|index| id(&format!("filler-{index}"))));
    let mut nodes = children
        .iter()
        .map(|child| (child.clone(), hold()))
        .collect::<BTreeMap<_, _>>();
    nodes.insert(
        id("scope"),
        BeatNode::sequence("Shared processing scope", children),
    );
    nodes.insert(id("root"), BeatNode::sequence("Root", vec![id("scope")]));
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
        SourceTimestamp {
            ticks: 1024,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
    )
    .unwrap();
    let asset = AssetId::new("sound").unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset.clone(),
        AssetRecord {
            label: "Sound contract".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    let event = BeatSound {
        label: "Independent sound".into(),
        source: SourceAudio { asset, span },
        mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
        offset: AudioSample(0),
        treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([
        (id("owner-a"), BTreeMap::from([(sound(), event.clone())])),
        (id("owner-b"), BTreeMap::from([(sound(), event)])),
    ]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let timing = AudioTimingId {
        allocation: revision("capture"),
        ordinal: 0,
    };
    let journals = ["owner-a", "owner-b"]
        .into_iter()
        .map(|owner| {
            (
                id(owner),
                BTreeMap::from([(
                    sound(),
                    SoundClockJournal::new(
                        id("scope"),
                        vec![SoundClockReference::new(
                            timing.clone(),
                            id("scope"),
                            id(owner),
                        )],
                    )
                    .unwrap(),
                )]),
            )
        })
        .collect();
    let bindings = AudioBindingState::new_with_sound_clocks(
        vec![AudioTimingRecord {
            id: timing,
            layout: FrozenAudioLayout::capture(&document).unwrap(),
        }],
        BTreeMap::new(),
        BTreeMap::new(),
        journals,
    )
    .unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn repeated_small_queries_reuse_one_complete_proof_across_event_owners() {
    let plan = RenderPlan::compile(&fixture()).unwrap();
    let a = plan
        .beat_sound_clock_scopes(&id("owner-a"), &sound(), 12)
        .unwrap();
    let b = plan
        .beat_sound_clock_scopes(&id("owner-b"), &sound(), 12)
        .unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].proof_work(), 12);
    assert!(a[0].proof_construction_work() > 12);
    assert!(Arc::ptr_eq(&a[0].correspondence, &b[0].correspondence));
    for _ in 0..16 {
        let again = plan
            .beat_sound_clock_scopes(&id("owner-a"), &sound(), 12)
            .unwrap();
        assert!(Arc::ptr_eq(&a[0].correspondence, &again[0].correspondence));
        assert_eq!(again[0].proof_work(), 12);
    }
    for work in [0, 11, MAX_DOCUMENT_NODES + 1] {
        assert!(
            plan.beat_sound_clock_scopes(&id("owner-a"), &sound(), work)
                .is_err()
        );
    }
    // Query reuse does not move the full proof work outside compilation's
    // aggregate bound. This budget can cover the proof alone, not its snapshot
    // plus inventory and relation setup.
    assert!(compile_bounded(&plan, a[0].proof_construction_work()).is_err());
}

fn replace_second_reference(plan: &RenderPlan, reference: SoundClockReference) -> RenderPlan {
    let mut forged = plan.clone();
    let mut journals = plan.audio_bindings.sound_clocks().clone();
    journals.get_mut(&id("owner-b")).unwrap().insert(
        sound(),
        SoundClockJournal::new(id("scope"), vec![reference]).unwrap(),
    );
    forged.audio_bindings = AudioBindingState::new_with_sound_clocks(
        plan.audio_bindings
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        BTreeMap::new(),
        BTreeMap::new(),
        journals,
    )
    .unwrap();
    forged
}

#[test]
fn shared_cache_keys_cannot_hide_changed_certificates_or_wrong_owner_aliases() {
    let plan = RenderPlan::compile(&fixture()).unwrap();
    let reference =
        plan.audio_bindings.sound_clocks()[&id("owner-b")][&sound()].clocks()[0].clone();
    let split = SoundClockSplit::new(
        id("owner-b"),
        ["left", "right"]
            .map(|name| SoundClockSplitPiece {
                partition: id(name),
                full_owner: id(&format!("{name}-full")),
            })
            .to_vec(),
    )
    .unwrap();
    let isolation = SoundClockIsolation {
        live_repeat: id("scope"),
        historical_repeat: id("scope"),
        iteration: IterationId {
            allocation: revision("plays"),
            ordinal: 0,
        },
        live_root: id("owner-b"),
    };
    let malformed = [
        reference
            .clone()
            .with_introduced_groups(BTreeSet::from([id("owner-b")]))
            .unwrap(),
        reference
            .clone()
            .with_introduced_splits(vec![split])
            .unwrap(),
        reference
            .clone()
            .with_introduced_isolations(vec![isolation])
            .unwrap(),
        SoundClockReference::new(
            reference.timing().clone(),
            reference.scope().clone(),
            id("owner-a"),
        ),
    ];
    for reference in malformed {
        let forged = replace_second_reference(&plan, reference);
        assert!(compile(&forged).is_err());
    }
    // Fault injection above does not mutate the original immutable inventory.
    assert!(
        plan.beat_sound_clock_scopes(&id("owner-b"), &sound(), 12)
            .is_ok()
    );
}

#[test]
fn a_recompiled_revision_keeps_its_own_scope_authority() {
    let before = fixture();
    let old_plan = RenderPlan::compile(&before).unwrap();
    let transaction = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("after"),
            command: Command::Rename {
                node: id("owner-a"),
                label: "Changed label".into(),
            },
        },
    )
    .unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    let new_plan = RenderPlan::compile(&after).unwrap();
    let old = old_plan
        .beat_sound_clock_scopes(&id("owner-a"), &sound(), 12)
        .unwrap();
    let new = new_plan
        .beat_sound_clock_scopes(&id("owner-a"), &sound(), 12)
        .unwrap();
    assert!(!Arc::ptr_eq(&old[0].correspondence, &new[0].correspondence));
    let context = new_plan
        .audio_sound_instance_context(
            &InstancePath {
                node: id("owner-a"),
                repeats: vec![],
            },
            Default::default(),
        )
        .unwrap();
    assert!(
        old[0]
            .try_remap_context_with_work(&context, MAX_DOCUMENT_NODES)
            .is_err()
    );
    assert!(
        new[0]
            .try_remap_context_with_work(&context, MAX_DOCUMENT_NODES)
            .unwrap()
            .0
            .is_some()
    );
}

#[test]
fn retained_audio_plan_constructors_have_no_sound_clock_inventory() {
    let document = fixture();
    let plan = RenderPlan::compile(&document).unwrap();
    let layout = plan.audio_bindings.timings().values().next().unwrap();
    let retained = plan
        .compile_sound_processing_layout(layout, &BTreeSet::from([AssetId::new("sound").unwrap()]))
        .unwrap();
    assert!(retained.sound_clock_correspondences.is_empty());
    assert!(
        retained
            .beat_sound_clock_scopes(&id("owner-a"), &sound(), 12)
            .unwrap()
            .is_empty()
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(AudioBindingState::default()).unwrap();
    wire["beat_sounds"] = serde_json::json!({});
    let physical = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let context = FrozenAudioContext::capture(&physical).unwrap();
    let retained = RenderPlan::compile_audio_context(&context).unwrap();
    assert!(retained.sound_clock_correspondences.is_empty());
}
