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

fn split_delivery_fixture() -> ProjectDocument {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    for owner in ["owner-a", "owner-b"] {
        wire["audio_bindings"]["sound_clocks"][owner]["event"]["scope"] = serde_json::json!(owner);
        wire["audio_bindings"]["sound_clocks"][owner]["event"]["clocks"][0]["scope"] =
            serde_json::json!(owner);
    }
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let transaction = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("split-delivery"),
            command: Command::Split {
                node: id("owner-a"),
                at: FrameDuration::new(2).unwrap(),
                identities: SplitIdentities {
                    nodes: ["left", "right", "copy"].map(id).to_vec(),
                },
            },
        },
    )
    .unwrap();
    transaction.forward.apply(&before).unwrap()
}

#[test]
fn current_delivery_family_crosses_the_root_without_expanding_clock_scope() {
    let document = split_delivery_fixture();
    let plan = RenderPlan::compile(&document).unwrap();
    assert!(
        plan.sound_delivery_forests
            .contains_key(&(id("owner-a"), sound()))
    );
    assert!(
        plan.sound_delivery_forests
            .contains_key(&(id("copy"), sound()))
    );
    let limits = AudioQueryLimits::default();
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: id("root"),
                repeats: vec![],
            },
            limits,
        )
        .unwrap();
    let family = |owner: &str| {
        let owner = id(owner);
        let event = &document.beat_sounds()[&owner][&sound()];
        let scopes = plan
            .beat_sound_clock_scopes(&owner, &sound(), MAX_DOCUMENT_NODES)
            .unwrap();
        assert!(scopes.iter().all(|scope| {
            scope
                .try_remap_context_with_work(&context, MAX_DOCUMENT_NODES)
                .unwrap()
                .0
                .is_none()
        }));
        let occurrence = context
            .source_voice_occurrence(
                InstancePath {
                    node: owner,
                    repeats: vec![],
                },
                crate::AudioSourceVoiceRecipe {
                    source: event.source.clone(),
                    mapping: event.mapping,
                    offset: event.offset,
                },
                limits,
            )
            .unwrap();
        occurrence
            .split_permission_family(&scopes, &context, limits)
            .unwrap()
            .0
            .unwrap()
    };
    assert_eq!(family("owner-a"), family("copy"));
}

#[test]
fn incomplete_or_reordered_delivery_forests_cannot_join_on_lineage() {
    let document = split_delivery_fixture();
    for children in [
        serde_json::json!(["right", "left", "owner-b"]),
        serde_json::json!(["left", "owner-b", "right"]),
    ] {
        let mut wire = serde_json::to_value(&document).unwrap();
        let mut sequence = children.as_array().unwrap().clone();
        sequence.extend((0..24).map(|i| serde_json::json!(format!("filler-{i}"))));
        wire["nodes"]["scope"]["kind"]["children"] = serde_json::Value::Array(sequence);
        let changed = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let plan = RenderPlan::compile(&changed).unwrap();
        assert!(plan.sound_clock_layout.is_some());
        assert!(plan.sound_delivery_forests.is_empty());
    }
}

#[test]
fn rejected_full_context_attempt_reports_its_consumed_work() {
    let document = fixture();
    let before = FrozenAudioLayout::capture(&document).unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(AudioBindingState::default()).unwrap();
    // Preserve the complete parent's duration and change only its last two
    // children, so the mismatch is encountered after the preceding pairs.
    wire["nodes"]["filler-22"]["kind"]["recipe"]["duration"] = serde_json::json!(3);
    wire["nodes"]["filler-23"]["kind"]["recipe"]["duration"] = serde_json::json!(5);
    let changed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let current = FrozenAudioLayout::capture(&changed).unwrap();
    let repeats = SoundClockRepeatMap::default();
    let (attempt, used) = before.sound_clock_correspondence_attempt(
        &current,
        (&id("scope"), &id("scope")),
        &repeats,
        MAX_DOCUMENT_NODES,
    );
    assert!(attempt.is_err());
    assert!(used > document.nodes().len());
    let (success, success_work) = before.sound_clock_correspondence_attempt(
        &before,
        (&id("scope"), &id("scope")),
        &repeats,
        MAX_DOCUMENT_NODES,
    );
    assert_eq!(success.unwrap().work(), success_work);
    let (limited, limit_work) = before.sound_clock_correspondence_attempt(
        &current,
        (&id("scope"), &id("scope")),
        &repeats,
        used - 1,
    );
    assert!(limited.is_err());
    assert!(limit_work < used);
}

#[test]
fn independent_complete_forests_do_not_share_a_delivery_family() {
    let document = split_delivery_fixture();
    let mut wire = serde_json::to_value(&document).unwrap();
    for (old, new) in [
        ("left", "other-left"),
        ("right", "other-right"),
        ("owner-a", "other-owner"),
        ("copy", "other-copy"),
    ] {
        wire["nodes"][new] = wire["nodes"][old].clone();
    }
    wire["nodes"]["other-left"]["kind"]["child"] = serde_json::json!("other-owner");
    wire["nodes"]["other-right"]["kind"]["child"] = serde_json::json!("other-copy");
    for (old, new) in [("owner-a", "other-owner"), ("copy", "other-copy")] {
        wire["audio_lineage"][new] = wire["audio_lineage"][old].clone();
        wire["beat_sounds"][new] = wire["beat_sounds"][old].clone();
        wire["audio_bindings"]["sound_clocks"][new] =
            wire["audio_bindings"]["sound_clocks"][old].clone();
        wire["audio_bindings"]["sound_clocks"][new]["event"]["scope"] = serde_json::json!(new);
    }
    let children = wire["nodes"]["scope"]["kind"]["children"]
        .as_array_mut()
        .unwrap();
    children.extend([
        serde_json::json!("other-left"),
        serde_json::json!("other-right"),
    ]);
    let copied = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&copied).unwrap();
    let limits = AudioQueryLimits::default();
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: id("root"),
                repeats: vec![],
            },
            limits,
        )
        .unwrap();
    let family = |owner: &str| {
        let owner = id(owner);
        let event = &copied.beat_sounds()[&owner][&sound()];
        let voice = context
            .source_voice_occurrence(
                InstancePath {
                    node: owner.clone(),
                    repeats: vec![],
                },
                crate::AudioSourceVoiceRecipe {
                    source: event.source.clone(),
                    mapping: event.mapping,
                    offset: event.offset,
                },
                limits,
            )
            .unwrap();
        let clocks = plan
            .beat_sound_clock_scopes(&owner, &sound(), MAX_DOCUMENT_NODES)
            .unwrap();
        voice
            .split_permission_family(&clocks, &context, limits)
            .unwrap()
            .0
            .unwrap()
    };
    assert_eq!(family("owner-a"), family("copy"));
    assert_eq!(family("other-owner"), family("other-copy"));
    assert_ne!(family("owner-a"), family("other-owner"));
}

#[test]
fn delivery_proofs_use_the_clock_compilers_remaining_allowance() {
    let mut plan = RenderPlan::compile(&split_delivery_fixture()).unwrap();
    let (_, _, remaining) = compile(&plan).unwrap();
    let used = MAX_AUDIO_BINDING_ENTRIES - remaining;
    assert!(used > 0);
    let (proofs, _, small_remainder) = compile_bounded(&plan, used + 1).unwrap();
    assert_eq!(small_remainder, 1);
    plan.sound_clock_correspondences = proofs;
    assert!(super::super::sound_delivery_forest::compile(&plan, small_remainder).is_err());
    assert!(
        !super::super::sound_delivery_forest::compile(&plan, remaining)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn one_current_forest_keeps_concrete_repeat_plays_in_separate_families() {
    let mut wire = serde_json::to_value(split_delivery_fixture()).unwrap();
    let iterations = IterationOrder::new(revision("outer-plays"), 2).unwrap();
    let mut repeat = BeatNode::sequence("Two plays", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("scope"),
        iterations: iterations.clone(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["plays"] = serde_json::to_value(repeat).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["plays"]);
    let repeats = serde_json::to_value(
        SoundClockRepeatMap::new(vec![SoundClockRepeatStep::Introduced {
            live_repeat: id("plays"),
            plays: iterations.clone(),
        }])
        .unwrap(),
    )
    .unwrap();
    for events in wire["audio_bindings"]["sound_clocks"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        for journal in events.as_object_mut().unwrap().values_mut() {
            for clock in journal["clocks"].as_array_mut().unwrap() {
                clock["repeats"] = repeats.clone();
            }
        }
    }
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&document).unwrap();
    let limits = AudioQueryLimits::default();
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: id("root"),
                repeats: vec![],
            },
            limits,
        )
        .unwrap();
    let family = |owner: &str, ordinal| {
        let owner = id(owner);
        let event = &document.beat_sounds()[&owner][&sound()];
        let voice = context
            .source_voice_occurrence(
                InstancePath {
                    node: owner.clone(),
                    repeats: vec![RepeatInstance {
                        node: id("plays"),
                        iteration: IterationId {
                            allocation: revision("outer-plays"),
                            ordinal,
                        },
                    }],
                },
                crate::AudioSourceVoiceRecipe {
                    source: event.source.clone(),
                    mapping: event.mapping,
                    offset: event.offset,
                },
                limits,
            )
            .unwrap();
        let clocks = plan
            .beat_sound_clock_scopes(&owner, &sound(), MAX_DOCUMENT_NODES)
            .unwrap();
        voice
            .split_permission_family(&clocks, &context, limits)
            .unwrap()
            .0
            .unwrap()
    };
    assert_eq!(family("owner-a", 0), family("copy", 0));
    assert_eq!(family("owner-a", 1), family("copy", 1));
    assert_ne!(family("owner-a", 0), family("copy", 1));
}

fn sound_processing_inventory_fixture()
-> (ProjectDocument, AudioTimingId, AudioTimingId, AudioTimingId) {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    let second_asset = wire["assets"]["sound"].clone();
    wire["assets"]["sound-b"] = second_asset;
    wire["beat_sounds"]["owner-b"]["event"]["source"]["asset"] =
        serde_json::json!(AssetId::new("sound-b").unwrap());
    wire["audio_bindings"] = serde_json::to_value(AudioBindingState::default()).unwrap();
    let bare = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let layout = FrozenAudioLayout::capture(&bare).unwrap();
    let timing_a = AudioTimingId {
        allocation: revision("inventory"),
        ordinal: 0,
    };
    let timing_b = AudioTimingId {
        allocation: revision("inventory"),
        ordinal: 1,
    };
    let unreferenced = AudioTimingId {
        allocation: revision("inventory"),
        ordinal: 2,
    };
    let journals = BTreeMap::from([
        (
            id("owner-a"),
            BTreeMap::from([(
                sound(),
                SoundClockJournal::new(
                    id("scope"),
                    vec![
                        SoundClockReference::new(timing_a.clone(), id("scope"), id("owner-a")),
                        SoundClockReference::new(timing_b.clone(), id("scope"), id("owner-a")),
                    ],
                )
                .unwrap(),
            )]),
        ),
        (
            id("owner-b"),
            BTreeMap::from([(
                sound(),
                SoundClockJournal::new(
                    id("scope"),
                    vec![SoundClockReference::new(
                        timing_a.clone(),
                        id("scope"),
                        id("owner-b"),
                    )],
                )
                .unwrap(),
            )]),
        ),
    ]);
    let bindings = AudioBindingState::new_with_sound_clocks(
        vec![
            AudioTimingRecord {
                id: timing_a.clone(),
                layout: layout.clone(),
            },
            AudioTimingRecord {
                id: timing_b.clone(),
                layout,
            },
        ],
        BTreeMap::new(),
        BTreeMap::new(),
        journals,
    )
    .unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        timing_a,
        timing_b,
        unreferenced,
    )
}

#[test]
fn processing_asset_inventory_is_complete_deduplicated_and_timing_scoped() {
    let (document, timing_a, timing_b, unreferenced) = sound_processing_inventory_fixture();
    let plan = RenderPlan::compile(&document).unwrap();

    assert_eq!(
        plan.sound_processing_assets(&timing_a),
        Some(&BTreeSet::from([
            AssetId::new("sound").unwrap(),
            AssetId::new("sound-b").unwrap(),
        ]))
    );
    // owner-a's same source is referenced by two clocks; each timing keeps a
    // set, so the repeated asset is represented once in both unions.
    assert_eq!(
        plan.sound_processing_assets(&timing_b),
        Some(&BTreeSet::from([AssetId::new("sound").unwrap()]))
    );
    assert!(plan.sound_processing_assets(&unreferenced).is_none());
    assert!(plan.sound_processing_layout(&timing_a).is_some());
    assert!(plan.sound_processing_layout(&unreferenced).is_none());
}

#[test]
fn processing_asset_inventory_insertion_uses_the_shared_compile_budget() {
    let plan = RenderPlan::compile(&sound_processing_inventory_fixture().0).unwrap();
    let layout = plan.sound_clock_layout.as_deref().unwrap();
    let baseline = layout.nodes().len()
        + layout.audio_lineage().len()
        + layout
            .nodes()
            .values()
            .map(|node| match &node.kind {
                FrozenAudioKind::Repeat { iterations, .. } => iterations.segment_count(),
                _ => 0,
            })
            .sum::<usize>();
    // The first reference traversal fits, but its new timing/asset membership
    // must also be charged before insertion from this same work allowance.
    assert!(compile_bounded(&plan, baseline + 1).is_err());
}

#[test]
fn processing_asset_membership_storage_has_a_checked_aggregate_cap() {
    let mut count = 0;
    for _ in 0..MAX_AUDIO_BINDING_ENTRIES {
        count = super::next_inventory_membership(count).unwrap();
    }
    assert_eq!(count, MAX_AUDIO_BINDING_ENTRIES);
    assert!(super::next_inventory_membership(count).is_err());
}
