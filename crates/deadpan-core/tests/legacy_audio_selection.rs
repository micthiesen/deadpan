use deadpan_core::*;
use serde_json::{Value, json};

fn ratio(n: i64) -> Value {
    json!({"numerator":n.to_string(),"denominator":"1"})
}
fn selected() -> Value {
    json!({"type":"selected_placement", "start":ratio(-2), "frames":ratio(30), "selection":{"start":ratio(2),"end":ratio(8)}})
}
fn source(version: u32) -> Value {
    let span = json!({"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":48000,"time_base":{"numerator":1,"denominator":48000}}});
    let mut source = json!({"duration":30,"video":{"type":"blank"},"audio":{"asset":"audio","span":span},"link":"independent","audio_offset":-73});
    if version >= 6 {
        source["audio_mapping"] = json!({"type":"fit_beat"});
    }
    if version >= 7 {
        source["video_mapping"] = json!({"type":"fit_beat"});
    }
    source
}
fn document(version: u32) -> Value {
    let mut wire = json!({"schema_version":version,"project_id":"legacy-selection","revision_id":"initial","presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},"root":"root","nodes":{"root":{"label":"Root","kind":{"type":"sequence","children":["source"]}},"source":{"label":"Original","kind":{"type":"source","source":source(version)}}},"assets":{"audio":{"label":"Audio","content_hash":"a".repeat(64),"video":null,"audio":source(version)["audio"]["span"],"still_image":false,"frame_count":null}}});
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
fn forbidden_mapping(mut wire: Value, pointer: &str, reject: impl Fn(&str) -> bool) {
    let old = wire.pointer(pointer).unwrap().clone();
    *wire.pointer_mut(pointer).unwrap() = selected();
    assert!(
        reject(&wire.to_string()),
        "admitted selected mapping at {pointer}"
    );
    assert!(
        reject(
            &wire
                .to_string()
                .replace("selected_placement", "selected_\\u0070lacement")
        ),
        "admitted escaped variant"
    );
    for field in ["selection", "future"] {
        for value in [Value::Null, json!({"start":ratio(2),"end":ratio(8)})] {
            *wire.pointer_mut(pointer).unwrap() = old.clone();
            wire.pointer_mut(pointer).unwrap()[field] = value;
            let json = wire.to_string();
            assert!(reject(&json), "admitted mapping field {field} at {pointer}");
            assert!(
                reject(&json.replace("\"selection\":", "\"selec\\u0074ion\":")),
                "admitted escaped selection field"
            );
        }
    }
}
macro_rules! legacy_tests {
    ($($name:ident: $v:literal => $legacy:ident),+ $(,)?) => {$(
        #[test]
        fn $name() {
            let wire=document($v);
            let old=$legacy::Document::from_json(&wire.to_string()).unwrap();
            let current=old.clone().upgrade().unwrap();
            assert!(old.matches(&current));
            let mut modern=serde_json::to_value(&current).unwrap();
            modern["nodes"]["source"]["kind"]["source"]["audio_mapping"]=selected();
            assert!(!old.matches(&ProjectDocument::from_json(&modern.to_string()).unwrap()));
            let mut forged=wire.clone();
            // Versions before 6 have no audio_mapping key at all.
            forged["nodes"]["source"]["kind"]["source"]["audio_mapping"]=json!({"type":"fit_beat"});
            forbidden_mapping(forged,"/nodes/source/kind/source/audio_mapping",|json|$legacy::Document::from_json(json).is_err());
            let mut subtree=json!({"root":"source","nodes":{"source":wire["nodes"]["source"]}});
            // Schemas 4 through 6 require this field. Later schemas default it;
            // schemas 1 through 3 reject it because overrides did not exist.
            if $v >= 4 { subtree["overrides"]=json!({}); }
            let insertion=request(json!({"command":"insert","parent":"root","index":0,"subtree":subtree}));
            $legacy::upgrade_request(&insertion.to_string()).expect("valid legacy insertion grammar");
            let mut forged=insertion;
            forged["command"]["subtree"]["nodes"]["source"]["kind"]["source"]["audio_mapping"]=json!({"type":"fit_beat"});
            forbidden_mapping(forged,"/command/subtree/nodes/source/kind/source/audio_mapping",|json|$legacy::upgrade_request(json).is_err());
            if $v >= 4 {
                let mut occurrence=request(json!({"command":"edit_occurrence","instance":{"node":"root","repeats":[]},"edit":{"type":"insert","index":0,"subtree":subtree},"identities":{"nodes":[],"marks":[]}}));
                $legacy::upgrade_request(&occurrence.to_string()).expect("valid legacy occurrence insertion grammar");
                occurrence["command"]["edit"]["subtree"]["nodes"]["source"]["kind"]["source"]["audio_mapping"]=json!({"type":"fit_beat"});
                forbidden_mapping(occurrence,"/command/edit/subtree/nodes/source/kind/source/audio_mapping",|json|$legacy::upgrade_request(json).is_err());
            }
            for occurrence in [false,true] {
                let command=if occurrence {
                    json!({"command":"edit_occurrence","instance":{"node":"source","repeats":[]},"edit":{"type":"set_source_audio_mapping","mapping":{"type":"fit_beat"},"offset":0},"identities":{"nodes":[],"marks":[]}})
                } else {json!({"command":"set_source_audio_mapping","node":"source","mapping":{"type":"fit_beat"},"offset":0})};
                let wire=request(command);
                assert_eq!($legacy::upgrade_request(&wire.to_string()).is_ok(),$v>=6);
                forbidden_mapping(wire,if occurrence{"/command/edit/mapping"}else{"/command/mapping"},|json|$legacy::upgrade_request(json).is_err());
            }
            let tx=apply(&current,&CommandRequest{project_id:current.project_id().clone(),expected_revision:current.revision_id().clone(),new_revision:RevisionId::new("renamed").unwrap(),command:Command::Rename{node:NodeId::new("source").unwrap(),label:"Renamed".into()}}).unwrap();
            let mut edit=serde_json::to_value(&tx).unwrap();
            for direction in ["forward","inverse"] {
                if $v<3 {edit[direction].as_object_mut().unwrap().remove("marks");}
                if $v<4 {edit[direction].as_object_mut().unwrap().remove("overrides");}
                for side in ["before","after"] {
                    edit[direction]["nodes"]["source"][side]["kind"]["source"]=source($v);
                }
            }
            assert!($legacy::matches_edit(&edit.to_string(),&tx).unwrap());
            for direction in ["forward","inverse"] {
                for side in ["before","after"] {
                    let path=format!("/{direction}/nodes/source/{side}/kind/source/audio_mapping");
                    let mut forged=edit.clone();
                    forged[direction]["nodes"]["source"][side]["kind"]["source"]["audio_mapping"]=json!({"type":"fit_beat"});
                    forbidden_mapping(forged,&path,|json|$legacy::matches_edit(json,&tx).is_err());
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
    strict_v19:19=>legacy_v19,
}

#[test]
fn schema19_retains_captured_hold_context_setters_in_both_command_scopes() {
    let context = CapturedFraming::new(vec![CapturedCanvas {
        width: 16,
        height: 16,
        fit: CapturedFit::Fill,
        layers: vec![None],
    }])
    .unwrap();
    for context in [None, Some(context)] {
        for command in [
            Command::SetHoldPictureContext {
                node: NodeId::new("hold").unwrap(),
                context: context.clone(),
            },
            Command::EditOccurrence {
                instance: InstancePath {
                    node: NodeId::new("hold").unwrap(),
                    repeats: vec![],
                },
                edit: OccurrenceEdit::SetHoldPictureContext {
                    context: context.clone(),
                },
                identities: OccurrenceIdentities::default(),
            },
        ] {
            let wire = request(serde_json::to_value(command).unwrap()).to_string();
            assert_eq!(
                legacy_v19::upgrade_request(&wire).unwrap(),
                serde_json::from_str::<CommandRequest>(&wire).unwrap()
            );
            assert!(legacy_v18::upgrade_request(&wire).is_err());
        }
    }
}

#[test]
fn schema19_snapshot_preflight_bounds_captured_context_before_full_deserialization() {
    let context = CapturedFraming::new(vec![CapturedCanvas {
        width: 16,
        height: 16,
        fit: CapturedFit::Fit,
        layers: vec![None; MAX_CAPTURED_SCOPES],
    }])
    .unwrap();
    let count = MAX_CAPTURED_FRAMING_RECORDS / context.record_count().unwrap();
    let mut wire = document(19);
    for index in 0..count {
        let id = format!("captured-{index}");
        wire["nodes"][&id] = json!({"label":"Retained view","kind":{"type":"hold","recipe":{"duration":1,"video":{"type":"background"},"audio":{"type":"silence"},"picture_context":context}}});
        wire["nodes"]["root"]["kind"]["children"]
            .as_array_mut()
            .unwrap()
            .push(json!(id));
    }
    assert!(legacy_v19::Document::from_json(&wire.to_string()).is_ok());
    wire["nodes"]["over-limit"] = wire["nodes"]["captured-0"].clone();
    wire["nodes"]["root"]["kind"]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!("over-limit"));
    // A malformed outer field proves the bounded scanner runs before the whole
    // legacy tree is deserialized and cloned for upgrade validation.
    wire["project_id"] = Value::Null;
    for encoded in [
        wire.to_string(),
        wire.to_string()
            .replace("picture_context", "picture_con\\u0074ext"),
    ] {
        assert!(
            legacy_v19::Document::from_json(&encoded)
                .unwrap_err()
                .to_string()
                .contains("aggregate captured framing record limit")
        );
    }
}
