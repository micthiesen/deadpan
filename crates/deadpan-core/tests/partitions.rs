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
