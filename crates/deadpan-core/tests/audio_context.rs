use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::{Value, json};

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
        label: "private source label".into(),
        audio_edges: AudioEdgePolicies {
            source_placement_start: AudioEdgePolicy::Hard,
            ..Default::default()
        },
        kind: NodeKind::Source {
            source: SourceNode {
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
    };
    let picture_only = BeatNode {
        label: "picture only".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
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
    };
    let hold = BeatNode::hold(
        "tail",
        HoldRecipe {
            duration: frames(3),
            video: HoldVideo::Background,
            audio: HoldAudio::Tail {
                source: source.clone(),
                maximum: frames(2),
            },
        },
    );
    let repeat = BeatNode {
        label: "repeat".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: node("source"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 1_000_000_000)
                .unwrap(),
            gap: Some(HoldRecipe {
                duration: frames(1),
                video: HoldVideo::Background,
                audio: HoldAudio::RoomTone {
                    source: source.clone(),
                },
            }),
        },
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
            },
        },
    )
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
fn ingress_rejects_open_or_inconsistent_inventory() {
    let context =
        FrozenAudioContext::capture(&fixture(AudioSample(0), SourceAudioMapping::FitBeat)).unwrap();
    let mut value: Value = serde_json::from_str(&context.to_json().unwrap()).unwrap();
    let baseline = value.clone();
    value["schema_version"] = json!(2);
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
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
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
