//! Checked creative edge fixtures ported from the existing integration tests.
use super::*;
use std::collections::BTreeMap;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Voice",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn retime(child: &str, length: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Timing".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(length),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose: RetimePurpose::Edit,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn document(rate: u32, children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("fade-query").unwrap(),
        RevisionId::new("current").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(rate, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn bind(
    current: &ProjectDocument,
    reference: &ProjectDocument,
    physical: &str,
    clock: AudioClockRoot,
    phase: Option<ExactRatio>,
) -> ProjectDocument {
    let timing = AudioTimingId {
        allocation: RevisionId::new("fade-clock").unwrap(),
        ordinal: 0,
    };
    let binding = OwnedAudioBinding {
        reanchors: Vec::new(),
        lattice: AudioPlacementTemplate {
            reference_local_offset: deadpan_core::ExactRatio::ZERO,
            gap_after: None,
            reference: AudioReferenceClock {
                recipe: AudioRecipeKind::Node,
                timing: timing.clone(),
                root: clock,
                physical: id(physical),
            },
            arguments: vec![],
            births: vec![],
        },
        resume: phase.map(|constant| AudioResume {
            local_boundary: ExactRatio::ZERO,
            phase: AudioLocalPhase {
                constant,
                terms: vec![],
            },
        }),
    };
    let state = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing,
            layout: FrozenAudioLayout::capture(reference).unwrap(),
        }],
        BTreeMap::from([(id(physical), binding)]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn with_editorial(
    document: &ProjectDocument,
    node: &str,
    start: bool,
    end: bool,
    hard: bool,
) -> ProjectDocument {
    let mut owner = document.nodes()[&id(node)].clone();
    owner.audio_editorial_edges = AudioEditorialEdges { start, end };
    if hard {
        if start {
            owner.audio_edges.node_start = AudioEdgePolicy::Hard;
        }
        if end {
            owner.audio_edges.node_end = AudioEdgePolicy::Hard;
        }
    }
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"][node] = serde_json::to_value(owner).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn root(
    name: &str,
    document: &ProjectDocument,
    reference: Option<&ProjectDocument>,
    symbol: &str,
    extra: Value,
    probes: &[(&str, i64, i64)],
    visit: &mut impl FnMut(Parent<'_, '_>, &Value),
) {
    let plan = RenderPlan::compile(document).unwrap();
    let parent = Parent::Root(&plan);
    let source = json!({"kind":"checked_port_of_existing_fade_test","path":"tests/audio_fades.rs",
        "symbol":symbol,"document":document,"reference_document":reference,"details":extra});
    visit(parent, &descriptor(name, parent, source, probes));
}

fn translations(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let symbol =
        "translated_output_retains_tiny_width_and_rate_changes_use_a_stable_virtual_origin";
    let original = document(32_000, &["voice"], vec![("voice", hold(1))]);
    root(
        "fade/translation/reference",
        &original,
        None,
        symbol,
        json!({"pair":"translation"}),
        &[],
        visit,
    );
    let moved = document(
        32_000,
        &["prefix", "voice"],
        vec![("prefix", hold(1)), ("voice", hold(1))],
    );
    let bound = bind(
        &moved,
        &original,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    );
    root(
        "fade/translation/moved_bound",
        &bound,
        Some(&original),
        symbol,
        json!({"pair":"translation","current_document":moved}),
        &[("original_delivery", 2, 3)],
        visit,
    );
    for prefix in [false, true] {
        let mut nodes = vec![
            ("voice", hold(1)),
            ("slow", retime("voice", 2, 0, 1, PitchPolicy::FollowSpeed)),
        ];
        let children = if prefix {
            nodes.push(("prefix", hold(1)));
            vec!["prefix", "slow"]
        } else {
            vec!["slow"]
        };
        let current = document(32_000, &children, nodes);
        let bound = bind(
            &current,
            &original,
            "voice",
            AudioClockRoot::ProjectRootRoundEven,
            None,
        );
        let (start, end) = if prefix { (2, 4) } else { (0, 3) };
        root(
            &format!("fade/translation/slow_prefix_{prefix}"),
            &bound,
            Some(&original),
            symbol,
            json!({"pair":"translation","current_document":current,"prefix":prefix}),
            &[("original_delivery", start, end)],
            visit,
        );
    }
}

fn point_and_preserve(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let symbol = "opaque_preserve_uses_output_geometry_and_does_not_transport_input_fades";
    let current = document(
        32_000,
        &["preserve"],
        vec![
            ("voice", hold(1)),
            ("preserve", retime("voice", 2, 0, 1, PitchPolicy::Preserve)),
        ],
    );
    root(
        "fade/opaque_preserve/unbound",
        &current,
        None,
        symbol,
        json!({"pair":"opaque_preserve"}),
        &[("original_delivery", 0, 3)],
        visit,
    );
    let bound = bind(
        &current,
        &current,
        "voice",
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("preserve"),
        },
        None,
    );
    root(
        "fade/opaque_preserve/bound_input",
        &bound,
        Some(&current),
        symbol,
        json!({"pair":"opaque_preserve","current_document":current}),
        &[("original_delivery", 0, 3)],
        visit,
    );

    let symbol = "selected_origin_point_clock_uses_current_support_without_resetting_phase";
    let reference = document(
        32_000,
        &["preserve"],
        vec![
            ("voice", hold(3)),
            ("preserve", retime("voice", 2, 1, 2, PitchPolicy::Preserve)),
        ],
    );
    let current = document(32_000, &["voice"], vec![("voice", hold(3))]);
    let bound = bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("preserve"),
        },
        None,
    );
    root(
        "fade/point/selected_origin_full_voice",
        &bound,
        Some(&reference),
        symbol,
        json!({"pair":"selected_origin_point","current_document":current}),
        &[("original_delivery", 0, 4), ("before_support", 0, 1)],
        visit,
    );
    let cropped = document(
        32_000,
        &["crop"],
        vec![
            ("voice", hold(3)),
            ("crop", retime("voice", 1, 1, 2, PitchPolicy::FollowSpeed)),
        ],
    );
    let bound = bind(
        &cropped,
        &reference,
        "voice",
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("preserve"),
        },
        None,
    );
    root(
        "fade/point/selected_origin_current_crop",
        &bound,
        Some(&reference),
        symbol,
        json!({"pair":"selected_origin_point","current_document":cropped}),
        &[("original_delivery", 0, 2)],
        visit,
    );
    let birth = bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::DefinitionPointCeil { root: id("voice") },
        None,
    );
    root(
        "fade/point/definition_birth",
        &birth,
        Some(&reference),
        symbol,
        json!({"pair":"selected_origin_point","current_document":current}),
        &[("original_delivery", 0, 4)],
        visit,
    );
}

fn fractional(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let symbol = "fractional_progress_and_inverse_ceil_partitioning_are_request_independent";
    let nodes = || {
        vec![
            ("a", hold(1)),
            ("b", hold(1)),
            ("pair", BeatNode::sequence("Pair", vec![id("a"), id("b")])),
            ("preserve", retime("pair", 4, 0, 2, PitchPolicy::Preserve)),
        ]
    };
    let reference = document(48_000, &["preserve"], nodes());
    let mut current_nodes = nodes();
    current_nodes.push((
        "speed",
        retime("preserve", 3, 0, 4, PitchPolicy::FollowSpeed),
    ));
    let current = document(48_000, &["speed"], current_nodes);
    let bound = bind(
        &current,
        &reference,
        "preserve",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(1, 4)),
    );
    root(
        "fade/fractional/resumed_preserve_then_speed",
        &bound,
        Some(&reference),
        symbol,
        json!({"current_document":current,"phase":exact(ratio(1,4))}),
        &[
            ("original_delivery", 0, 3),
            ("before_seam", 1, 2),
            ("after_seam", 2, 3),
        ],
        visit,
    );
}

fn signed(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let symbol = "signed_domain_and_retained_negative_origin_keep_absolute_tie_parity";
    let current = document(32_000, &["voice"], vec![("voice", hold(4))]);
    let plan = RenderPlan::compile(&current).unwrap();
    let selector_value = crate::AudioDefinitionSelector::Node { node: id("voice") };
    let placement = crate::AudioRootPlacement::new(
        ratio(-1, 3),
        ratio(1, 2),
        ExactRatio::ZERO..ExactRatio::integer(4),
    )
    .unwrap();
    let physical = plan
        .audio_definition(selector_value.clone())
        .unwrap()
        .in_root_clock(placement.clone())
        .unwrap();
    let parent = Parent::Domain(&physical);
    let source = json!({"kind":"checked_definition_placement","path":"tests/audio_fades.rs","symbol":symbol,
        "document":current,"selector":selector_value,"placement":placement,"setup":"audio_definition.in_root_clock"});
    visit(
        parent,
        &descriptor(
            "fade/signed/definition_domain",
            parent,
            source,
            &[("original_delivery", 0, 2)],
        ),
    );
    let mut partition = retime("voice", 2, 2, 4, PitchPolicy::FollowSpeed);
    let NodeKind::Retime { purpose, .. } = &mut partition.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let reference = document(
        32_000,
        &["partition"],
        vec![("voice", hold(4)), ("partition", partition)],
    );
    let current = document(
        32_000,
        &["prefix", "speed"],
        vec![
            ("prefix", hold(1)),
            ("voice", hold(4)),
            ("speed", retime("voice", 2, 0, 4, PitchPolicy::FollowSpeed)),
        ],
    );
    let bound = bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    );
    root(
        "fade/signed/retained_negative_origin",
        &bound,
        Some(&reference),
        symbol,
        json!({"current_document":current}),
        &[("original_delivery", 2, 4)],
        visit,
    );
}

fn hard_edges(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let symbol =
        "current_coincident_hard_edges_survive_while_noncoincident_crop_edges_do_not_inherit_them";
    let mut voice = hold(4);
    voice.audio_edges = AudioEdgePolicies {
        node_start: AudioEdgePolicy::Hard,
        node_end: AudioEdgePolicy::Hard,
        ..Default::default()
    };
    let reference = document(48_000, &["voice"], vec![("voice", hold(4))]);
    let current = document(48_000, &["voice"], vec![("voice", voice.clone())]);
    let bound = bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    );
    root(
        "fade/hard/coincident_untrimmed",
        &bound,
        Some(&reference),
        symbol,
        json!({"pair":"coincident_hard","current_document":current}),
        &[],
        visit,
    );
    let current = document(
        48_000,
        &["crop"],
        vec![
            ("voice", voice),
            ("crop", retime("voice", 2, 1, 3, PitchPolicy::FollowSpeed)),
        ],
    );
    let bound = bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    );
    root(
        "fade/hard/noncoincident_crop",
        &bound,
        Some(&reference),
        symbol,
        json!({"pair":"coincident_hard","current_document":current}),
        &[],
        visit,
    );
}

fn editorials(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let symbol = "editorial_entry_uses_current_grid_without_reclocking_the_retained_opposite_edge";
    let reference = document(32_000, &["voice"], vec![("voice", hold(4))]);
    let mut crop = retime("voice", 3, 1, 4, PitchPolicy::FollowSpeed);
    let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let current = document(
        32_000,
        &["lead", "crop"],
        vec![("lead", hold(1)), ("voice", hold(4)), ("crop", crop)],
    );
    let bound = bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(1, 3)),
    );
    root(
        "fade/editorial/entry_before",
        &bound,
        Some(&reference),
        symbol,
        json!({"pair":"editorial_entry","current_document":current}),
        &[("incident_sample", 2, 3), ("next_sample", 3, 4)],
        visit,
    );
    let marked = with_editorial(&bound, "crop", true, false, false);
    root(
        "fade/editorial/entry_after",
        &marked,
        Some(&reference),
        symbol,
        json!({"pair":"editorial_entry","unmarked_document":bound}),
        &[("incident_sample", 2, 3), ("next_sample", 3, 4)],
        visit,
    );
    let symbol = "tiny_voice_shortens_a_large_marked_owner_and_frozen_context_keeps_the_marker";
    for length in [1, 2] {
        let plain = document(
            48_000,
            &["group"],
            vec![
                ("voice", hold(length)),
                ("rest", hold(200)),
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("voice"), id("rest")]),
                ),
            ],
        );
        let marked = with_editorial(&plain, "group", true, false, false);
        root(
            &format!("fade/editorial/tiny_{length}_before"),
            &plain,
            None,
            symbol,
            json!({"pair":format!("tiny_{length}")}),
            &[
                ("incident_sample", 0, 1),
                ("later_sibling", length, length + 2),
            ],
            visit,
        );
        root(
            &format!("fade/editorial/tiny_{length}_after"),
            &marked,
            None,
            symbol,
            json!({"pair":format!("tiny_{length}"),"unmarked_document":plain}),
            &[
                ("incident_sample", 0, 1),
                ("later_sibling", length, length + 2),
            ],
            visit,
        );
    }
    let symbol = "editorial_hard_precedence_uses_exact_coordinates_not_rounded_samples";
    let plain = document(
        96_000,
        &["lead", "voice"],
        vec![("lead", hold(1)), ("voice", hold(8))],
    );
    let mut wire = serde_json::to_value(&plain).unwrap();
    let mut owner = plain.nodes()[&id("root")].clone();
    owner.audio_edges.node_start = AudioEdgePolicy::Hard;
    wire["nodes"]["root"] = serde_json::to_value(owner).unwrap();
    let plain = ProjectDocument::from_json(&wire.to_string()).unwrap();
    for hard in [false, true] {
        let marked = with_editorial(&plain, "voice", true, false, hard);
        root(
            &format!("fade/editorial/exact_hard_{hard}"),
            &marked,
            None,
            symbol,
            json!({"pair":"exact_hard","unmarked_document":plain}),
            &[("rounded_coincidence", 0, 1)],
            visit,
        );
    }
}

fn source_endpoints(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    for endpoint in [AudioSourceEndpoint::Start, AudioSourceEndpoint::End] {
        let (document, plan) =
            policy_query_baseline::policy_retained_cases::source_endpoint_parent(endpoint);
        let parent = Parent::Root(&plan);
        let source = json!({"kind":"reused_checked_source_endpoint_fixture","path":"src/audio_owners/legacy_snapshot_v1/policy_retained_cases.rs",
            "symbol":"source_endpoint_parent","document":document,"endpoint":endpoint});
        visit(
            parent,
            &descriptor(
                &format!("fade/source_endpoint/{endpoint:?}/root"),
                parent,
                source.clone(),
                &[("original_endpoint_delivery", 8008, 8264)],
            ),
        );
        let owners = plan
            .audio_gain_owners(AudioSample(8008)..AudioSample(8009), Default::default())
            .unwrap();
        let owner = owners
            .spans()
            .iter()
            .flat_map(|span| span.owners())
            .find(|owner| owner.instance().node.as_str() == "source")
            .unwrap();
        let input = plan
            .audio_owner_input(owner, AudioSample(8008), Default::default())
            .unwrap();
        let parent = Parent::Domain(input.context());
        let mut source = source;
        source["setup_input"] = super::super::input(&input);
        visit(
            parent,
            &descriptor(
                &format!("fade/source_endpoint/{endpoint:?}/domain"),
                parent,
                source,
                &[("original_endpoint_delivery", 8008, 8264)],
            ),
        );
    }
}

fn virtual_source(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let source_document = |speed: bool| {
        let current = if speed {
            document(
                32_000,
                &["speed"],
                vec![
                    ("voice", hold(4)),
                    ("speed", retime("voice", 1, 0, 4, PitchPolicy::FollowSpeed)),
                ],
            )
        } else {
            document(32_000, &["voice"], vec![("voice", hold(4))])
        };
        let time_base = SourceTimeBase::new(1, 48_000).unwrap();
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 64,
                time_base,
            },
        )
        .unwrap();
        let source = BeatNode {
            label: "Voice".into(),
            framing: None,
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Source {
                source: SourceNode {
                    edit_window: None,
                    duration: frames(4),
                    video: SourceVideo::Blank,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio: Some(SourceAudio {
                        asset: AssetId::new("original").unwrap(),
                        span,
                    }),
                    audio_mapping: SourceAudioMapping::Placement {
                        start: ratio(3, 10),
                        frames: ratio(2, 5),
                    },
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        };
        let mut wire = serde_json::to_value(current).unwrap();
        wire["nodes"]["voice"] = serde_json::to_value(source).unwrap();
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            AssetId::new("original").unwrap(),
            AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(span),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        )]))
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let original = source_document(false);

    root(
        "fade/virtual_source/original",
        &original,
        None,
        "virtual_source_support_can_have_zero_or_one_fade_samples",
        json!({"pair":"virtual_source"}),
        &[("original_probe", 0, 1)],
        visit,
    );
    let faster = bind(
        &source_document(true),
        &original,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    );
    root(
        "fade/virtual_source/bound_fast",
        &faster,
        Some(&original),
        "virtual_source_support_can_have_zero_or_one_fade_samples",
        json!({"pair":"virtual_source","current_document":source_document(true)}),
        &[("original_probe", 0, 1)],
        visit,
    );
}
fn retained_editorial_geometry(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let mut first = hold(2);
    first.audio_edges.node_start = AudioEdgePolicy::Hard;
    let base = document(
        30,
        &["preserve"],
        vec![
            ("first", first),
            ("second", hold(2)),
            (
                "group",
                BeatNode::sequence("Input", vec![id("first"), id("second")]),
            ),
            ("preserve", retime("group", 3, 0, 4, PitchPolicy::Preserve)),
        ],
    );
    let marked = with_editorial(&base, "second", true, false, false);
    let bound = bind(
        &marked,
        &base,
        "preserve",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(-1, 3200)),
    );

    root(
        "fade/retained/current_leaf_hard_mismatch",
        &bound,
        Some(&base),
        "bound_preserve_inner_leaf_cannot_lend_hard_policy_to_a_different_current_leaf",
        json!({"current_document":marked}),
        &[
            ("seam_straddle", 2400, 2402),
            ("second_incident", 2401, 2402),
        ],
        visit,
    );
    let nodes = || {
        vec![
            ("first", hold(2)),
            ("second", hold(2)),
            (
                "group",
                BeatNode::sequence("Input", vec![id("first"), id("second")]),
            ),
            ("preserve", retime("group", 6, 0, 4, PitchPolicy::Preserve)),
        ]
    };
    let reference = document(30, &["preserve"], nodes());
    let mut crop = retime("preserve", 3, 3, 6, PitchPolicy::FollowSpeed);
    let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let mut current_nodes = nodes();
    current_nodes.extend([("lead", hold(3)), ("crop", crop)]);
    let current = document(30, &["lead", "crop"], current_nodes);
    let marked = with_editorial(&current, "crop", true, false, false);
    let bound = bind(
        &marked,
        &reference,
        "preserve",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(-1, 160)),
    );

    root(
        "fade/retained/delivered_cut",
        &bound,
        Some(&reference),
        "bound_outer_edge_fades_the_voice_delivered_at_the_cut_not_the_next_inner_voice",
        json!({"current_document":current,"marked_document":marked}),
        &[
            ("cut_straddle", 4800, 4812),
            ("inside_remaining_voice", 4804, 4807),
        ],
        visit,
    );
}
fn delivered_width(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let nodes = || {
        vec![
            ("a", hold(1)),
            ("b", hold(1)),
            ("c", hold(1)),
            ("d", hold(1)),
            (
                "group",
                BeatNode::sequence("Input", vec![id("a"), id("b"), id("c"), id("d")]),
            ),
            ("preserve", retime("group", 6, 0, 4, PitchPolicy::Preserve)),
        ]
    };
    let reference = document(48_000, &["preserve"], nodes());
    for (start_edge, crop_start, crop_end) in [(true, 5, 9), (false, 0, 6)] {
        let mut crop = retime(
            "speed",
            crop_end - crop_start,
            crop_start,
            crop_end,
            PitchPolicy::FollowSpeed,
        );
        let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
            unreachable!()
        };
        *purpose = RetimePurpose::Partition;
        let mut current_nodes = nodes();
        current_nodes.extend([
            (
                "speed",
                retime("preserve", 9, 0, 6, PitchPolicy::FollowSpeed),
            ),
            ("crop", crop),
        ]);
        let children = if crop_start > 0 {
            current_nodes.push(("lead", hold(crop_start)));
            vec!["lead", "crop"]
        } else {
            vec!["crop"]
        };
        let current = document(48_000, &children, current_nodes);
        let marked = with_editorial(&current, "crop", start_edge, !start_edge, false);
        let current = bind(
            &marked,
            &reference,
            "preserve",
            AudioClockRoot::ProjectRootRoundEven,
            None,
        );

        root(
            &format!("fade/retained/delivered_width_start_{start_edge}"),
            &current,
            Some(&reference),
            "new_bound_edge_width_uses_delivered_samples_instead_of_virtual_envelope_rounding",
            json!({"start_edge":start_edge,"crop_start":crop_start,"crop_end":crop_end,"marked_document":marked}),
            &[
                ("one_sample_voice", 5, 6),
                ("complete_crop", crop_start, crop_end),
            ],
            visit,
        );
    }
}
fn hidden_eligibility(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let mut crop = retime("voice", 200, 1, 201, PitchPolicy::FollowSpeed);
    let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let plain = document(
        48_000,
        &["group"],
        vec![
            ("lead", hold(1)),
            ("voice", hold(201)),
            ("crop", crop),
            (
                "group",
                BeatNode::sequence("Group", vec![id("lead"), id("crop")]),
            ),
        ],
    );
    let ancestor = with_editorial(&plain, "group", true, false, false);

    let symbol = "later_partition_drops_ancestor_marker_but_retains_its_own_hidden_owner_edge";
    for (name, current) in [("plain", &plain), ("ancestor", &ancestor)] {
        root(
            &format!("fade/hidden/{name}"),
            current,
            Some(&plain),
            symbol,
            json!({"pair":"hidden_partition"}),
            &[("later_partition", 1, 97)],
            visit,
        );
    }
    let owned = with_editorial(&plain, "voice", true, false, false);
    for bound in [false, true] {
        let current = if bound {
            bind(
                &owned,
                &plain,
                "voice",
                AudioClockRoot::ProjectRootRoundEven,
                None,
            )
        } else {
            owned.clone()
        };
        root(
            &format!("fade/hidden/owner_bound_{bound}"),
            &current,
            Some(&plain),
            symbol,
            json!({"pair":"hidden_partition","bound":bound,"marked_document":owned}),
            &[("own_hidden_edge", 1, 2)],
            visit,
        );
    }
    let plain = document(48_000, &["voice"], vec![("voice", hold(200))]);
    for (start, end, phase, range) in [(true, false, -10, 10..20), (false, true, 10, 180..190)] {
        let bound = bind(
            &plain,
            &plain,
            "voice",
            AudioClockRoot::ProjectRootRoundEven,
            Some(ExactRatio::integer(phase)),
        );
        let marked = with_editorial(&bound, "voice", start, end, false);

        for (label, current) in [("bound", &bound), ("marked", &marked)] {
            root(
                &format!("fade/eligibility/start_{start}/{label}"),
                current,
                Some(&plain),
                "transported_bound_owner_edges_are_not_reintroduced_after_eligibility_rejects_them",
                json!({"pair":format!("eligibility_start_{start}"),"start_edge":start,"end_edge":end,"phase":phase}),
                &[
                    ("selected", range.start, range.end),
                    ("incident", range.start, range.start + 1),
                ],
                visit,
            );
        }
    }
    let plain = document(
        48_000,
        &["preserve"],
        vec![
            ("a", hold(150)),
            ("b", hold(150)),
            ("group", BeatNode::sequence("Input", vec![id("a"), id("b")])),
            (
                "preserve",
                retime("group", 200, 0, 300, PitchPolicy::Preserve),
            ),
        ],
    );
    let inner = with_editorial(&plain, "b", true, false, false);
    let bound = bind(
        &inner,
        &plain,
        "preserve",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ExactRatio::integer(-10)),
    );
    let marked = with_editorial(&bound, "preserve", true, false, false);

    for (label, current) in [("bound", &bound), ("outer_marked", &marked)] {
        root(
            &format!("fade/opaque_owner/{label}"),
            current,
            Some(&plain),
            "bound_opaque_owner_suppression_keeps_a_distinct_inner_marker",
            json!({"pair":"opaque_owner","inner_marked_document":inner}),
            &[("rejected_outer", 10, 20), ("distinct_inner", 110, 111)],
            visit,
        );
    }
}
fn repeat_editorial(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
    let fragment = BeatNode {
        label: "Fragment".into(),
        kind: NodeKind::Retime {
            child: id("voice"),
            duration: frames(4),
            mapping: FrameRange::new(ProjectFrame(2), ProjectFrame(6)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Partition,
        },
        ..hold(1)
    };
    let base = document(
        30,
        &["fragment"],
        vec![("voice", hold(8)), ("fragment", fragment)],
    );
    let edit = |document: &ProjectDocument, revision: &str, command: Command| {
        apply(
            document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(revision).unwrap(),
                command,
            },
        )
        .unwrap()
        .forward
        .apply(document)
        .unwrap()
    };
    let looped = edit(
        &base,
        "loop",
        Command::WrapRepeat {
            node: id("fragment"),
            id: id("loop"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );

    let marked = edit(
        &looped,
        "marked",
        Command::SetEditorialEdges {
            node: id("fragment"),
            edges: AudioEditorialEdges {
                start: true,
                end: true,
            },
        },
    );
    let mut hard = marked.clone();
    for (revision, edge) in [
        ("hard-start", AudioBoundaryKind::NodeStart),
        ("hard-end", AudioBoundaryKind::NodeEnd),
    ] {
        hard = edit(
            &hard,
            revision,
            Command::SetAudioEdge {
                node: id("fragment"),
                edge,
                policy: AudioEdgePolicy::Hard,
            },
        );
    }
    for (label, current) in [("unmarked", &looped), ("marked", &marked), ("hard", &hard)] {
        root(
            &format!("fade/repeat_fragment/{label}"),
            current,
            Some(&base),
            "marked_fragment_play_seams_fade_or_cut_by_their_policy",
            json!({"pair":"fragment_seam","construction":"actual WrapRepeat, SetEditorialEdges and SetAudioEdge commands"}),
            &[("whole_two_plays", 0, 12800), ("repeat_seam", 6399, 6401)],
            visit,
        );
    }
}
mod gap_cases {
    use super::*;
    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }
    fn frames(value: i64) -> FrameDuration {
        FrameDuration::new(value).unwrap()
    }
    fn recipe(duration: i64) -> HoldRecipe {
        HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        }
    }
    fn play(ordinal: u32) -> IterationId {
        IterationId {
            allocation: RevisionId::new("plays").unwrap(),
            ordinal,
        }
    }
    fn captured(plays: u32) -> ProjectDocument {
        let document = ProjectDocument::new(
            ProjectId::new("bound-gaps").unwrap(),
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
        let mut wire = serde_json::to_value(document).unwrap();
        for (name, node) in [
            (
                "root",
                BeatNode::sequence("Root", vec![id("prefix"), id("repeat")]),
            ),
            ("prefix", BeatNode::hold("Prefix", recipe(1))),
            ("child", BeatNode::hold("Child", recipe(1))),
            (
                "repeat",
                BeatNode {
                    label: "Repeat".into(),
                    framing: None,
                    audio_treatments: Default::default(),
                    audio_editorial_edges: Default::default(),
                    audio_edges: AudioEdgePolicies {
                        node_start: AudioEdgePolicy::Hard,
                        node_end: AudioEdgePolicy::Hard,
                        repeat_gap_start: AudioEdgePolicy::Hard,
                        ..Default::default()
                    },
                    kind: NodeKind::Repeat {
                        child: id("child"),
                        iterations: IterationOrder::new(play(0).allocation, plays).unwrap(),
                        gap: Some(recipe(2)),
                        escalation: None,
                    },
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                },
            ),
        ] {
            wire["nodes"][name] = serde_json::to_value(node).unwrap();
        }
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        wire["audio_bindings"] = serde_json::to_value(
            capture_unbound_audio_bindings(
                &document,
                AudioTimingId {
                    allocation: RevisionId::new("capture").unwrap(),
                    ordinal: 0,
                },
            )
            .unwrap(),
        )
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }
    fn edit(document: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
        apply(
            document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(revision).unwrap(),
                command,
            },
        )
        .unwrap()
        .forward
        .apply(document)
        .unwrap()
    }

    pub(super) fn visit(visit: &mut impl FnMut(Parent<'_, '_>, &Value)) {
        let before = captured(3);
        let moved = edit(
            &before,
            "moved",
            Command::SetHoldDuration {
                node: id("prefix"),
                duration: frames(2),
            },
        );
        let plan = RenderPlan::compile(&moved).unwrap();
        let source = json!({"kind":"checked_port_of_existing_gap_test","path":"tests/gap_bindings.rs",
        "symbol":"moved_gap_is_bound_in_root_and_captured_domains_with_its_own_edges",
        "document":moved,"reference_document":before,"setup":{"plays":3,"domain_observed_at":4805}});
        let parent = Parent::Root(&plan);
        visit(
            parent,
            &descriptor(
                "fade/gap_bound/root",
                parent,
                source.clone(),
                &[("original_gap_delivery", 4805, 8008)],
            ),
        );
        let physical = plan
            .audio_domain_at(AudioSample(4805), Default::default())
            .unwrap();
        let parent = Parent::Domain(&physical);
        visit(
            parent,
            &descriptor(
                "fade/gap_bound/domain",
                parent,
                source,
                &[("original_gap_delivery", 4805, 8008)],
            ),
        );
    }
}

pub(in crate::plan) fn visit_cases(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    translations(&mut visit);
    point_and_preserve(&mut visit);
    fractional(&mut visit);
    signed(&mut visit);
    hard_edges(&mut visit);
    editorials(&mut visit);
    source_endpoints(&mut visit);
    virtual_source(&mut visit);
    retained_editorial_geometry(&mut visit);
    delivered_width(&mut visit);
    hidden_eligibility(&mut visit);
    repeat_editorial(&mut visit);
    gap_cases::visit(&mut visit);
}
