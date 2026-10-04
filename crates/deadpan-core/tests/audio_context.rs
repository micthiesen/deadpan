use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::{Value, json};

#[test]
fn editorial_edges_survive_context_capture_with_closed_historical_vocabulary() {
    let before = fixture(AudioSample(0), SourceAudioMapping::FitBeat);
    let original = FrozenAudioContext::capture(&before).unwrap();
    let mut authored = serde_json::to_value(&before).unwrap();
    authored["nodes"]["source"]["audio_editorial_edges"] = json!({"start":true,"end":false});
    let marked = ProjectDocument::from_json(&authored.to_string()).unwrap();
    let context = FrozenAudioContext::capture(&marked).unwrap();
    assert_eq!(
        context.layout().nodes()[&node("source")].editorial_edges,
        AudioEditorialEdges {
            start: true,
            end: false
        }
    );
    assert_eq!(context.inputs(), original.inputs());
    assert!(context.matches_document(&marked).unwrap());
    assert!(!original.matches_document(&marked).unwrap());
    assert_eq!(
        FrozenAudioContext::from_json(&context.to_json().unwrap()).unwrap(),
        context
    );
    let baseline = serde_json::to_value(&original).unwrap();
    for version in 1..=5 {
        let mut historical = baseline.clone();
        historical["schema_version"] = json!(version);
        assert!(FrozenAudioContext::from_json(&historical.to_string()).is_ok());
        for flags in [
            Value::Null,
            json!({"start":false,"end":false}),
            json!({"start":true,"end":false}),
        ] {
            historical["layout"]["nodes"]["source"]["editorial_edges"] = flags;
            let encoded = historical.to_string();
            assert!(
                FrozenAudioContext::from_json(&encoded).is_err(),
                "context {version} admitted new field"
            );
            assert!(
                FrozenAudioContext::from_json(
                    &encoded.replace("editorial_edges", "editorial_\\u0065dges")
                )
                .is_err()
            );
        }
    }
    for value in [
        json!({}),
        json!({"start":true}),
        json!({"start":true,"end":null}),
        json!({"start":true,"end":false,"other":0}),
    ] {
        let mut bad = serde_json::to_value(&context).unwrap();
        bad["layout"]["nodes"]["source"]["editorial_edges"] = value;
        assert!(FrozenAudioContext::from_json(&bad.to_string()).is_err());
    }
    let duplicated = context.to_json().unwrap().replace(
        "\"editorial_edges\":",
        "\"editorial_edges\":{\"start\":false,\"end\":false},\"editorial_\\u0065dges\":",
    );
    assert!(FrozenAudioContext::from_json(&duplicated).is_err());
}

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(a: i128, b: i128) -> ExactRatio {
    ExactRatio::new(a, b).unwrap()
}
fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 480_000,
            time_base,
        },
    )
    .unwrap()
}
fn record(label: &str, audio: bool) -> AssetRecord {
    AssetRecord {
        label: label.into(),
        content_hash: "a".repeat(64),
        video: Some(span()),
        audio: audio.then_some(span()),
        still_image: false,
        frame_count: None,
        source_qualification: None,
    }
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
    transaction.forward.apply(document).unwrap()
}
fn fixture(offset: AudioSample, mapping: SourceAudioMapping) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("context").unwrap(),
        RevisionId::new("r1").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    for (name, has_audio) in [("original", true), ("picture", false), ("unused", true)] {
        document = edit(
            &document,
            Command::AddAsset {
                id: asset(name),
                asset: record(name, has_audio),
            },
        );
    }
    let source = SourceAudio {
        asset: asset("original"),
        span: span(),
    };
    let source_node = BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "private source label".into(),
        audio_editorial_edges: Default::default(),
        audio_edges: AudioEdgePolicies {
            source_placement_start: AudioEdgePolicy::Hard,
            ..Default::default()
        },
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(10),
                video: SourceVideo::Stream {
                    asset: asset("picture"),
                    span: span(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(source.clone()),
                audio_mapping: mapping,
                link: LinkRelation::Independent,
                audio_offset: offset,
            },
        },
        cutaways: Vec::new(),
    };
    let picture_only = BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "picture only".into(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(2),
                video: SourceVideo::Stream {
                    asset: asset("picture"),
                    span: span(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: None,
                audio_mapping: SourceAudioMapping::FitBeat,
                link: LinkRelation::Independent,
                audio_offset: AudioSample(0),
            },
        },
        cutaways: Vec::new(),
    };
    let hold = BeatNode::hold(
        "tail",
        HoldRecipe {
            picture_context: None,
            duration: frames(3),
            video: HoldVideo::Background,
            audio: HoldAudio::Tail {
                source: source.clone(),
                maximum: frames(2),
            },
        },
    );
    let repeat = BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "repeat".into(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: node("source"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 1_000_000_000)
                .unwrap(),
            gap: Some(HoldRecipe {
                picture_context: None,
                duration: frames(1),
                video: HoldVideo::Background,
                audio: HoldAudio::RoomTone {
                    source: source.clone(),
                },
            }),
            escalation: None,
        },
        cutaways: Vec::new(),
    };
    edit(
        &document,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("sequence"),
                nodes: BTreeMap::from([
                    (
                        node("sequence"),
                        BeatNode::sequence(
                            "sequence",
                            vec![node("repeat"), node("hold"), node("pictureonly")],
                        ),
                    ),
                    (node("repeat"), repeat),
                    (node("source"), source_node),
                    (node("hold"), hold),
                    (node("pictureonly"), picture_only),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
}

#[test]
fn old_context_versions_reject_even_empty_gap_branch_vocabulary() {
    let document = fixture(AudioSample(0), SourceAudioMapping::FitBeat);
    let context = FrozenAudioContext::capture(&document).unwrap();
    let current = serde_json::to_value(&context).unwrap();
    for version in [1, 2] {
        let mut wire = current.clone();
        wire["schema_version"] = json!(version);
        assert!(FrozenAudioContext::from_json(&wire.to_string()).is_ok());
        for field in [json!({}), Value::Null] {
            wire["layout"]["gap_overrides"] = field;
            assert!(FrozenAudioContext::from_json(&wire.to_string()).is_err());
        }
    }
}

#[test]
fn captures_exact_inputs_and_only_their_full_asset_records() {
    for offset in [AudioSample(-17), AudioSample(23)] {
        let mapping = SourceAudioMapping::Placement {
            start: ratio(-1, 3),
            frames: ratio(11, 2),
        };
        let document = fixture(offset, mapping);
        let snapshot = FrozenAudioContext::capture(&document).unwrap();
        assert_eq!(snapshot.project_id(), document.project_id());
        assert_eq!(snapshot.revision_id(), document.revision_id());
        assert_eq!(snapshot.assets().len(), 1);
        assert_eq!(
            snapshot.assets()[&asset("original")],
            document.assets()[&asset("original")]
        );
        assert_eq!(snapshot.inputs().len(), 3);
        assert!(!snapshot.inputs().contains_key(&node("pictureonly")));
        assert!(
            matches!(snapshot.inputs()[&node("source")], FrozenAudioInput::Source { mapping: SourceAudioMapping::Placement { .. }, offset: saved, .. } if saved == offset)
        );
        assert!(matches!(
            snapshot.inputs()[&node("repeat")],
            FrozenAudioInput::Hold { .. }
        ));
        assert!(matches!(
            snapshot.inputs()[&node("hold")],
            FrozenAudioInput::Hold { .. }
        ));
        assert!(matches!(
            snapshot.layout().nodes()[&node("pictureonly")].kind,
            FrozenAudioKind::Source { placement: None }
        ));
        let json = snapshot.to_json().unwrap();
        assert_eq!(FrozenAudioContext::from_json(&json).unwrap(), snapshot);
        assert!(!json.contains("private source label"));
        assert!(!json.contains("picture only"));
        assert!(!json.contains("unused"));
        assert!(json.contains("original"));
        assert_eq!(
            snapshot.layout().nodes()[&node("repeat")].kind.clone(),
            FrozenAudioContext::from_json(&json)
                .unwrap()
                .layout()
                .nodes()[&node("repeat")]
                .kind
        );
    }
}

#[test]
fn selected_context_keeps_full_phase_mapping_and_captures_only_its_audible_extent() {
    let mapping = SourceAudioMapping::SelectedPlacement {
        start: ratio(-1, 3),
        frames: ratio(11, 2),
        selection: ExactFrameRange {
            start: ratio(1, 7),
            end: ratio(5, 3),
        },
    };
    let offset = AudioSample(23);
    let document = fixture(offset, mapping);
    let context = FrozenAudioContext::capture(&document).unwrap();
    assert_eq!(
        context.inputs()[&node("source")],
        FrozenAudioInput::Source {
            source: SourceAudio {
                asset: asset("original"),
                span: span()
            },
            mapping,
            offset,
        }
    );
    assert_eq!(
        context.layout().nodes()[&node("source")].kind,
        FrozenAudioKind::Source {
            placement: Some(
                mapping
                    .selection_frames_with_offset(
                        frames(10),
                        offset,
                        document.presentation_basis().frame_rate
                    )
                    .unwrap()
            ),
        }
    );
    assert_eq!(
        FrozenAudioContext::from_json(&context.to_json().unwrap()).unwrap(),
        context
    );
    let mut wire = serde_json::to_value(context).unwrap();
    assert_eq!(wire["schema_version"], json!(6));
    wire["schema_version"] = json!(1);
    assert!(FrozenAudioContext::from_json(&wire.to_string()).is_err());
    let escaped = wire
        .to_string()
        .replace("selected_placement", "selected_placem\\u0065nt");
    assert!(FrozenAudioContext::from_json(&escaped).is_err());
}

#[test]
fn legacy_context_roundtrips_its_closed_mapping_vocabulary_without_upgrading() {
    let context = FrozenAudioContext::capture(&fixture(
        AudioSample(0),
        SourceAudioMapping::Placement {
            start: ratio(-1, 3),
            frames: ratio(11, 2),
        },
    ))
    .unwrap();
    let mut wire = serde_json::to_value(context).unwrap();
    wire["schema_version"] = json!(1);
    let legacy = FrozenAudioContext::from_json(&wire.to_string()).unwrap();
    assert_eq!(serde_json::to_value(legacy).unwrap(), wire);
    wire["inputs"]["source"]["mapping"]["selection"] = Value::Null;
    assert!(FrozenAudioContext::from_json(&wire.to_string()).is_err());
}

#[test]
fn ingress_rejects_open_or_inconsistent_inventory() {
    let context =
        FrozenAudioContext::capture(&fixture(AudioSample(0), SourceAudioMapping::FitBeat)).unwrap();
    let mut value: Value = serde_json::from_str(&context.to_json().unwrap()).unwrap();
    let baseline = value.clone();
    value["schema_version"] = json!(7);
    assert_eq!(
        FrozenAudioContext::from_json(&value.to_string())
            .unwrap_err()
            .code,
        DocumentErrorCode::UnsupportedSchema
    );
    value = baseline.clone();
    value["new_binding"] = json!(true);
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["inputs"].as_object_mut().unwrap().remove("source");
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["assets"].as_object_mut().unwrap().insert(
        "picture".into(),
        serde_json::to_value(record("picture", false)).unwrap(),
    );
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["inputs"]["source"]["offset"] = json!(17);
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["inputs"]["source"]["source"]["asset"] = json!("missing");
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["inputs"]["source"]["type"] = json!("hold");
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["inputs"]["source"]["future"] = json!(1);
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["inputs"]["source"]["source"]["span"]["end"]["ticks"] = json!(480_001);
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["assets"]["original"]["content_hash"] = json!("invalid");
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["inputs"]["source"]["mapping"] = Value::Null;
    assert!(FrozenAudioContext::from_json(&value.to_string()).is_err());
    value = baseline.clone();
    value["inputs"]["source"]["future"] = json!("x".repeat(64 * 1024));
    assert_eq!(
        FrozenAudioContext::from_json(&value.to_string())
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    let duplicate = context.to_json().unwrap().replacen(
        "\"schema_version\":6",
        "\"schema_version\":6,\"schema_version\":6",
        1,
    );
    assert!(FrozenAudioContext::from_json(&duplicate).is_err());
    let duplicate_input =
        context
            .to_json()
            .unwrap()
            .replacen("\"source\":{", "\"source\":null,\"source\":{", 1);
    assert!(FrozenAudioContext::from_json(&duplicate_input).is_err());
    let oversize = " ".repeat(MAX_DOCUMENT_JSON_BYTES + 1);
    assert_eq!(
        FrozenAudioContext::from_json(&oversize).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
}

fn gain_treatment(millidecibels: i32) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(millidecibels).unwrap(), false, vec![], vec![]).unwrap(),
    )
}

#[test]
fn frozen_context_rejects_aggregate_gain_records_during_ingress() {
    let segments = (1..=MAX_GAIN_SEGMENTS)
        .map(|end| {
            GainSegment::new(ratio(end as i128, 1), GainDb::UNITY, GainCurve::Linear).unwrap()
        })
        .collect::<Vec<_>>();
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ratio(0, 1), ratio(MAX_GAIN_SEGMENTS as i128, 1)).unwrap(),
        GainDb::UNITY,
        segments,
    )
    .unwrap();
    let treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(
            GainDb::UNITY,
            false,
            vec![envelope; MAX_GAIN_ENVELOPES],
            vec![],
        )
        .unwrap(),
    );
    let owners = MAX_GAIN_RECORDS / treatments.record_count() + 1;
    let context =
        FrozenAudioContext::capture(&fixture(AudioSample(0), SourceAudioMapping::FitBeat)).unwrap();
    let mut wire = serde_json::to_value(context).unwrap();
    wire["audio_treatments"] = json!({});
    for index in 0..owners {
        wire["audio_treatments"][format!("gain-{index}")] = json!(treatments);
    }
    let error = FrozenAudioContext::from_json(&wire.to_string()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("audio treatment record limit exceeded"),
        "{error}"
    );
}

#[test]
fn frozen_gain_retains_group_and_picture_only_owners_and_authenticates_every_recipe() {
    let original = fixture(AudioSample(0), SourceAudioMapping::FitBeat);
    let mut document = original.clone();
    for (owner, db) in [("root", -3000), ("sequence", 6000), ("pictureonly", 0)] {
        document = edit(
            &document,
            Command::SetAudioTreatments {
                node: node(owner),
                treatments: gain_treatment(db),
            },
        );
    }
    let context = FrozenAudioContext::capture(&document).unwrap();
    assert_eq!(context.audio_treatments().len(), 3);
    assert_eq!(
        context.audio_treatments()[&node("pictureonly")],
        gain_treatment(0)
    );
    assert_eq!(
        context.layout(),
        FrozenAudioContext::capture(&original).unwrap().layout()
    );
    assert!(context.matches_document(&document).unwrap());
    assert_eq!(
        FrozenAudioContext::from_json(&context.to_json().unwrap()).unwrap(),
        context
    );
    for mutation in ["missing", "changed"] {
        let mut wire = serde_json::to_value(&context).unwrap();
        if mutation == "missing" {
            wire["audio_treatments"]
                .as_object_mut()
                .unwrap()
                .remove("pictureonly");
        } else {
            wire["audio_treatments"]["root"] = serde_json::to_value(gain_treatment(-6000)).unwrap();
        }
        let forged = FrozenAudioContext::from_json(&wire.to_string()).unwrap();
        assert!(!forged.matches_document(&document).unwrap());
    }
}

#[test]
fn legacy_context_gain_vocabulary_is_closed_even_for_null_empty_or_escaped_names() {
    let document = fixture(AudioSample(0), SourceAudioMapping::FitBeat);
    let context = FrozenAudioContext::capture(&document).unwrap();
    let current = serde_json::to_value(&context).unwrap();
    assert!(current.get("audio_treatments").is_none());
    for version in 1..=3 {
        let mut wire = current.clone();
        wire["schema_version"] = json!(version);
        let old = FrozenAudioContext::from_json(&wire.to_string()).unwrap();
        assert!(old.matches_document(&document).unwrap());
        for value in [Value::Null, json!({}), json!({"root":gain_treatment(0)})] {
            wire["audio_treatments"] = value;
            let encoded = wire.to_string();
            assert!(FrozenAudioContext::from_json(&encoded).is_err());
            assert!(
                FrozenAudioContext::from_json(
                    &encoded.replace("audio_treatments", "audio_treatm\\u0065nts")
                )
                .is_err()
            );
        }
    }
}

#[test]
fn frozen_gain_rejects_unknown_empty_duplicate_and_overlayered_owners() {
    let document = fixture(AudioSample(0), SourceAudioMapping::FitBeat);
    let context = FrozenAudioContext::capture(&document).unwrap();
    for value in [
        json!({"missing":gain_treatment(0)}),
        json!({"root":AudioTreatments::default()}),
        Value::Null,
    ] {
        let mut wire = serde_json::to_value(&context).unwrap();
        wire["audio_treatments"] = value;
        assert!(FrozenAudioContext::from_json(&wire.to_string()).is_err());
    }
    let mut wire = serde_json::to_value(&context).unwrap();
    wire["audio_treatments"] = json!({"root":gain_treatment(0)});
    let recipe = wire["audio_treatments"]["root"].to_string();
    let duplicate = wire.to_string().replace(
        &format!("\"audio_treatments\":{{\"root\":{recipe}}}"),
        &format!("\"audio_treatments\":{{\"root\":{recipe},\"root\":{recipe}}}"),
    );
    assert_ne!(duplicate, wire.to_string());
    assert!(FrozenAudioContext::from_json(&duplicate).is_err());

    // A valid timing-only context can have more depth than the gain-layer cap.
    let mut deep = document;
    for index in 0..MAX_GAIN_LAYERS {
        deep = edit(
            &deep,
            Command::Group {
                parent: node("root"),
                start: 0,
                end: 1,
                id: node(&format!("gain-group-{index}")),
                label: "gain owner".into(),
            },
        );
    }
    let mut wire = serde_json::to_value(FrozenAudioContext::capture(&deep).unwrap()).unwrap();
    wire["audio_treatments"] = json!({"root": gain_treatment(0)});
    for index in 0..MAX_GAIN_LAYERS {
        wire["audio_treatments"][format!("gain-group-{index}")] = json!(gain_treatment(0));
    }
    assert_eq!(
        FrozenAudioContext::from_json(&wire.to_string())
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
}

#[test]
fn compact_repeat_overrides_and_lineage_survive_capture() {
    let base = fixture(AudioSample(0), SourceAudioMapping::FitBeat);
    let NodeKind::Repeat { iterations, .. } = &base.nodes()[&node("repeat")].kind else {
        panic!()
    };
    let iteration = iterations.at(999_999_999).unwrap();
    let mut value: Value = serde_json::from_str(&base.to_json().unwrap()).unwrap();
    value["nodes"]["alternate"] = serde_json::to_value(BeatNode::hold(
        "alternate",
        HoldRecipe {
            picture_context: None,
            duration: frames(10),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    value["overrides"]["repeat"] = serde_json::to_value(
        PlayOverrides::try_from(vec![PlayOverride {
            iteration,
            root: node("alternate"),
        }])
        .unwrap(),
    )
    .unwrap();
    value["audio_lineage"]["source"] = json!({"allocation":"originrev","origin":"source"});
    let document = ProjectDocument::from_json(&value.to_string()).unwrap();
    let snapshot = FrozenAudioContext::capture(&document).unwrap();
    assert_eq!(snapshot.layout().overrides(), document.overrides());
    assert_eq!(snapshot.layout().audio_lineage(), document.audio_lineage());
    assert_eq!(
        FrozenAudioContext::from_json(&snapshot.to_json().unwrap()).unwrap(),
        snapshot
    );
    assert_eq!(snapshot.layout().duration(), document.duration().unwrap());
}

#[test]
fn copied_snapshot_is_independent_of_later_document_revision() {
    let document = fixture(AudioSample(-1), SourceAudioMapping::FitBeat);
    let before = FrozenAudioContext::capture(&document).unwrap();
    let updated = edit(
        &document,
        Command::AddAsset {
            id: asset("later"),
            asset: record("later", true),
        },
    );
    assert_ne!(updated.revision_id(), before.revision_id());
    assert_eq!(
        FrozenAudioContext::from_json(&before.to_json().unwrap()).unwrap(),
        before
    );
    assert!(!before.assets().contains_key(&asset("later")));
}

#[test]
fn effective_placement_can_exceed_authored_placement_bounds() {
    let document = fixture(
        AudioSample(48_000),
        SourceAudioMapping::Placement {
            start: ratio(i128::from(i64::MAX - 20), 1),
            frames: ratio(10, 1),
        },
    );
    let context = FrozenAudioContext::capture(&document).unwrap();
    let FrozenAudioKind::Source {
        placement: Some(placement),
    } = context.layout().nodes()[&node("source")].kind
    else {
        panic!()
    };
    assert!(placement.start.numerator() > i128::from(i64::MAX));
    assert!(matches!(
        context.inputs()[&node("source")],
        FrozenAudioInput::Source {
            mapping: SourceAudioMapping::Placement { .. },
            offset: AudioSample(48_000),
            ..
        }
    ));
    assert_eq!(
        FrozenAudioContext::from_json(&context.to_json().unwrap()).unwrap(),
        context
    );
}

#[test]
fn dormant_context_keeps_input_asset_offset_and_distinguishes_absent_audio() {
    let mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: ExactRatio::integer(20),
        selection: ExactFrameRange {
            start: ratio(2, 1),
            end: ratio(2, 1),
        },
    };
    let document = fixture(AudioSample(16016), mapping);
    let context = FrozenAudioContext::capture(&document).unwrap();
    assert_eq!(
        context.inputs()[&node("source")],
        FrozenAudioInput::Source {
            source: SourceAudio {
                asset: asset("original"),
                span: span()
            },
            mapping,
            offset: AudioSample(16016),
        }
    );
    assert!(context.assets().contains_key(&asset("original")));
    assert!(matches!(&context.layout().nodes()[&node("source")].kind,
        FrozenAudioKind::Source { placement: Some(placement) } if placement.start == ratio(12, 1) && placement.end == ratio(12, 1)));
    assert!(!context.inputs().contains_key(&node("pictureonly")));
    assert_eq!(
        FrozenAudioContext::from_json(&context.to_json().unwrap()).unwrap(),
        context
    );
    let wire = serde_json::to_value(&context).unwrap();
    assert_eq!(wire["schema_version"], json!(6));
    for version in 1..=4 {
        let mut forged = wire.clone();
        forged["schema_version"] = json!(version);
        assert!(FrozenAudioContext::from_json(&forged.to_string()).is_err());
        // Reject a dormant frozen layout even when the input mapping itself is
        // legal old vocabulary. New meaning cannot hide in the retained layout.
        forged["inputs"]["source"]["mapping"] = json!({"type":"fit_beat"});
        assert!(FrozenAudioContext::from_json(&forged.to_string()).is_err());
    }
}

#[test]
fn positive_audio_context_support_keeps_all_pre_dormant_versions_readable() {
    for mapping in [
        SourceAudioMapping::FitBeat,
        SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: ratio(20, 1),
            selection: ExactFrameRange {
                start: ratio(2, 1),
                end: ratio(4, 1),
            },
        },
    ] {
        let document = fixture(AudioSample(0), mapping);
        let context = FrozenAudioContext::capture(&document).unwrap();
        for version in 1..=4 {
            let mut wire = serde_json::to_value(&context).unwrap();
            wire["schema_version"] = json!(version);
            let result = FrozenAudioContext::from_json(&wire.to_string());
            if version == 1 && matches!(mapping, SourceAudioMapping::SelectedPlacement { .. }) {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().matches_document(&document).unwrap());
            }
        }
    }
}
