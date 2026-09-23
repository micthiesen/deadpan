use deadpan_core::*;
use serde_json::{Value, json};

type UpgradeRequest = fn(&str) -> Result<CommandRequest, DocumentError>;

const REQUESTS: [UpgradeRequest; 13] = [
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
    legacy_v12::upgrade_request,
    legacy_v13::upgrade_request,
];

fn request(command: Value) -> Value {
    json!({"project_id":"legacy-split", "expected_revision":"initial", "new_revision":"next", "command":command})
}

#[test]
fn all_thirteen_legacy_command_grammars_reject_split_and_its_fields() {
    for (index, upgrade) in REQUESTS.into_iter().enumerate() {
        let version = index + 1;
        let rename = request(json!({"command":"rename", "node":"first", "label":"Kept"}));
        assert!(upgrade(&rename.to_string()).is_ok(), "schema {version}");
        for at in [json!(4), Value::Null] {
            for identities in [json!({"nodes":["left","right"]}), Value::Null] {
                let split = request(
                    json!({"command":"split", "node":"first", "at":at, "identities":identities}),
                );
                assert!(upgrade(&split.to_string()).is_err(), "schema {version}");
                let occurrence = request(
                    json!({"command":"edit_occurrence", "instance":{"node":"first","repeats":[]}, "edit":{"type":"split","at":at,"identities":identities}, "identities":{"nodes":[],"marks":[]}}),
                );
                assert!(
                    upgrade(&occurrence.to_string()).is_err(),
                    "schema {version}"
                );
            }
        }
        for field in ["split", "at", "identities"] {
            for value in [Value::Null, json!([])] {
                let mut forged = rename.clone();
                forged["command"][field] = value.clone();
                assert!(
                    upgrade(&forged.to_string()).is_err(),
                    "schema {version}: {field}"
                );
                let mut occurrence = request(
                    json!({"command":"edit_occurrence", "instance":{"node":"first","repeats":[]}, "edit":{"type":"rename","label":"Kept"}, "identities":{"nodes":[],"marks":[]}}),
                );
                if version >= 4 {
                    assert!(upgrade(&occurrence.to_string()).is_ok(), "schema {version}");
                }
                occurrence["command"]["edit"][field] = value;
                assert!(
                    upgrade(&occurrence.to_string()).is_err(),
                    "schema {version}: {field}"
                );
            }
        }
    }
}

#[test]
fn legacy_fieldless_occurrence_commands_do_not_ignore_split_payloads() {
    for (index, upgrade) in REQUESTS.into_iter().enumerate().skip(3) {
        let version = index + 1;
        for kind in ["delete", "ungroup", "revert_generated_hold"] {
            if version == 4 && kind == "revert_generated_hold" {
                continue;
            }
            let original = request(
                json!({"command":"edit_occurrence", "instance":{"node":"first","repeats":[]}, "edit":{"type":kind}, "identities":{"nodes":[],"marks":[]}}),
            );
            assert!(
                upgrade(&original.to_string()).is_ok(),
                "schema {version}: {kind}"
            );
            for field in ["split", "at", "identities"] {
                let mut forged = original.clone();
                forged["command"]["edit"][field] = Value::Null;
                assert!(
                    upgrade(&forged.to_string()).is_err(),
                    "schema {version}: {kind}.{field}"
                );
            }
        }
    }
}

fn wire() -> Value {
    let hold = json!({"label":"Hold", "kind":{"type":"hold", "recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}});
    let local = |node: &str| json!({"space":"local","node":node,"position":ExactRatio::integer(5)});
    json!({"schema_version":13,"project_id":"legacy-split","revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":24,"denominator":1},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root","nodes":{"root":{"label":"Sequence","kind":{"type":"sequence","children":["first","second","third"]}},"first":hold,"second":hold,"third":hold},"assets":{},"overrides":{},
        "marks":{"cue":{"owner":"first","label":"Logical cue","boundary":{"coordinate":local("first"),"bias":"right"},"loss_policy":"delete_owned","state":{"type":"bound"},"fragments":[{"owner":"second","coordinate":local("second"),"state":{"type":"bound"}},{"owner":"third","coordinate":{"space":"occurrence","instance":{"node":"third","repeats":[]},"position":ExactRatio::integer(5)},"state":{"type":"bound"}}]}}})
}

#[test]
fn schema13_retains_fragments_and_compares_every_history_patch_binding() {
    let old = wire();
    let frozen = legacy_v13::Document::from_json(&old.to_string()).unwrap();
    let document = frozen.clone().upgrade().unwrap();
    assert!(frozen.matches(&document));
    let mut expected = old;
    expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
    assert_eq!(serde_json::to_value(&document).unwrap(), expected);
    let request: CommandRequest =
        serde_json::from_value(request(json!({"command":"delete","node":"first"}))).unwrap();
    let edit = apply(&document, &request).unwrap();
    assert!(legacy_v13::matches_edit(&serde_json::to_string(&edit).unwrap(), &edit).unwrap());
    let next = edit.forward.apply(&document).unwrap();
    assert_eq!(
        next.marks()[&MarkId::new("cue").unwrap()].binding_count(),
        2
    );
    assert_eq!(edit.inverse.apply(&next).unwrap(), document);
    for direction in ["forward", "inverse"] {
        for side in ["before", "after"] {
            for field in ["future", "bias", "label", "loss_policy"] {
                let mut forged = serde_json::to_value(&edit).unwrap();
                forged[direction]["marks"]["cue"][side]["fragments"][0][field] = Value::Null;
                assert!(
                    legacy_v13::matches_edit(&forged.to_string(), &edit).is_err(),
                    "{direction}.{side}.{field}"
                );
            }
            let mut changed = edit.clone();
            let patch = if direction == "forward" {
                &mut changed.forward
            } else {
                &mut changed.inverse
            };
            let value = patch.marks.get_mut(&MarkId::new("cue").unwrap()).unwrap();
            let mark = if side == "before" {
                value.before.as_mut().unwrap()
            } else {
                value.after.as_mut().unwrap()
            };
            mark.fragments[0].coordinate = Anchor::Local {
                node: NodeId::new("third").unwrap(),
                position: ExactRatio::integer(6),
            };
            assert!(
                !legacy_v13::matches_edit(&serde_json::to_string(&edit).unwrap(), &changed)
                    .unwrap()
            );
        }
    }
}

#[test]
fn schema13_freezes_fragment_fields_states_and_coordinates() {
    for pointer in [
        "/marks/cue",
        "/marks/cue/boundary",
        "/marks/cue/boundary/coordinate",
        "/marks/cue/state",
        "/marks/cue/fragments/0",
        "/marks/cue/fragments/0/state",
        "/marks/cue/fragments/0/coordinate",
        "/marks/cue/fragments/1/coordinate/instance",
    ] {
        let mut forged = wire();
        forged.pointer_mut(pointer).unwrap()["future"] = Value::Null;
        assert!(
            legacy_v13::Document::from_json(&forged.to_string()).is_err(),
            "{pointer}"
        );
    }
    for (pointer, value) in [
        ("/marks/cue/fragments", Value::Null),
        ("/marks/cue/fragments/0/state", json!({"type":"future"})),
        (
            "/marks/cue/fragments/0/state",
            json!({"type":"unresolved","reason":"future"}),
        ),
        (
            "/marks/cue/fragments/0/coordinate",
            json!({"space":"future"}),
        ),
    ] {
        let mut forged = wire();
        *forged.pointer_mut(pointer).unwrap() = value;
        assert!(
            legacy_v13::Document::from_json(&forged.to_string()).is_err(),
            "{pointer}"
        );
    }
    let mut single = wire();
    single["marks"]["cue"]
        .as_object_mut()
        .unwrap()
        .remove("fragments");
    let absent = legacy_v13::Document::from_json(&single.to_string())
        .unwrap()
        .upgrade()
        .unwrap();
    single["marks"]["cue"]["fragments"] = json!([]);
    let empty = legacy_v13::Document::from_json(&single.to_string())
        .unwrap()
        .upgrade()
        .unwrap();
    assert_eq!(absent, empty);
    assert!(!empty.to_json().unwrap().contains("\"fragments\""));
}
