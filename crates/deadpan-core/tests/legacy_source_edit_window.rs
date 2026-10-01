use deadpan_core::*;
use serde_json::{Value, json};

fn ratio(n: i64) -> Value {
    json!({"numerator":n.to_string(),"denominator":"1"})
}

fn window() -> Value {
    json!({"start":ratio(0),"end":ratio(30)})
}

fn source(version: u32) -> Value {
    let span = json!({"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":30000,"time_base":{"numerator":1,"denominator":30000}}});
    let mut source = json!({"duration":30,"video":{"type":"stream","asset":"video","span":span},"audio":null,"link":"independent","audio_offset":0});
    if version >= 6 {
        source["audio_mapping"] = json!({"type":"fit_beat"});
    }
    if version >= 7 {
        source["video_mapping"] = json!({"type":"fit_beat"});
    }
    source
}

fn document(version: u32) -> Value {
    let mut wire = json!({"schema_version":version,"project_id":"legacy-selection","revision_id":"initial","presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},"root":"root","nodes":{"root":{"label":"Root","kind":{"type":"sequence","children":["source"]}},"source":{"label":"Original","kind":{"type":"source","source":source(version)}}},"assets":{"video":{"label":"Video","content_hash":"a".repeat(64),"video":source(version)["video"]["span"],"audio":null,"still_image":false,"frame_count":30}}});
    if version >= 3 {
        wire["marks"] = json!({});
    }
    if version >= 4 {
        wire["overrides"] = json!({});
    }
    if version >= 10 {
        let empty = ProjectDocument::new(
            ProjectId::new("empty").unwrap(),
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
        wire["basis_state"] = serde_json::to_value(empty).unwrap()["basis_state"].clone();
    }
    wire
}

fn request(command: Value) -> Value {
    json!({"project_id":"legacy-selection","expected_revision":"initial","new_revision":"next","command":command})
}

fn forbidden_window(wire: Value, pointer: &str, reject: impl Fn(&str) -> bool) {
    for value in [Value::Null, window()] {
        let mut forged = wire.clone();
        forged.pointer_mut(pointer).unwrap()["edit_window"] = value;
        let raw = forged.to_string();
        assert!(reject(&raw), "admitted edit_window at {pointer}");
        assert!(
            reject(&raw.replace("\"edit_window\":", "\"edit_wind\\u006fw\":")),
            "admitted escaped edit_window at {pointer}"
        );
    }
}

fn select_window(node: &mut BeatNode) {
    let NodeKind::Source { source } = &mut node.kind else {
        panic!()
    };
    source.edit_window =
        Some(SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(30)).unwrap());
}

macro_rules! legacy_tests {
    ($($name:ident: $v:literal => $legacy:ident),+ $(,)?) => {$(
        #[test]
        fn $name() {
            let wire = document($v);
            let old = $legacy::Document::from_json(&wire.to_string()).unwrap();
            let current = old.clone().upgrade().unwrap();
            assert!(old.matches(&current));
            let NodeKind::Source { source: upgraded } = &current.nodes()[&NodeId::new("source").unwrap()].kind else { panic!() };
            assert_eq!(upgraded.edit_window, None);
            let mut modern = serde_json::to_value(&current).unwrap();
            modern["nodes"]["source"]["kind"]["source"]["edit_window"] = window();
            assert!(!old.matches(&ProjectDocument::from_json(&modern.to_string()).unwrap()));
            forbidden_window(wire.clone(), "/nodes/source/kind/source", |json| $legacy::Document::from_json(json).is_err());

            let mut subtree = json!({"root":"source","nodes":{"source":wire["nodes"]["source"]}});
            if $v >= 4 { subtree["overrides"] = json!({}); }
            let insertion = request(json!({"command":"insert","parent":"root","index":0,"subtree":subtree}));
            $legacy::upgrade_request(&insertion.to_string()).expect("valid legacy insertion grammar");
            forbidden_window(insertion, "/command/subtree/nodes/source/kind/source", |json| $legacy::upgrade_request(json).is_err());
            if $v >= 4 {
                let occurrence = request(json!({"command":"edit_occurrence","instance":{"node":"root","repeats":[]},"edit":{"type":"insert","index":0,"subtree":subtree},"identities":{"nodes":[],"marks":[]}}));
                $legacy::upgrade_request(&occurrence.to_string()).expect("valid legacy occurrence insertion grammar");
                forbidden_window(occurrence, "/command/edit/subtree/nodes/source/kind/source", |json| $legacy::upgrade_request(json).is_err());
            }
            if $v >= 9 {
                let import = request(json!({"command":"import_source","id":"video","asset":wire["assets"]["video"],"insertion":{"parent":"root","index":0,"node":"source","label":"Original","source":source($v)}}));
                $legacy::upgrade_request(&import.to_string()).expect("valid legacy import grammar");
                forbidden_window(import, "/command/insertion/source", |json| $legacy::upgrade_request(json).is_err());
            }
            if $v >= 27 {
                let splice = request(json!({"command":"splice_source","parent":"root","index":0,"source":source($v),"id":"source","label":"Original","timing":{"allocation":"next","ordinal":0}}));
                $legacy::upgrade_request(&splice.to_string()).expect("valid legacy source splice grammar");
                forbidden_window(splice, "/command/source", |json| $legacy::upgrade_request(json).is_err());
            }

            let tx = apply(&current, &CommandRequest {
                project_id: current.project_id().clone(),
                expected_revision: current.revision_id().clone(),
                new_revision: RevisionId::new("renamed").unwrap(),
                command: Command::Rename { node: NodeId::new("source").unwrap(), label: "Renamed".into() },
            }).unwrap();
            let mut retained = serde_json::to_value(&tx).unwrap();
            for direction in ["forward", "inverse"] {
                if $v < 3 { retained[direction].as_object_mut().unwrap().remove("marks"); }
                if $v < 4 { retained[direction].as_object_mut().unwrap().remove("overrides"); }
                for side in ["before", "after"] {
                    retained[direction]["nodes"]["source"][side]["kind"]["source"] = source($v);
                }
            }
            assert!($legacy::matches_edit(&retained.to_string(), &tx).unwrap());
            for direction in ["forward", "inverse"] {
                for side in ["before", "after"] {
                    let path = format!("/{direction}/nodes/source/{side}/kind/source");
                    forbidden_window(retained.clone(), &path, |json| $legacy::matches_edit(json, &tx).is_err());
                    let mut changed = tx.clone();
                    let patch = if direction == "forward" { &mut changed.forward } else { &mut changed.inverse };
                    let change = patch.nodes.get_mut(&NodeId::new("source").unwrap()).unwrap();
                    let node = if side == "before" { change.before.as_mut() } else { change.after.as_mut() }.unwrap();
                    select_window(node);
                    assert!(!$legacy::matches_edit(&retained.to_string(), &changed).unwrap(), "current window projected into {direction} {side}");
                }
            }
        }
    )+};
}
legacy_tests! {
    strict_v1:1=>legacy_v1, strict_v2:2=>legacy_v2, strict_v3:3=>legacy_v3,
    strict_v4:4=>legacy_v4, strict_v5:5=>legacy_v5, strict_v6:6=>legacy_v6,
    strict_v7:7=>legacy_v7, strict_v8:8=>legacy_v8, strict_v9:9=>legacy_v9,
    strict_v10:10=>legacy_v10, strict_v11:11=>legacy_v11, strict_v12:12=>legacy_v12,
    strict_v13:13=>legacy_v13, strict_v14:14=>legacy_v14, strict_v15:15=>legacy_v15,
    strict_v16:16=>legacy_v16, strict_v17:17=>legacy_v17, strict_v18:18=>legacy_v18,
    strict_v19:19=>legacy_v19, strict_v20:20=>legacy_v20, strict_v21:21=>legacy_v21,
    strict_v22:22=>legacy_v22, strict_v23:23=>legacy_v23, strict_v24:24=>legacy_v24,
    strict_v25:25=>legacy_v25, strict_v26:26=>legacy_v26, strict_v27:27=>legacy_v27,
    strict_v28:28=>legacy_v28, strict_v29:29=>legacy_v29, strict_v30:30=>legacy_v30,
    strict_v31:31=>legacy_v31, strict_v32:32=>legacy_v32,
}
