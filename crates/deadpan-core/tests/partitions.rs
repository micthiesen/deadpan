use deadpan_core::*;
use serde_json::{Value, json};

fn wire(version: u32) -> Value {
    let mut value = json!({
        "schema_version": version, "project_id": "partitions", "revision_id": "initial",
        "presentation_basis": {"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
        "root":"root", "nodes": {
            "root":{"label":"Sequence","kind":{"type":"sequence","children":["crop"]}},
            "crop":{"label":"Crop","kind":{"type":"retime","child":"hold","duration":4,"mapping":{"start":2,"end":6},"pitch":"preserve"}},
            "hold":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}}
        }, "assets":{}
    });
    if version >= 3 {
        value["marks"] = json!({});
    }
    if version >= 4 {
        value["overrides"] = json!({});
    }
    if version >= 10 {
        value["basis_state"] =
            json!({"rate_origin":"explicit","geometry_origin":"explicit","primary":null});
    }
    value
}

fn document(purpose: RetimePurpose) -> ProjectDocument {
    let mut value = wire(DOCUMENT_SCHEMA_VERSION);
    value["nodes"]["crop"]["kind"]["purpose"] = serde_json::to_value(purpose).unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}

fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("next").unwrap(),
        command,
    }
}

#[test]
fn transparent_partition_requires_unity_and_cannot_author_edges() {
    let original = document(RetimePurpose::Partition);
    assert_eq!(original.duration().unwrap().frames(), 4);
    let before = original.to_json().unwrap();
    assert_eq!(ProjectDocument::from_json(&before).unwrap(), original);
    for duration in [3, 5] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["nodes"]["crop"]["kind"]["duration"] = json!(duration);
        assert!(ProjectDocument::from_json(&value.to_string()).is_err());
    }
    for edge in [AudioBoundaryKind::NodeStart, AudioBoundaryKind::NodeEnd] {
        let command = Command::SetAudioEdge {
            node: NodeId::new("crop").unwrap(),
            edge,
            policy: AudioEdgePolicy::Hard,
        };
        assert!(apply(&original, &request(&original, command)).is_err());
        assert_eq!(original.to_json().unwrap(), before);
    }
    let mut value = serde_json::to_value(&original).unwrap();
    value["nodes"]["crop"]["kind"]["mapping"] = json!({"start":8,"end":12});
    assert!(ProjectDocument::from_json(&value.to_string()).is_err());
}

#[test]
fn partition_metadata_survives_reversible_commands_and_ordinary_retime_stays_default() {
    let original = document(RetimePurpose::Partition);
    let edit = apply(
        &original,
        &request(
            &original,
            Command::Rename {
                node: NodeId::new("crop").unwrap(),
                label: "Retained processing domain".into(),
            },
        ),
    )
    .unwrap();
    let encoded = serde_json::to_string(&edit).unwrap();
    let decoded: EditTransaction = serde_json::from_str(&encoded).unwrap();
    let after = decoded.forward.apply(&original).unwrap();
    assert_eq!(decoded.inverse.apply(&after).unwrap(), original);
    assert!(matches!(
        &after.nodes()[&NodeId::new("crop").unwrap()].kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ));
    let old = wire(DOCUMENT_SCHEMA_VERSION);
    let parsed = ProjectDocument::from_json(&old.to_string()).unwrap();
    assert_eq!(parsed, document(RetimePurpose::Edit));
    assert!(!parsed.to_json().unwrap().contains("purpose"));
    let mut slow = old;
    slow["nodes"]["crop"]["kind"]["duration"] = json!(9);
    let slow = ProjectDocument::from_json(&slow.to_string()).unwrap();
    assert_eq!(slow.duration().unwrap().frames(), 9);
    assert!(
        apply(
            &slow,
            &request(
                &slow,
                Command::SetAudioEdge {
                    node: NodeId::new("crop").unwrap(),
                    edge: AudioBoundaryKind::NodeEnd,
                    policy: AudioEdgePolicy::Hard,
                }
            )
        )
        .is_ok()
    );
}

macro_rules! legacy_document {
    ($version:expr, $wire:expr, $old:ident, $body:expr) => {
        match $version {
            1 => {
                let $old = legacy_v1::Document::from_json($wire)?;
                $body
            }
            2 => {
                let $old = legacy_v2::Document::from_json($wire)?;
                $body
            }
            3 => {
                let $old = legacy_v3::Document::from_json($wire)?;
                $body
            }
            4 => {
                let $old = legacy_v4::Document::from_json($wire)?;
                $body
            }
            5 => {
                let $old = legacy_v5::Document::from_json($wire)?;
                $body
            }
            6 => {
                let $old = legacy_v6::Document::from_json($wire)?;
                $body
            }
            7 => {
                let $old = legacy_v7::Document::from_json($wire)?;
                $body
            }
            8 => {
                let $old = legacy_v8::Document::from_json($wire)?;
                $body
            }
            9 => {
                let $old = legacy_v9::Document::from_json($wire)?;
                $body
            }
            10 => {
                let $old = legacy_v10::Document::from_json($wire)?;
                $body
            }
            11 => {
                let $old = legacy_v11::Document::from_json($wire)?;
                $body
            }
            _ => unreachable!(),
        }
    };
}
fn upgrade(version: u32, value: &Value) -> Result<ProjectDocument, DocumentError> {
    legacy_document!(version, &value.to_string(), old, old.upgrade())
}
fn matches(version: u32, value: &Value, modern: &ProjectDocument) -> Result<bool, DocumentError> {
    legacy_document!(version, &value.to_string(), old, Ok(old.matches(modern)))
}
type RequestAdapter = fn(&str) -> Result<CommandRequest, DocumentError>;
const REQUESTS: [RequestAdapter; 11] = [
    legacy_v1::upgrade_request,
    legacy_v2::upgrade_request,
    legacy_v3::upgrade_request,
    legacy_v4::upgrade_request,
    legacy_v5::upgrade_request,
    legacy_v6::upgrade_request,
    legacy_v7::upgrade_request,
    legacy_v8::upgrade_request,
    legacy_v9::upgrade_request,
    legacy_v10::upgrade_request,
    legacy_v11::upgrade_request,
];
type EditAdapter = fn(&str, &EditTransaction) -> Result<bool, DocumentError>;
const EDITS: [EditAdapter; 11] = [
    legacy_v1::matches_edit,
    legacy_v2::matches_edit,
    legacy_v3::matches_edit,
    legacy_v4::matches_edit,
    legacy_v5::matches_edit,
    legacy_v6::matches_edit,
    legacy_v7::matches_edit,
    legacy_v8::matches_edit,
    legacy_v9::matches_edit,
    legacy_v10::matches_edit,
    legacy_v11::matches_edit,
];

#[test]
fn all_legacy_documents_preserve_authored_crops_and_reject_partition_vocabulary() {
    for version in 1..=11 {
        let value = wire(version);
        let upgraded = upgrade(version, &value).unwrap();
        assert!(matches(version, &value, &upgraded).unwrap());
        assert!(matches!(
            &upgraded.nodes()[&NodeId::new("crop").unwrap()].kind,
            NodeKind::Retime {
                purpose: RetimePurpose::Edit,
                ..
            }
        ));
        let mut partition = serde_json::to_value(&upgraded).unwrap();
        partition["nodes"]["crop"]["kind"]["purpose"] = json!("partition");
        let partition = ProjectDocument::from_json(&partition.to_string()).unwrap();
        assert!(!matches(version, &value, &partition).unwrap());
        for purpose in [Value::Null, json!("edit"), json!("partition")] {
            let mut forged = value.clone();
            forged["nodes"]["crop"]["kind"]["purpose"] = purpose;
            assert!(upgrade(version, &forged).is_err(), "schema {version}");
        }
    }
}

#[test]
fn all_legacy_subtree_and_occurrence_requests_reject_new_purpose() {
    for (index, adapter) in REQUESTS.into_iter().enumerate() {
        let version = u32::try_from(index + 1).unwrap();
        let mut subtree = json!({"root":"root", "nodes":wire(version)["nodes"]});
        if version >= 4 {
            subtree["overrides"] = json!({});
        }
        let mut commands =
            vec![json!({"command":"insert","parent":"root","index":0,"subtree":subtree})];
        if version >= 4 {
            commands.extend([
                json!({"command":"set_play_override","node":"repeat","iteration":{"allocation":"initial","ordinal":0},"subtree":subtree}),
                json!({"command":"edit_occurrence","instance":{"node":"root","repeats":[]},"edit":{"type":"insert","index":0,"subtree":subtree},"identities":{"nodes":[],"marks":[]}}),
                json!({"command":"edit_occurrence","instance":{"node":"repeat","repeats":[]},"edit":{"type":"set_play_override","iteration":{"allocation":"initial","ordinal":0},"subtree":subtree},"identities":{"nodes":[],"marks":[]}}),
            ]);
        }
        for command in commands {
            let old = json!({"project_id":"partitions","expected_revision":"initial","new_revision":"next","command":command});
            assert!(adapter(&old.to_string()).is_ok(), "schema {version}");
            for purpose in [Value::Null, json!("edit"), json!("partition")] {
                let mut forged = old.clone();
                let command = &mut forged["command"];
                let subtree = if command["command"] == "edit_occurrence" {
                    &mut command["edit"]["subtree"]
                } else {
                    &mut command["subtree"]
                };
                subtree["nodes"]["crop"]["kind"]["purpose"] = purpose;
                assert!(
                    adapter(&forged.to_string()).is_err(),
                    "schema {version}: {forged}"
                );
            }
        }
    }
}

#[test]
fn all_legacy_forward_and_inverse_patches_require_ordinary_retime_purpose() {
    for (index, adapter) in EDITS.into_iter().enumerate() {
        let version = u32::try_from(index + 1).unwrap();
        let original = upgrade(version, &wire(version)).unwrap();
        let edit = apply(
            &original,
            &request(
                &original,
                Command::Rename {
                    node: NodeId::new("crop").unwrap(),
                    label: "Renamed".into(),
                },
            ),
        )
        .unwrap();
        let mut old = serde_json::to_value(&edit).unwrap();
        for direction in ["forward", "inverse"] {
            if version < 3 {
                old[direction].as_object_mut().unwrap().remove("marks");
            }
            if version < 4 {
                old[direction].as_object_mut().unwrap().remove("overrides");
            }
        }
        assert!(
            adapter(&old.to_string(), &edit).unwrap(),
            "schema {version}"
        );
        for direction in ["forward", "inverse"] {
            for side in ["before", "after"] {
                for purpose in [Value::Null, json!("edit"), json!("partition")] {
                    let mut forged = old.clone();
                    forged[direction]["nodes"]["crop"][side]["kind"]["purpose"] = purpose;
                    assert!(
                        adapter(&forged.to_string(), &edit).is_err(),
                        "schema {version}"
                    );
                }
                let mut changed = serde_json::to_value(&edit).unwrap();
                changed[direction]["nodes"]["crop"][side]["kind"]["purpose"] = json!("partition");
                let changed = serde_json::from_value(changed).unwrap();
                assert!(
                    !adapter(&old.to_string(), &changed).unwrap(),
                    "schema {version}"
                );
            }
        }
    }
}
