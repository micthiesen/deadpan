use deadpan_core::*;
use serde_json::{Value, json};

fn request(command: Value) -> Value {
    json!({"project_id":"legacy-lineage", "expected_revision":"initial", "new_revision":"next", "command":command})
}

#[test]
fn schema14_split_and_occurrence_split_keep_closed_identity_vocabulary() {
    for occurrence in [false, true] {
        let identities = json!({"nodes":["left", "right", "copy"]});
        let mut wire = request(if occurrence {
            json!({"command":"edit_occurrence", "instance":{"node":"hold", "repeats":[]}, "edit":{"type":"split", "at":5, "identities":identities}, "identities":{"nodes":[], "marks":[]}})
        } else {
            json!({"command":"split", "node":"hold", "at":5, "identities":identities})
        });
        let upgraded = legacy_v14::upgrade_request(&wire.to_string()).unwrap();
        assert!(matches!(
            upgraded.command,
            Command::Split { .. }
                | Command::EditOccurrence {
                    edit: OccurrenceEdit::Split { .. },
                    ..
                }
        ));
        assert!(legacy_v13::upgrade_request(&wire.to_string()).is_err());
        let path = if occurrence {
            "/command/edit/identities"
        } else {
            "/command/identities"
        };
        for field in ["audio_lineage", "marks", "future"] {
            for value in [Value::Null, json!({})] {
                wire.pointer_mut(path).unwrap()[field] = value;
                assert!(legacy_v14::upgrade_request(&wire.to_string()).is_err());
                wire.pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
            }
        }
        let path = if occurrence {
            "/command/edit"
        } else {
            "/command"
        };
        wire.pointer_mut(path).unwrap()["audio_lineage"] = json!({});
        assert!(legacy_v14::upgrade_request(&wire.to_string()).is_err());
    }
}

#[test]
fn schema14_import_subtrees_cannot_claim_audio_lineage() {
    let hold = json!({"label":"Hold", "kind":{"type":"hold", "recipe":{"duration":10, "video":{"type":"background"}, "audio":{"type":"silence"}}}});
    for occurrence in [false, true] {
        let subtree = json!({"root":"hold", "nodes":{"hold":hold}});
        let mut wire = request(if occurrence {
            json!({"command":"edit_occurrence", "instance":{"node":"root", "repeats":[]}, "edit":{"type":"insert", "index":0, "subtree":subtree}, "identities":{"nodes":[], "marks":[]}})
        } else {
            json!({"command":"insert", "parent":"root", "index":0, "subtree":subtree})
        });
        assert!(legacy_v14::upgrade_request(&wire.to_string()).is_ok());
        let path = if occurrence {
            "/command/edit/subtree"
        } else {
            "/command/subtree"
        };
        for value in [
            Value::Null,
            json!({}),
            json!({"hold":{"allocation":"old", "origin":"hold"}}),
        ] {
            wire.pointer_mut(path).unwrap()["audio_lineage"] = value;
            assert!(legacy_v14::upgrade_request(&wire.to_string()).is_err());
        }
    }
}
