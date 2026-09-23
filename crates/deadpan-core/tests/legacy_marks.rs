use deadpan_core::*;
use serde_json::{Value, json};
use std::fmt::Write;

type Upgrade = fn(&str) -> Result<ProjectDocument, DocumentError>;
type Matches = fn(&str, &ProjectDocument) -> Result<bool, DocumentError>;
type MatchesEdit = fn(&str, &EditTransaction) -> Result<bool, DocumentError>;
type UpgradeRequest = fn(&str) -> Result<CommandRequest, DocumentError>;

struct Adapter {
    version: u32,
    upgrade: Upgrade,
    matches: Matches,
    matches_edit: MatchesEdit,
    request: UpgradeRequest,
}

macro_rules! adapter {
    ($version:literal, $module:ident) => {
        Adapter {
            version: $version,
            upgrade: |json| $module::Document::from_json(json)?.upgrade(),
            matches: |json, current| Ok($module::Document::from_json(json)?.matches(current)),
            matches_edit: $module::matches_edit,
            request: $module::upgrade_request,
        }
    };
}

const ADAPTERS: [Adapter; 10] = [
    adapter!(3, legacy_v3),
    adapter!(4, legacy_v4),
    adapter!(5, legacy_v5),
    adapter!(6, legacy_v6),
    adapter!(7, legacy_v7),
    adapter!(8, legacy_v8),
    adapter!(9, legacy_v9),
    adapter!(10, legacy_v10),
    adapter!(11, legacy_v11),
    adapter!(12, legacy_v12),
];

fn node(id: &str) -> NodeId {
    NodeId::new(id).unwrap()
}
fn mark_id() -> MarkId {
    MarkId::new("cue").unwrap()
}

fn wire(version: u32) -> Value {
    let mut value = json!({
        "schema_version": version, "project_id":"legacy-marks", "revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":24,"denominator":1},"color_policy":"sdr_rec709"},
        "root":"root", "nodes": {
            "root":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}},
            "hold":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}}
        }, "assets":{},
        "marks":{"cue":{
            "owner":"hold", "label":"Cue", "boundary":{"coordinate":{"space":"local","node":"hold","position":ExactRatio::integer(5)},"bias":"right"},
            "loss_policy":"keep_unresolved", "state":{"type":"bound"}
        }}
    });
    if version >= 4 {
        value["overrides"] = json!({});
    }
    if version >= 10 {
        value["basis_state"] =
            json!({"rate_origin":"explicit","geometry_origin":"explicit","primary":null});
    }
    value
}

fn fragment() -> MarkFragment {
    MarkFragment {
        owner: node("hold"),
        coordinate: Anchor::Local {
            node: node("hold"),
            position: ExactRatio::integer(6),
        },
        state: MarkState::Bound,
    }
}

fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("next").unwrap(),
        command,
    }
}

fn mark_request(document: &ProjectDocument) -> CommandRequest {
    request(
        document,
        Command::SetMark {
            id: mark_id(),
            owner: node("hold"),
            label: "Changed cue".into(),
            boundary: document.marks()[&mark_id()].boundary.clone(),
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
}

fn edit_wire(version: u32, edit: &EditTransaction) -> Value {
    let mut value = serde_json::to_value(edit).unwrap();
    if version < 4 {
        for direction in ["forward", "inverse"] {
            value[direction]
                .as_object_mut()
                .unwrap()
                .remove("overrides");
        }
    }
    value
}

#[test]
fn bound_mark_state_rejects_unrecognized_payloads_in_documents_and_history() {
    assert_eq!(
        serde_json::to_value(MarkState::Bound).unwrap(),
        json!({"type":"bound"})
    );
    for adapter in ADAPTERS
        .into_iter()
        .chain(std::iter::once(adapter!(13, legacy_v13)))
    {
        let old = wire(adapter.version);
        let document = (adapter.upgrade)(&old.to_string()).unwrap();
        let edit = apply(&document, &mark_request(&document)).unwrap();
        for field in ["future", "reason", "fragments"] {
            let mut forged = old.clone();
            forged["marks"]["cue"]["state"][field] = Value::Null;
            assert!(
                (adapter.upgrade)(&forged.to_string()).is_err(),
                "schema {}: {field}",
                adapter.version
            );
            for direction in ["forward", "inverse"] {
                for side in ["before", "after"] {
                    let mut patch = edit_wire(adapter.version, &edit);
                    patch[direction]["marks"]["cue"][side]["state"][field] = Value::Null;
                    assert!(
                        (adapter.matches_edit)(&patch.to_string(), &edit).is_err(),
                        "schema {}: {direction}.{side}.{field}",
                        adapter.version
                    );
                }
            }
        }
    }
    let mut modern = wire(DOCUMENT_SCHEMA_VERSION);
    modern["marks"]["cue"]["fragments"] = json!([fragment()]);
    let before = ProjectDocument::from_json(&modern.to_string()).unwrap();
    for state in ["/marks/cue/state", "/marks/cue/fragments/0/state"] {
        let mut forged = modern.clone();
        forged.pointer_mut(state).unwrap()["future"] = Value::Null;
        assert!(ProjectDocument::from_json(&forged.to_string()).is_err());
    }
    assert_eq!(
        ProjectDocument::from_json(&before.to_json().unwrap()).unwrap(),
        before
    );
}

#[test]
fn every_old_mark_document_rejects_fragments_even_empty_or_null() {
    for adapter in ADAPTERS {
        let old = wire(adapter.version);
        let upgraded = (adapter.upgrade)(&old.to_string()).unwrap();
        assert!(upgraded.marks()[&mark_id()].fragments.is_empty());
        assert!((adapter.matches)(&old.to_string(), &upgraded).unwrap());
        for fragments in [Value::Null, json!([]), json!([fragment()])] {
            let mut forged = old.clone();
            forged["marks"]["cue"]["fragments"] = fragments;
            assert!(
                (adapter.upgrade)(&forged.to_string()).is_err(),
                "schema {}",
                adapter.version
            );
        }
        let mut modern = serde_json::to_value(&upgraded).unwrap();
        modern["marks"]["cue"]["fragments"] = json!([fragment()]);
        let modern = ProjectDocument::from_json(&modern.to_string()).unwrap();
        assert!(
            !(adapter.matches)(&old.to_string(), &modern).unwrap(),
            "schema {} projected away a binding",
            adapter.version
        );
    }
}

#[test]
fn schema12_upgrades_the_full_old_mark_limit_without_extra_bindings() {
    let mut template = wire(12);
    let mark = template["marks"]["cue"].to_string();
    template["marks"] = Value::Null;
    // Build the bounded old wire directly instead of holding 100,000 nested
    // serde_json::Value objects alongside both validated document versions.
    let mut marks = String::from("{");
    for index in 0..MAX_DOCUMENT_MARKS {
        if index > 0 {
            marks.push(',');
        }
        write!(&mut marks, "\"cue-{index}\":{mark}").unwrap();
    }
    marks.push('}');
    let json = template
        .to_string()
        .replace("\"marks\":null", &format!("\"marks\":{marks}"));
    assert!(json.len() < MAX_DOCUMENT_JSON_BYTES);
    let document = legacy_v12::Document::from_json(&json)
        .unwrap()
        .upgrade()
        .unwrap();
    assert_eq!(document.marks().len(), MAX_DOCUMENT_MARKS);
    assert!(
        document
            .marks()
            .values()
            .all(|mark| mark.fragments.is_empty())
    );
    assert!(!document.to_json().unwrap().contains("\"fragments\""));
}

#[test]
fn every_old_mark_patch_rejects_fragments_in_both_sides_and_directions() {
    for adapter in ADAPTERS {
        let document = (adapter.upgrade)(&wire(adapter.version).to_string()).unwrap();
        let request = mark_request(&document);
        let edit = apply(&document, &request).unwrap();
        let old = edit_wire(adapter.version, &edit);
        assert!((adapter.matches_edit)(&old.to_string(), &edit).unwrap());
        assert_eq!(
            (adapter.request)(&serde_json::to_string(&request).unwrap()).unwrap(),
            request
        );
        for direction in ["forward", "inverse"] {
            for side in ["before", "after"] {
                for fragments in [Value::Null, json!([]), json!([fragment()])] {
                    let mut forged = old.clone();
                    forged[direction]["marks"]["cue"][side]["fragments"] = fragments;
                    assert!(
                        (adapter.matches_edit)(&forged.to_string(), &edit).is_err(),
                        "schema {} {direction}.{side}",
                        adapter.version
                    );
                }
                let mut modern = edit.clone();
                let patch = if direction == "forward" {
                    &mut modern.forward
                } else {
                    &mut modern.inverse
                };
                let change = patch.marks.get_mut(&mark_id()).unwrap();
                let mark = if side == "before" {
                    change.before.as_mut().unwrap()
                } else {
                    change.after.as_mut().unwrap()
                };
                mark.fragments.push(fragment());
                assert!(
                    !(adapter.matches_edit)(&old.to_string(), &modern).unwrap(),
                    "schema {} projected away {direction}.{side} binding",
                    adapter.version
                );
            }
        }
        for fragments in [Value::Null, json!([])] {
            let mut forged = serde_json::to_value(&request).unwrap();
            forged["command"]["fragments"] = fragments;
            assert!((adapter.request)(&forged.to_string()).is_err());
        }
    }
}

#[test]
fn schema12_preserves_partitions_in_documents_subtrees_and_history() {
    let mut old = wire(12);
    old["nodes"]["root"]["kind"]["children"] = json!(["crop"]);
    old["nodes"]["crop"] = json!({"label":"Partition","kind":{"type":"retime","child":"hold","duration":4,"mapping":{"start":2,"end":6},"pitch":"preserve","purpose":"partition"}});
    let document = legacy_v12::Document::from_json(&old.to_string())
        .unwrap()
        .upgrade()
        .unwrap();
    assert!(
        legacy_v12::Document::from_json(&old.to_string())
            .unwrap()
            .matches(&document)
    );
    assert!(matches!(
        &document.nodes()[&node("crop")].kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ));
    let edit = apply(
        &document,
        &request(
            &document,
            Command::Rename {
                node: node("crop"),
                label: "Kept partition".into(),
            },
        ),
    )
    .unwrap();
    let edit_json = serde_json::to_value(&edit).unwrap();
    assert!(legacy_v12::matches_edit(&edit_json.to_string(), &edit).unwrap());
    let insert = request(
        &document,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("crop"),
                nodes: [node("crop"), node("hold")]
                    .map(|id| (id.clone(), document.nodes()[&id].clone()))
                    .into(),
                overrides: Default::default(),
            },
        },
    );
    let insert_json = serde_json::to_value(&insert).unwrap();
    assert_eq!(
        legacy_v12::upgrade_request(&insert_json.to_string()).unwrap(),
        insert
    );
    for purpose in [Value::Null, json!("future_partition")] {
        let mut invalid_document = old.clone();
        invalid_document["nodes"]["crop"]["kind"]["purpose"] = purpose.clone();
        assert!(legacy_v12::Document::from_json(&invalid_document.to_string()).is_err());
        let mut invalid_request = insert_json.clone();
        invalid_request["command"]["subtree"]["nodes"]["crop"]["kind"]["purpose"] = purpose.clone();
        assert!(legacy_v12::upgrade_request(&invalid_request.to_string()).is_err());
        let mut invalid_edit = edit_json.clone();
        invalid_edit["forward"]["nodes"]["crop"]["after"]["kind"]["purpose"] = purpose;
        assert!(legacy_v12::matches_edit(&invalid_edit.to_string(), &edit).is_err());
    }
    let mut unknown_field = old;
    unknown_field["nodes"]["crop"]["kind"]["future_field"] = Value::Null;
    assert!(legacy_v12::Document::from_json(&unknown_field.to_string()).is_err());
}
