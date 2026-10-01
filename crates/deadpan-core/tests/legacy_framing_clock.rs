use deadpan_core::*;
use serde_json::{Value, json};

fn node_id() -> NodeId {
    NodeId::new("hold").unwrap()
}

fn framing() -> Framing {
    Framing::creep(
        FramingPose::identity(),
        FramingPose::new(
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::integer(2),
        )
        .unwrap(),
        FramingCurve::Linear,
    )
    .unwrap()
}

fn retained(prefix: i64) -> Framing {
    framing()
        .prepend_owner_frames(
            FrameDuration::new(prefix).unwrap(),
            FrameDuration::new(8).unwrap(),
        )
        .unwrap()
}

fn document(version: u32) -> Value {
    let empty = ProjectDocument::new(
        ProjectId::new("legacy-framing").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["schema_version"] = json!(version);
    wire["nodes"]["root"]["kind"]["children"] = json!(["hold"]);
    wire["nodes"]["hold"] = json!({
        "label":"Hold",
        "framing":framing(),
        "kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}}
    });
    wire
}

fn request(command: Value) -> Value {
    json!({"project_id":"legacy-framing","expected_revision":"initial","new_revision":"next","command":command})
}

fn reject_clocks(wire: &Value, pointer: &str, reject: impl Fn(&str) -> bool) {
    for clock in [
        Value::Null,
        json!(0),
        json!({"type":"owner_output"}),
        serde_json::to_value(retained(0).clock).unwrap(),
        serde_json::to_value(retained(3).clock).unwrap(),
    ] {
        let mut changed = wire.clone();
        changed.pointer_mut(pointer).unwrap()["clock"] = clock;
        let plain = changed.to_string();
        assert!(reject(&plain), "admitted clock at {pointer}: {plain}");
        let escaped = plain.replace("\"clock\":", "\"cl\\u006fck\":");
        assert_ne!(plain, escaped);
        assert!(reject(&escaped), "admitted escaped clock at {pointer}");
    }
}

macro_rules! legacy_tests {
    ($($name:ident: $version:literal => $legacy:ident),+ $(,)?) => {$(
        #[test]
        fn $name() {
            let wire = document($version);
            let old = $legacy::Document::from_json(&wire.to_string()).unwrap();
            let current = old.clone().upgrade().unwrap();
            assert!(old.matches(&current));
            assert_eq!(current.nodes()[&node_id()].framing.as_ref().unwrap().clock, FramingClock::OwnerOutput);
            assert!(serde_json::to_value(&old).unwrap()["nodes"]["hold"]["framing"].get("clock").is_none());
            reject_clocks(&wire, "/nodes/hold/framing", |json| $legacy::Document::from_json(json).is_err());
            // A zero offset has the same immediate poses, but is still newer
            // intent: later owner growth must keep a different duration clock.
            for prefix in [0, 3] {
                let mut changed = serde_json::to_value(&current).unwrap();
                changed["nodes"]["hold"]["framing"] = serde_json::to_value(retained(prefix)).unwrap();
                let changed = ProjectDocument::from_json(&changed.to_string()).unwrap();
                assert!(!old.matches(&changed), "retained framing projected into schema {}", $version);
            }

            for occurrence in [false, true] {
                let command = if occurrence {
                    json!({"command":"edit_occurrence","instance":{"node":"hold","repeats":[]},"edit":{"type":"set_framing","framing":framing()},"identities":{"nodes":[],"marks":[]}})
                } else {
                    json!({"command":"set_framing","node":"hold","framing":framing()})
                };
                let valid = request(command);
                let upgraded = $legacy::upgrade_request(&valid.to_string()).unwrap();
                let actual = match upgraded.command {
                    Command::SetFraming { framing, .. } | Command::EditOccurrence { edit: OccurrenceEdit::SetFraming { framing }, .. } => framing.unwrap(),
                    _ => panic!("unexpected upgraded command"),
                };
                assert_eq!(actual, framing());
                let pointer = if occurrence { "/command/edit/framing" } else { "/command/framing" };
                reject_clocks(&valid, pointer, |json| $legacy::upgrade_request(json).is_err());
                let mut clear = valid;
                *clear.pointer_mut(pointer).unwrap() = Value::Null;
                $legacy::upgrade_request(&clear.to_string()).expect("legacy clearing remains valid");
            }

            let subtree = json!({"root":"hold","nodes":{"hold":wire["nodes"]["hold"]},"overrides":{}});
            for occurrence in [false, true] {
                let command = if occurrence {
                    json!({"command":"edit_occurrence","instance":{"node":"root","repeats":[]},"edit":{"type":"insert","index":0,"subtree":subtree},"identities":{"nodes":[],"marks":[]}})
                } else {
                    json!({"command":"insert","parent":"root","index":0,"subtree":subtree})
                };
                let valid = request(command);
                $legacy::upgrade_request(&valid.to_string()).expect("ordinary legacy subtree remains valid");
                let pointer = if occurrence { "/command/edit/subtree/nodes/hold/framing" } else { "/command/subtree/nodes/hold/framing" };
                reject_clocks(&valid, pointer, |json| $legacy::upgrade_request(json).is_err());
            }

            let transaction = apply(&current, &CommandRequest {
                project_id: current.project_id().clone(),
                expected_revision: current.revision_id().clone(),
                new_revision: RevisionId::new("renamed").unwrap(),
                command: Command::Rename { node: node_id(), label:"Renamed".into() },
            }).unwrap();
            let edit = serde_json::to_value(&transaction).unwrap();
            assert!($legacy::matches_edit(&edit.to_string(), &transaction).unwrap());
            for direction in ["forward", "inverse"] {
                for side in ["before", "after"] {
                    let pointer = format!("/{direction}/nodes/hold/{side}/framing");
                    reject_clocks(&edit, &pointer, |json| $legacy::matches_edit(json, &transaction).is_err());
                    for prefix in [0, 3] {
                        let mut changed = transaction.clone();
                        let patch = if direction == "forward" { &mut changed.forward } else { &mut changed.inverse };
                        let change = patch.nodes.get_mut(&node_id()).unwrap();
                        let node = if side == "before" { change.before.as_mut() } else { change.after.as_mut() }.unwrap();
                        node.framing = Some(retained(prefix));
                        assert!(!$legacy::matches_edit(&edit.to_string(), &changed).unwrap(), "retained clock projected into {direction} {side}");
                    }
                }
            }
        }
    )+};
}

legacy_tests! {
    closed_v18:18=>legacy_v18, closed_v19:19=>legacy_v19, closed_v20:20=>legacy_v20,
    closed_v21:21=>legacy_v21, closed_v22:22=>legacy_v22, closed_v23:23=>legacy_v23,
    closed_v24:24=>legacy_v24, closed_v25:25=>legacy_v25, closed_v26:26=>legacy_v26,
    closed_v27:27=>legacy_v27, closed_v28:28=>legacy_v28, closed_v29:29=>legacy_v29,
    closed_v30:30=>legacy_v30, closed_v31:31=>legacy_v31, closed_v32:32=>legacy_v32,
}
