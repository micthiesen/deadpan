use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::{Value, json};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn duration(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(a: i128, b: i128) -> ExactRatio {
    ExactRatio::new(a, b).unwrap()
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "silence",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
            command,
        },
    )
    .unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    after
}

fn fixture(plays: u32) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("reference").unwrap(),
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
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    let selected = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 441_000,
            time_base,
        },
    )
    .unwrap();
    let audio = SourceAudio {
        asset: AssetId::new("original").unwrap(),
        span: selected,
    };
    let document = edit(
        &document,
        Command::AddAsset {
            id: audio.asset.clone(),
            asset: AssetRecord {
                label: "private media label".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(selected),
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        },
    );
    let mut nodes = BTreeMap::from([
        (
            id("context"),
            BeatNode::sequence("context", vec![id("pre"), id("repeat"), id("tail")]),
        ),
        (id("pre"), hold(1)),
        (id("tail"), hold(2)),
        (id("override"), hold(7)),
        (
            id("source"),
            BeatNode {
                label: "original selection".into(),
                audio_edges: AudioEdgePolicies {
                    source_placement_start: AudioEdgePolicy::Hard,
                    ..Default::default()
                },
                kind: NodeKind::Source {
                    source: SourceNode {
                        duration: duration(6),
                        video: SourceVideo::Blank,
                        video_mapping: SourceVideoMapping::FitBeat,
                        audio: Some(audio.clone()),
                        audio_mapping: SourceAudioMapping::Placement {
                            start: ratio(-1, 3),
                            frames: ratio(6, 1),
                        },
                        link: LinkRelation::Independent,
                        audio_offset: AudioSample(-17),
                    },
                },
            },
        ),
        (
            id("retime"),
            BeatNode {
                label: "preserve".into(),
                audio_edges: Default::default(),
                kind: NodeKind::Retime {
                    child: id("source"),
                    duration: duration(3),
                    mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(5)).unwrap(),
                    pitch: PitchPolicy::Preserve,
                    purpose: RetimePurpose::Edit,
                },
            },
        ),
    ]);
    let allocation = RevisionId::new("plays").unwrap();
    nodes.insert(
        id("repeat"),
        BeatNode {
            label: "repeat".into(),
            audio_edges: Default::default(),
            kind: NodeKind::Repeat {
                child: id("retime"),
                iterations: IterationOrder::new(allocation.clone(), plays).unwrap(),
                gap: Some(HoldRecipe {
                    duration: duration(1),
                    video: HoldVideo::Background,
                    audio: HoldAudio::RoomTone { source: audio },
                }),
            },
        },
    );
    let overrides = BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: IterationId {
                allocation,
                ordinal: 4,
            },
            root: id("override"),
        }])
        .unwrap(),
    )]);
    edit(
        &document,
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("context"),
                nodes,
                overrides,
            },
        },
    )
}

fn iteration(document: &ProjectDocument, index: u32) -> IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id("repeat")].kind else {
        panic!()
    };
    iterations.at(index).unwrap()
}
fn instance(document: &ProjectDocument, node: &str, index: u32) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: iteration(document, index),
        }],
    }
}

#[test]
fn capture_roundtrips_only_timing_and_audibility() {
    let document = fixture(9);
    let frozen = FrozenAudioLayout::capture(&document).unwrap();
    frozen.validate().unwrap();
    assert_eq!(frozen.root(), document.root());
    assert_eq!(frozen.rate(), document.presentation_basis().frame_rate);
    assert_eq!(frozen.duration(), document.duration().unwrap());
    assert_eq!(frozen.nodes().len(), document.nodes().len());
    assert_eq!(frozen.overrides(), document.overrides());
    let json = frozen.to_json().unwrap();
    assert_eq!(FrozenAudioLayout::from_json(&json).unwrap(), frozen);
    for excluded in [
        "private media label",
        "original selection",
        "content_hash",
        "asset",
        "441000",
        "marks",
        "revision_id",
        "bindings",
        "index",
    ] {
        assert!(!json.contains(excluded), "captured {excluded}");
    }
    let FrozenAudioKind::Source {
        placement: Some(placement),
    } = frozen.nodes()[&id("source")].kind
    else {
        panic!()
    };
    let expected_start = ratio(-1, 3).checked_sub(ratio(85, 8008)).unwrap();
    assert_eq!(placement.start, expected_start);
    assert_eq!(
        placement.end,
        expected_start.checked_add(ratio(6, 1)).unwrap()
    );
    assert_eq!(
        frozen.nodes()[&id("source")].edges.source_placement_start,
        AudioEdgePolicy::Hard
    );
    assert!(matches!(
        frozen.nodes()[&id("repeat")].kind,
        FrozenAudioKind::Repeat {
            gap_audio: ReferenceAudibility::RoomTone,
            ..
        }
    ));
}

#[test]
fn frozen_phase_survives_live_play_reorder_move_and_deleted_siblings() {
    let document = fixture(9);
    let frozen = FrozenAudioLayout::capture(&document).unwrap();
    let scope = instance(&document, "source", 5);
    let original = frozen.project(&scope, ratio(1, 3), None, 100).unwrap();
    assert_eq!(original.origin, ratio(97, 4));
    assert_eq!(original.frames_per_local_frame, ratio(3, 4));
    assert_eq!(original.point, ratio(49, 2));
    assert_eq!(original.local_duration, duration(6));
    let moved = edit(
        &document,
        Command::MovePlays {
            node: id("repeat"),
            start: 5,
            end: 6,
            destination: 0,
        },
    );
    let moved = edit(
        &moved,
        Command::Move {
            node: id("tail"),
            parent: id("context"),
            index: 0,
        },
    );
    let deleted = edit(&moved, Command::Delete { node: id("pre") });
    assert_eq!(
        frozen.project(&scope, ratio(1, 3), None, 100).unwrap(),
        original
    );
    let current = FrozenAudioLayout::capture(&deleted)
        .unwrap()
        .project(&scope, ratio(1, 3), None, 100)
        .unwrap();
    assert_ne!(current.origin, original.origin);
    assert_eq!(current.origin, ratio(5, 4));
    assert_eq!(
        FrozenAudioLayout::from_json(&frozen.to_json().unwrap())
            .unwrap()
            .project(&scope, ratio(1, 3), None, 100)
            .unwrap(),
        original
    );
}

#[test]
fn billion_plays_and_sparse_override_have_bounded_projection_and_explicit_gap_clock() {
    let document = fixture(1_000_000_000);
    let frozen = FrozenAudioLayout::capture(&document).unwrap();
    assert!(frozen.to_json().unwrap().len() < 10_000);
    let last = instance(&document, "source", 999_999_999);
    let projection = frozen.project(&last, ExactRatio::ZERO, None, 20).unwrap();
    assert!(projection.work <= 10);
    assert_eq!(projection.origin, ratio(16_000_000_001, 4));
    assert_eq!(
        frozen
            .project(&last, ExactRatio::ZERO, None, projection.work - 1)
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    let overridden = instance(&document, "override", 4);
    assert_eq!(
        frozen
            .project(&overridden, ExactRatio::ZERO, None, 20)
            .unwrap()
            .origin,
        ratio(17, 1)
    );
    assert!(
        frozen
            .project(
                &instance(&document, "source", 4),
                ExactRatio::ZERO,
                None,
                20
            )
            .is_err()
    );
    assert!(
        frozen
            .project(
                &instance(&document, "override", 5),
                ExactRatio::ZERO,
                None,
                20
            )
            .is_err()
    );
    let gap_scope = InstancePath {
        node: id("repeat"),
        repeats: vec![],
    };
    let gap = frozen
        .project(&gap_scope, ratio(1, 2), Some(&iteration(&document, 4)), 20)
        .unwrap();
    assert_eq!(gap.origin, ratio(24, 1));
    assert_eq!(gap.point, ratio(49, 2));
    assert_eq!(gap.local_duration, duration(1));
    assert_eq!(gap.gap_after.as_ref(), Some(&iteration(&document, 4)));
    assert!(
        frozen
            .project(
                &gap_scope,
                ExactRatio::ZERO,
                Some(&iteration(&document, 999_999_999)),
                20
            )
            .is_err()
    );
}

#[test]
fn complete_nested_paths_are_required_and_signed_hidden_coordinates_are_not_clamped() {
    let document = fixture(9);
    let nested = edit(
        &document,
        Command::WrapRepeat {
            node: id("repeat"),
            id: id("outer"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    let frozen = FrozenAudioLayout::capture(&nested).unwrap();
    let NodeKind::Repeat { iterations, .. } = &nested.nodes()[&id("outer")].kind else {
        panic!()
    };
    let mut path = instance(&nested, "source", 5);
    path.repeats.insert(
        0,
        RepeatInstance {
            node: id("outer"),
            iteration: iterations.at(1).unwrap(),
        },
    );
    let good = frozen.project(&path, ratio(-2, 3), None, 30).unwrap();
    assert_eq!(good.point, good.origin.checked_sub(ratio(1, 2)).unwrap());
    for wrong in [
        InstancePath {
            node: id("source"),
            repeats: vec![],
        },
        InstancePath {
            node: id("source"),
            repeats: path.repeats.iter().rev().cloned().collect(),
        },
        InstancePath {
            node: id("root"),
            repeats: path.repeats.clone(),
        },
        InstancePath {
            node: id("missing"),
            repeats: vec![],
        },
    ] {
        assert!(frozen.project(&wrong, ExactRatio::ZERO, None, 30).is_err());
    }
    assert!(
        frozen
            .project(&path, ExactRatio::ZERO, Some(&iteration(&nested, 5)), 30)
            .is_err()
    );
    assert_eq!(
        frozen
            .project(&path, ExactRatio::ZERO, None, 0)
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
}

fn wire() -> Value {
    serde_json::to_value(FrozenAudioLayout::capture(&fixture(9)).unwrap()).unwrap()
}
fn rejected(value: Value) {
    assert!(
        FrozenAudioLayout::from_json(&value.to_string()).is_err(),
        "accepted {value}"
    );
}

#[test]
fn closed_grammar_rejects_unknown_missing_and_duplicate_fields() {
    let original = wire();
    for path in [
        "",
        "/rate",
        "/nodes/source",
        "/nodes/source/kind",
        "/nodes/source/kind/placement",
        "/nodes/source/edges",
        "/nodes/repeat/kind/iterations",
        "/nodes/repeat/kind/iterations/runs/0",
        "/nodes/retime/kind/mapping",
        "/overrides/repeat/0",
    ] {
        let mut value = original.clone();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), Value::Null);
        rejected(value);
    }
    for key in ["root", "rate", "nodes", "overrides"] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(key);
        rejected(value);
    }
    let mut value = original.clone();
    value["nodes"]["source"]["kind"]
        .as_object_mut()
        .unwrap()
        .remove("placement");
    rejected(value);
    let mut value = original.clone();
    value["nodes"]["pre"]["kind"]["audio"] = json!({"silence":{"unexpected":null}});
    rejected(value);
    let json = original.to_string();
    for duplicate in [
        json.replacen(
            "\"nodes\":{",
            &format!("\"nodes\":{{\"pre\":{},", original["nodes"]["pre"]),
            1,
        ),
        json.replacen(
            "\"overrides\":{",
            &format!(
                "\"overrides\":{{\"repeat\":{},",
                original["overrides"]["repeat"]
            ),
            1,
        ),
        json.replacen(
            "\"root\":\"root\"",
            "\"root\":\"root\",\"root\":\"root\"",
            1,
        ),
    ] {
        assert!(FrozenAudioLayout::from_json(&duplicate).is_err());
    }
}

#[test]
fn frozen_copy_lineage_roundtrips_owned_aliases_and_keeps_legacy_empty() {
    let original = fixture(1_000_000_000);
    let legacy = FrozenAudioLayout::capture(&original).unwrap();
    assert!(legacy.audio_lineage().is_empty());
    assert!(!legacy.to_json().unwrap().contains("audio_lineage"));
    assert!(
        FrozenAudioLayout::from_json(&legacy.to_json().unwrap())
            .unwrap()
            .audio_lineage()
            .is_empty()
    );
    let split = edit(
        &original,
        Command::Split {
            node: original.root().clone(),
            at: duration(2),
            identities: SplitIdentities {
                nodes: (0..40).map(|i| id(&format!("split-{i}"))).collect(),
            },
        },
    );
    assert!(!split.audio_lineage().is_empty());
    let frozen = FrozenAudioLayout::capture(&split).unwrap();
    assert_eq!(frozen.audio_lineage(), split.audio_lineage());
    assert_eq!(
        FrozenAudioLayout::from_json(&frozen.to_json().unwrap()).unwrap(),
        frozen
    );
    assert!(frozen.to_json().unwrap().len() < 20_000);
    // Historical origins need not name an existing alias. Only map ownership
    // is constrained by this frozen layout; no live-tree lookup is permitted.
    let mut value = serde_json::to_value(&frozen).unwrap();
    let owner = frozen.audio_lineage().keys().next().unwrap().to_string();
    value["audio_lineage"][&owner]["origin"] = json!("historical-context");
    FrozenAudioLayout::from_json(&value.to_string()).unwrap();
    value["audio_lineage"]["missing-owner"] =
        json!({"allocation":"old", "origin":"historical-context"});
    rejected(value);
}

#[test]
fn frozen_copy_lineage_preflight_is_closed_unique_and_bounded() {
    let original = wire();
    for token in [
        json!({"allocation":"old", "origin":"source", "media":"original"}),
        json!({"allocation":[], "origin":"source"}),
        json!({"allocation":"old", "origin":{}}),
        json!({"allocation":"old"}),
        json!({"origin":"source"}),
        Value::Null,
    ] {
        let mut value = original.clone();
        value["audio_lineage"] = json!({"source":token});
        rejected(value);
    }
    for input in [
        r#"{"audio_lineage":{"source":{"allocation":"old","origin":"source"},"sour\u0063e":BROKEN"#,
        r#"{"audio_lineage":{"source":{"allocation":"old","allocation":"new","origin":"source"}}}"#,
        r#"{"audio_lineage":{"source":{"allocation":"old","origin":"source","origin":"other"}}}"#,
        r#"{"audio_lineage":[],"audio_lineage":{}}"#,
    ] {
        let error = FrozenAudioLayout::from_json(input).unwrap_err();
        assert_eq!(error.code, DocumentErrorCode::InvalidJson);
        if input.contains("BROKEN") {
            assert!(
                error
                    .to_string()
                    .contains("duplicate frozen audio lineage alias")
            );
        }
    }
    let entries = (0..MAX_DOCUMENT_NODES)
        .map(|i| format!(r#""owner-{i}":{{"allocation":"old","origin":"source"}},"#))
        .collect::<String>();
    limit_before_malformed_tail(format!(r#"{{"audio_lineage":{{{entries}"last":BROKEN"#));
}

#[test]
fn admission_rejects_tampered_structure_durations_placements_and_policies() {
    let original = wire();
    for (pointer, replacement) in [
        ("/root", json!("pre")),
        (
            "/nodes/context/kind/children",
            json!(["pre", "pre", "repeat", "tail"]),
        ),
        (
            "/nodes/context/kind/children",
            json!(["missing", "repeat", "tail"]),
        ),
        (
            "/nodes/context/kind/children",
            json!(["root", "repeat", "tail"]),
        ),
        ("/nodes/context/kind/children", json!(["repeat", "tail"])),
        ("/nodes/context/duration", json!(100)),
        ("/nodes/source/duration", json!(0)),
        ("/nodes/repeat/duration", json!(1)),
        ("/nodes/repeat/kind/gap_duration", json!(0)),
        ("/nodes/retime/kind/purpose", json!("partition")),
        ("/nodes/retime/kind/mapping/end", json!(7)),
        ("/nodes/pre/edges/source_placement_start", json!("hard")),
        ("/overrides/repeat/0/iteration/ordinal", json!(99)),
        ("/overrides/repeat/0/root", json!("pre")),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        rejected(value);
    }
    let mut value = original.clone();
    value["nodes"]["source"]["kind"]["placement"]["end"] =
        value["nodes"]["source"]["kind"]["placement"]["start"].clone();
    rejected(value);
    let mut value = original.clone();
    value["overrides"]["pre"] = value["overrides"]["repeat"].clone();
    rejected(value);
    let mut value = original;
    value["overrides"]["repeat"] = json!([]);
    rejected(value);
    assert_eq!(
        FrozenAudioLayout::from_json(&" ".repeat(MAX_DOCUMENT_JSON_BYTES + 1))
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
}

#[test]
fn depth_budget_is_enforced_on_flat_wire_without_recursive_json() {
    let mut value = wire();
    value["nodes"]["context"]["kind"]["children"] = json!(["deep-0", "repeat", "tail"]);
    for index in 0..MAX_DOCUMENT_DEPTH {
        let child = if index + 1 == MAX_DOCUMENT_DEPTH {
            "pre".to_owned()
        } else {
            format!("deep-{}", index + 1)
        };
        value["nodes"][format!("deep-{index}")] = json!({ "duration":1, "edges":AudioEdgePolicies::default(), "kind":{"type":"sequence","children":[child]} });
    }
    assert_eq!(
        FrozenAudioLayout::from_json(&value.to_string())
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
}

#[test]
fn tail_capture_preserves_local_maximum_for_holds_and_repeat_gaps_without_media() {
    let mut value = serde_json::to_value(fixture(9)).unwrap();
    let source = value["nodes"]["source"]["kind"]["source"]["audio"].clone();
    value["nodes"]["tail"]["kind"]["recipe"]["audio"] = json!({
        "type":"tail", "source":source, "maximum":2
    });
    value["nodes"]["repeat"]["kind"]["gap"]["audio"] = json!({
        "type":"tail", "source":source, "maximum":1
    });
    let document = ProjectDocument::from_json(&value.to_string()).unwrap();
    let frozen = FrozenAudioLayout::capture(&document).unwrap();
    assert!(matches!(frozen.nodes()[&id("tail")].kind,
        FrozenAudioKind::Hold { audio: ReferenceAudibility::Tail { maximum } } if maximum == duration(2)));
    assert!(matches!(frozen.nodes()[&id("repeat")].kind,
        FrozenAudioKind::Repeat { gap_audio: ReferenceAudibility::Tail { maximum }, .. } if maximum == duration(1)));
    let encoded = frozen.to_json().unwrap();
    assert_eq!(FrozenAudioLayout::from_json(&encoded).unwrap(), frozen);
    let inspection: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        inspection["nodes"]["tail"]["kind"]["audio"],
        json!({"type":"tail","maximum":2})
    );
    assert_eq!(
        inspection["nodes"]["repeat"]["kind"]["gap_audio"],
        json!({"type":"tail","maximum":1})
    );
    assert!(!encoded.contains("\"asset\""));
    assert!(!encoded.contains("\"span\""));
}

#[test]
fn reference_audibility_has_closed_tagged_grammar_and_checked_tail_limits() {
    for audio in [
        json!({"type":"tail"}),
        json!({"type":"tail","maximum":0}),
        json!({"type":"tail","maximum":-1}),
        json!({"type":"tail","maximum":null}),
        json!({"type":"tail","maximum":1,"source":null}),
        json!({"type":"silence","maximum":1}),
        json!({"type":"room_tone","unexpected":null}),
        json!("silence"),
    ] {
        let mut value = wire();
        value["nodes"]["pre"]["kind"]["audio"] = audio;
        rejected(value);
    }
    for path in ["/nodes/pre/kind/audio", "/nodes/repeat/kind/gap_audio"] {
        let mut value = wire();
        *value.pointer_mut(path).unwrap() = json!({"type":"tail","maximum":2});
        rejected(value);
    }
    assert!(
        serde_json::from_str::<ReferenceAudibility>(r#"{"type":"tail","maximum":1,"maximum":1}"#)
            .is_err()
    );
    for audio in [
        ReferenceAudibility::Silence,
        ReferenceAudibility::RoomTone,
        ReferenceAudibility::Tail {
            maximum: duration(1),
        },
    ] {
        assert_eq!(
            serde_json::from_str::<ReferenceAudibility>(&serde_json::to_string(&audio).unwrap())
                .unwrap(),
            audio
        );
    }
}

fn limit_before_malformed_tail(json: String) {
    assert!(json.len() < MAX_DOCUMENT_JSON_BYTES);
    assert_eq!(
        FrozenAudioLayout::from_json(&json).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
}

#[test]
fn streaming_preflight_stops_oversized_lists_and_maps_before_their_malformed_tail() {
    let ids = "\"id\",".repeat(MAX_DOCUMENT_NODES + 1);
    limit_before_malformed_tail(format!(
        r#"{{"nodes":{{"root":{{"kind":{{"children":[{ids}BROKEN"#
    ));
    let runs = "{\"allocation\":\"a\",\"first\":0,\"count\":1},".repeat(MAX_FROZEN_AUDIO_RUNS + 1);
    limit_before_malformed_tail(format!(
        r#"{{"nodes":{{"repeat":{{"kind":{{"iterations":{{"runs":[{runs}BROKEN"#
    ));
    let entries = "{\"iteration\":{\"allocation\":\"a\",\"ordinal\":0},\"root\":\"id\"},"
        .repeat(MAX_DOCUMENT_NODES + 1);
    limit_before_malformed_tail(format!(r#"{{"overrides":{{"repeat":[{entries}BROKEN"#));
    let nodes = "\"id\":null,".repeat(MAX_DOCUMENT_NODES);
    limit_before_malformed_tail(format!(r#"{{"nodes":{{{nodes}"last":BROKEN"#));
    let owners = "\"owner\":[],".repeat(MAX_DOCUMENT_NODES);
    limit_before_malformed_tail(format!(r#"{{"overrides":{{{owners}"last":BROKEN"#));
}

#[test]
fn streaming_preflight_aggregates_edges_and_runs_across_collections() {
    let half = MAX_DOCUMENT_NODES / 2;
    let ids = vec!["\"id\""; half].join(",");
    // Both individually legal lists exhaust the aggregate before the scalar
    // child reference is even parsed. Child lists and single-child nodes share
    // exactly the same budget as sparse override roots.
    limit_before_malformed_tail(format!(
        r#"{{"nodes":{{"a":{{"kind":{{"children":[{ids}]}}}},"b":{{"kind":{{"children":[{ids}]}}}},"c":{{"kind":{{"child":BROKEN"#
    ));
    let entries =
        vec!["{\"iteration\":{\"allocation\":\"a\",\"ordinal\":0},\"root\":\"id\"}"; half]
            .join(",");
    limit_before_malformed_tail(format!(
        r#"{{"nodes":{{"a":{{"kind":{{"children":[{ids}]}}}}}},"overrides":{{"a":[{entries}],"b":[BROKEN"#
    ));
    let runs =
        vec!["{\"allocation\":\"a\",\"first\":0,\"count\":1}"; MAX_FROZEN_AUDIO_RUNS / 2].join(",");
    limit_before_malformed_tail(format!(
        r#"{{"nodes":{{"a":{{"kind":{{"iterations":{{"runs":[{runs}]}}}}}},"b":{{"kind":{{"iterations":{{"runs":[{runs}]}}}}}},"c":{{"kind":{{"iterations":{{"runs":[BROKEN"#
    ));
}

#[test]
fn scanner_keeps_field_named_aliases_and_escaped_keys_legal_and_rejects_trailing_input() {
    let document = ProjectDocument::new_automatic(
        ProjectId::new("alias-test").unwrap(),
        RevisionId::new("initial").unwrap(),
        id("nodes"),
    )
    .unwrap();
    let document = edit(
        &document,
        Command::Insert {
            parent: id("nodes"),
            index: 0,
            subtree: Subtree {
                root: id("children"),
                nodes: BTreeMap::from([
                    (
                        id("children"),
                        BeatNode::sequence("children", vec![id("runs"), id("overrides")]),
                    ),
                    (id("runs"), hold(1)),
                    (id("overrides"), hold(2)),
                ]),
                overrides: BTreeMap::new(),
            },
        },
    );
    let frozen = FrozenAudioLayout::capture(&document).unwrap();
    let json = frozen.to_json().unwrap();
    assert_eq!(FrozenAudioLayout::from_json(&json).unwrap(), frozen);
    let escaped = json.replacen("\"nodes\":", r#""no\u0064es":"#, 1);
    assert_ne!(escaped, json);
    assert_eq!(FrozenAudioLayout::from_json(&escaped).unwrap(), frozen);
    for malformed in [format!("{json} null"), "{\"nodes\":".into()] {
        assert_eq!(
            FrozenAudioLayout::from_json(&malformed).unwrap_err().code,
            DocumentErrorCode::InvalidJson
        );
    }
    let deep = format!("{{\"unknown\":{}0{}}}", "[".repeat(65), "]".repeat(65));
    assert_eq!(
        FrozenAudioLayout::from_json(&deep).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
}

fn document_with_run_count(per_repeat: usize) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("runs-test").unwrap(),
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
    wire["nodes"]["root"]["kind"]["children"] = json!(["r1", "r2"]);
    let runs: Vec<_> = (0..per_repeat)
        .map(|first| json!({"allocation":"initial","first":first,"count":1}))
        .collect();
    for (repeat, child) in [("r1", "h1"), ("r2", "h2")] {
        wire["nodes"][repeat] = json!({"label":"repeat","kind":{"type":"repeat","child":child,"iterations":{"runs":runs},"gap":null}});
        wire["nodes"][child] = serde_json::to_value(hold(1)).unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn capture_enforces_the_frozen_only_aggregate_run_cap_and_accepts_its_exact_boundary() {
    let document = document_with_run_count(MAX_FROZEN_AUDIO_RUNS / 2);
    let frozen = FrozenAudioLayout::capture(&document).unwrap();
    assert_eq!(frozen.duration(), duration(MAX_FROZEN_AUDIO_RUNS as i64));
    assert_eq!(
        FrozenAudioLayout::from_json(&frozen.to_json().unwrap()).unwrap(),
        frozen
    );
    // The authored document remains valid under its per-Repeat run limit.
    // Frozen capture rejects the combined count before cloning/indexing it.
    let document = document_with_run_count(MAX_FROZEN_AUDIO_RUNS / 2 + 1);
    assert_eq!(
        FrozenAudioLayout::capture(&document).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
}
