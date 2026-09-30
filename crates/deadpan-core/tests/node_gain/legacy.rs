use super::*;

fn old_document(version: u32, document: &ProjectDocument) -> Value {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["schema_version"] = json!(version);
    let object = wire.as_object_mut().unwrap();
    if version < 10 {
        object.remove("basis_state");
    }
    if version < 4 {
        object.remove("overrides");
    }
    if version < 3 {
        object.remove("marks");
    }
    wire
}

fn old_edit(version: u32, transaction: &EditTransaction) -> Value {
    let mut wire = serde_json::to_value(transaction).unwrap();
    for side in ["forward", "inverse"] {
        let object = wire[side].as_object_mut().unwrap();
        if version < 4 {
            object.remove("overrides");
        }
        if version < 3 {
            object.remove("marks");
        }
    }
    wire
}

#[test]
fn every_frozen_document_patch_and_command_rejects_new_vocabulary() {
    let before = fixture();
    let mut treated_wire = serde_json::to_value(&before).unwrap();
    treated_wire["nodes"]["hold"]["audio_treatments"] = serde_json::to_value(unity()).unwrap();
    let treated = ProjectDocument::from_json(&treated_wire.to_string()).unwrap();
    let (_, neutral_edit) = edit(
        &before,
        Command::Rename {
            node: id("hold"),
            label: "Renamed".into(),
        },
    );
    let mut treated_edit = neutral_edit.clone();
    treated_edit
        .forward
        .nodes
        .get_mut(&id("hold"))
        .unwrap()
        .after
        .as_mut()
        .unwrap()
        .audio_treatments = unity();
    treated_edit
        .inverse
        .nodes
        .get_mut(&id("hold"))
        .unwrap()
        .before
        .as_mut()
        .unwrap()
        .audio_treatments = unity();
    let mut new_commands = vec![
        Command::SetAudioTreatments {
            node: id("hold"),
            treatments: unity(),
        },
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("hold"),
                repeats: vec![],
            },
            edit: OccurrenceEdit::SetAudioTreatments {
                treatments: unity(),
            },
            identities: OccurrenceIdentities::default(),
        },
        Command::SpliceSourceAt {
            parent: id("root"),
            target: id("hold"),
            at: frames(1),
            source: SourceNode {
                duration: frames(1),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: None,
                audio_mapping: SourceAudioMapping::FitBeat,
                link: LinkRelation::Independent,
                audio_offset: AudioSample(0),
            },
            id: id("inserted"),
            label: "Inserted slice".into(),
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new("initialx").unwrap(),
                ordinal: 0,
            },
        },
    ];
    let Command::SpliceSourceAt {
        parent,
        source,
        id: inserted,
        label,
        identities,
        timing,
        ..
    } = new_commands[2].clone()
    else {
        panic!()
    };
    new_commands.push(Command::ReplaceSource {
        parent,
        range: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
        source,
        id: inserted,
        label,
        identities,
        timing,
    });
    macro_rules! check {
        ($version:literal, $adapter:ident) => {{
            let wire = old_document($version, &before);
            let old = $adapter::Document::from_json(&wire.to_string()).unwrap();
            let neutral = old.clone().upgrade().unwrap();
            assert!(neutral.nodes().values().all(|node| node.audio_treatments.is_empty()));
            assert!(old.matches(&neutral));
            assert!(!old.matches(&treated), stringify!($adapter));
            for field in [Value::Null, json!({}), serde_json::to_value(AudioTreatments::default()).unwrap(), serde_json::to_value(unity()).unwrap()] {
                let mut forged = wire.clone();
                forged["nodes"]["hold"]["audio_treatments"] = field.clone();
                assert!($adapter::Document::from_json(&forged.to_string()).is_err(), stringify!($adapter));
                let mut transaction = old_edit($version, &neutral_edit);
                transaction["forward"]["nodes"]["hold"]["after"]["audio_treatments"] = field;
                assert!($adapter::matches_edit(&transaction.to_string(), &neutral_edit).is_err(), stringify!($adapter));
            }
            let neutral_wire = old_edit($version, &neutral_edit).to_string();
            assert!($adapter::matches_edit(&neutral_wire, &neutral_edit).unwrap(), stringify!($adapter));
            assert!(!$adapter::matches_edit(&neutral_wire, &treated_edit).unwrap(), stringify!($adapter));
            for command in &new_commands {
                let wire = serde_json::to_string(&request(&before, command.clone())).unwrap();
                assert!($adapter::upgrade_request(&wire).is_err(), stringify!($adapter));
            }
            let mut subtree = json!({"root":"hold", "nodes": {"hold": wire["nodes"]["hold"].clone()}});
            if $version >= 4 { subtree["overrides"] = json!({}); }
            let mut inserted = serde_json::to_value(request(&before, Command::Rename { node: id("hold"), label: "unused".into() })).unwrap();
            inserted["command"] = json!({"command":"insert", "parent":"root", "index":0, "subtree":subtree});
            assert!($adapter::upgrade_request(&inserted.to_string()).is_ok(), stringify!($adapter));
            for field in [Value::Null, serde_json::to_value(AudioTreatments::default()).unwrap()] {
                let mut forged = inserted.clone();
                forged["command"]["subtree"]["nodes"]["hold"]["audio_treatments"] = field;
                assert!($adapter::upgrade_request(&forged.to_string()).is_err(), stringify!($adapter));
            }
        }};
    }
    check!(1, legacy_v1);
    check!(2, legacy_v2);
    check!(3, legacy_v3);
    check!(4, legacy_v4);
    check!(5, legacy_v5);
    check!(6, legacy_v6);
    check!(7, legacy_v7);
    check!(8, legacy_v8);
    check!(9, legacy_v9);
    check!(10, legacy_v10);
    check!(11, legacy_v11);
    check!(12, legacy_v12);
    check!(13, legacy_v13);
    check!(14, legacy_v14);
    check!(15, legacy_v15);
    check!(16, legacy_v16);
    check!(17, legacy_v17);
    check!(18, legacy_v18);
    check!(19, legacy_v19);
    check!(20, legacy_v20);
    check!(21, legacy_v21);
    check!(22, legacy_v22);
    check!(23, legacy_v23);
    check!(24, legacy_v24);
    check!(25, legacy_v25);
    check!(26, legacy_v26);
    check!(27, legacy_v27);
    check!(28, legacy_v28);
    check!(29, legacy_v29);
    check!(30, legacy_v30);
    check!(31, legacy_v31);
    check!(32, legacy_v32);
}

#[test]
fn frozen_sound_context_guards_reject_interior_splices_without_sounds() {
    let before = fixture();
    let request = request(
        &before,
        Command::SpliceSourceAt {
            parent: id("root"),
            target: id("hold"),
            at: frames(1),
            source: SourceNode {
                duration: frames(1),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: None,
                audio_mapping: SourceAudioMapping::FitBeat,
                link: LinkRelation::Independent,
                audio_offset: AudioSample(0),
            },
            id: id("inserted"),
            label: "Inserted slice".into(),
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new("initialx").unwrap(),
                ordinal: 0,
            },
        },
    );

    assert!(before.sounds().is_empty());
    for error in [
        legacy_v29::validate_request_context(&before, &request),
        legacy_v30::validate_request_context(&before, &request),
        legacy_v31::validate_request_context(&before, &request),
        legacy_v32::validate_request_context(&before, &request),
    ] {
        assert_eq!(error.unwrap_err().code, EditErrorCode::InvalidCommand);
    }
}

#[test]
fn schema_32_retains_hold_audio_setters_and_historical_contextual_admission() {
    let document = fixture();
    for command in [
        Command::SetHoldAudio {
            node: id("hold"),
            audio: HoldAudio::Silence,
        },
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("hold"),
                repeats: vec![],
            },
            edit: OccurrenceEdit::SetHoldAudio {
                audio: HoldAudio::Silence,
            },
            identities: OccurrenceIdentities::default(),
        },
    ] {
        let requested = request(&document, command);
        assert_eq!(
            legacy_v32::upgrade_request(&serde_json::to_string(&requested).unwrap()).unwrap(),
            requested
        );
        assert!(legacy_v32::validate_request_context(&document, &requested).is_ok());
        assert!(legacy_v31::upgrade_request(&serde_json::to_string(&requested).unwrap()).is_err());
        assert!(legacy_v31::validate_request_context(&document, &requested).is_err());
    }
    for command in [
        Command::SetAudioTreatments {
            node: id("hold"),
            treatments: unity(),
        },
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("hold"),
                repeats: vec![],
            },
            edit: OccurrenceEdit::SetAudioTreatments {
                treatments: unity(),
            },
            identities: OccurrenceIdentities::default(),
        },
    ] {
        let requested = request(&document, command);
        assert!(legacy_v29::validate_request_context(&document, &requested).is_err());
        assert!(legacy_v30::validate_request_context(&document, &requested).is_err());
        assert!(legacy_v31::validate_request_context(&document, &requested).is_err());
        assert!(legacy_v32::validate_request_context(&document, &requested).is_err());
    }
}
