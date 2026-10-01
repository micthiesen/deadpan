use deadpan_core::*;
use serde_json::{Value, json};

fn document(version: u32) -> Value {
    let current = ProjectDocument::new(
        ProjectId::new("editorial").unwrap(),
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
    let mut wire = serde_json::to_value(current).unwrap();
    wire["schema_version"] = json!(version);
    if version < 3 {
        wire.as_object_mut().unwrap().remove("marks");
    }
    if version < 4 {
        wire.as_object_mut().unwrap().remove("overrides");
    }
    if version < 10 {
        wire.as_object_mut().unwrap().remove("basis_state");
    }
    wire
}

fn forbidden(wire: &Value, pointer: &str, reject: impl Fn(&str) -> bool) {
    for value in [
        Value::Null,
        json!({"start":false,"end":false}),
        json!({"start":true,"end":false}),
    ] {
        let mut changed = wire.clone();
        changed.pointer_mut(pointer).unwrap()["audio_editorial_edges"] = value;
        let raw = changed.to_string();
        assert!(reject(&raw), "admitted editorial field at {pointer}");
        assert!(reject(&raw.replace(
            "audio_editorial_edges",
            "audio_editorial_\\u0065dges"
        )));
    }
}

macro_rules! historical {
    ($($name:ident: $version:literal => $legacy:ident),+ $(,)?) => {$(
        #[test]
        fn $name() {
            let wire = document($version);
            let old = $legacy::Document::from_json(&wire.to_string()).unwrap();
            let current = old.clone().upgrade().unwrap();
            assert!(old.matches(&current));
            forbidden(&wire, "/nodes/root", |json| $legacy::Document::from_json(json).is_err());
            let mut marked = serde_json::to_value(&current).unwrap();
            marked["nodes"]["root"]["audio_editorial_edges"] = json!({"start":true,"end":false});
            assert!(!old.matches(&ProjectDocument::from_json(&marked.to_string()).unwrap()));

            let mut subtree = json!({"root":"root","nodes":wire["nodes"]});
            if $version >= 4 { subtree["overrides"] = json!({}); }
            let insertion = json!({"project_id":"editorial","expected_revision":"initial","new_revision":"next","command":{"command":"insert","parent":"root","index":0,"subtree":subtree}});
            $legacy::upgrade_request(&insertion.to_string()).expect("baseline legacy insertion grammar");
            forbidden(&insertion, "/command/subtree/nodes/root", |json| $legacy::upgrade_request(json).is_err());
            if $version >= 4 {
                let occurrence = json!({"project_id":"editorial","expected_revision":"initial","new_revision":"next","command":{"command":"edit_occurrence","instance":{"node":"root","repeats":[]},"edit":{"type":"insert","index":0,"subtree":subtree},"identities":{"nodes":[],"marks":[]}}});
                $legacy::upgrade_request(&occurrence.to_string()).expect("baseline occurrence grammar");
                forbidden(&occurrence, "/command/edit/subtree/nodes/root", |json| $legacy::upgrade_request(json).is_err());
            }
            let transaction = apply(&current, &CommandRequest {
                project_id: current.project_id().clone(), expected_revision: current.revision_id().clone(), new_revision: RevisionId::new("next").unwrap(),
                command: Command::Rename { node: NodeId::new("root").unwrap(), label: "Renamed".into() },
            }).unwrap();
            let mut stored = serde_json::to_value(&transaction).unwrap();
            for direction in ["forward", "inverse"] {
                if $version < 3 { stored[direction].as_object_mut().unwrap().remove("marks"); }
                if $version < 4 { stored[direction].as_object_mut().unwrap().remove("overrides"); }
            }
            assert!($legacy::matches_edit(&stored.to_string(), &transaction).unwrap());
            for direction in ["forward", "inverse"] {
                for side in ["before", "after"] {
                    forbidden(&stored, &format!("/{direction}/nodes/root/{side}"), |json| $legacy::matches_edit(json, &transaction).is_err());
                    let mut modern = transaction.clone();
                    let patch = if direction == "forward" { &mut modern.forward } else { &mut modern.inverse };
                    let change = patch.nodes.get_mut(&NodeId::new("root").unwrap()).unwrap();
                    let node = if side == "before" { change.before.as_mut() } else { change.after.as_mut() }.unwrap();
                    node.audio_editorial_edges.start = true;
                    assert!(!$legacy::matches_edit(&stored.to_string(), &modern).unwrap());
                }
            }
        }
    )+};
}
historical! {
    closed_v1:1=>legacy_v1, closed_v2:2=>legacy_v2, closed_v3:3=>legacy_v3,
    closed_v4:4=>legacy_v4, closed_v5:5=>legacy_v5, closed_v6:6=>legacy_v6,
    closed_v7:7=>legacy_v7, closed_v8:8=>legacy_v8, closed_v9:9=>legacy_v9,
    closed_v10:10=>legacy_v10, closed_v11:11=>legacy_v11, closed_v12:12=>legacy_v12,
    closed_v13:13=>legacy_v13, closed_v14:14=>legacy_v14, closed_v15:15=>legacy_v15,
    closed_v16:16=>legacy_v16, closed_v17:17=>legacy_v17, closed_v18:18=>legacy_v18,
    closed_v19:19=>legacy_v19, closed_v20:20=>legacy_v20, closed_v21:21=>legacy_v21,
    closed_v22:22=>legacy_v22, closed_v23:23=>legacy_v23, closed_v24:24=>legacy_v24,
    closed_v25:25=>legacy_v25, closed_v26:26=>legacy_v26, closed_v27:27=>legacy_v27,
    closed_v28:28=>legacy_v28, closed_v29:29=>legacy_v29, closed_v30:30=>legacy_v30,
    closed_v31:31=>legacy_v31, closed_v32:32=>legacy_v32,
}
