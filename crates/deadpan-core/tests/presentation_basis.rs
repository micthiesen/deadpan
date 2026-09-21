use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::{Value, json};

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn asset_id() -> AssetId {
    AssetId::new("original").unwrap()
}
fn automatic() -> ProjectDocument {
    ProjectDocument::new_automatic(
        ProjectId::new("basis").unwrap(),
        RevisionId::new("r0").unwrap(),
        node("root"),
    )
    .unwrap()
}
fn candidate() -> PresentationBasis {
    PresentationBasis {
        width: 1080,
        height: 1920,
        frame_rate: FrameRate::new(30000, 1001).unwrap(),
        color_policy: ColorPolicy::SdrRec709,
    }
}
fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: -48000,
            time_base,
        },
        SourceTimestamp {
            ticks: 48000,
            time_base,
        },
    )
    .unwrap()
}
fn asset() -> AssetRecord {
    AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(span()),
        audio: Some(span()),
        still_image: false,
        frame_count: Some(FrameDuration::new(60).unwrap()),
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    }
}
fn insertion(name: &str, index: usize, audio_only: bool) -> SourceInsertion {
    SourceInsertion {
        parent: node("root"),
        index,
        node: node(name),
        label: "Original".into(),
        source: SourceNode {
            duration: FrameDuration::new(60).unwrap(),
            video: if audio_only {
                SourceVideo::Blank
            } else {
                SourceVideo::Stream {
                    asset: asset_id(),
                    span: span(),
                }
            },
            audio: Some(SourceAudio {
                asset: asset_id(),
                span: span(),
            }),
            link: if audio_only {
                LinkRelation::Independent
            } else {
                LinkRelation::Linked
            },
            audio_offset: AudioSample(-137),
            audio_mapping: SourceAudioMapping::natural_rate(span(), FrameRate::new(30, 1).unwrap())
                .unwrap(),
            video_mapping: SourceVideoMapping::FitBeat,
        },
    }
}
fn import(insertion: Option<SourceInsertion>, primary: Option<PrimarySourceImport>) -> Command {
    Command::ImportSource {
        id: asset_id(),
        asset: asset(),
        insertion: insertion.map(Box::new),
        primary,
    }
}
fn request(before: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new(format!("{}x", before.revision_id())).unwrap(),
        command,
    }
}
fn edit(before: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let request = request(before, command);
    assert_eq!(
        serde_json::from_value::<CommandRequest>(serde_json::to_value(&request).unwrap()).unwrap(),
        request
    );
    let transaction = apply(before, &request).unwrap();
    let after = transaction.forward.apply(before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    assert_eq!(
        serde_json::from_value::<EditTransaction>(serde_json::to_value(&transaction).unwrap())
            .unwrap(),
        transaction
    );
    (after, transaction)
}
fn hold() -> Command {
    Command::Insert {
        parent: node("root"),
        index: 0,
        subtree: Subtree {
            root: node("hold"),
            nodes: BTreeMap::from([(
                node("hold"),
                BeatNode::hold(
                    "Pause",
                    HoldRecipe {
                        duration: FrameDuration::new(5).unwrap(),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                    },
                ),
            )]),
            overrides: BTreeMap::new(),
        },
    }
}
fn mark(coordinate: Anchor) -> Command {
    Command::SetMark {
        id: MarkId::new("mark").unwrap(),
        owner: node("root"),
        label: "Boundary".into(),
        boundary: BoundaryAnchor {
            coordinate,
            bias: InsertionBias::Right,
        },
        loss_policy: AnchorLossPolicy::KeepUnresolved,
    }
}

#[test]
fn automatic_registration_and_source_marks_remain_eligible_for_atomic_primary_adoption() {
    let before = automatic();
    assert_eq!(before.basis_state(), &BasisState::provisional());
    assert_eq!(
        before.presentation_basis().frame_rate,
        FrameRate::new(30, 1).unwrap()
    );
    let (registered, registration) = edit(&before, import(None, None));
    assert!(registration.forward.presentation.is_none());
    let (marked, _) = edit(
        &registered,
        mark(Anchor::Source {
            asset: asset_id(),
            moment: SourceMoment::Timestamp {
                stream: SourceStream::Video,
                timestamp: span().start(),
            },
        }),
    );
    assert_eq!(marked.basis_state(), &BasisState::provisional());
    let (after, transaction) = edit(
        &marked,
        import(
            Some(insertion("picture", 0, false)),
            Some(PrimarySourceImport::Adopt { basis: candidate() }),
        ),
    );
    assert_eq!(after.presentation_basis(), &candidate());
    assert_eq!(
        after.basis_state().rate_origin,
        FrameRateOrigin::PrimarySource
    );
    assert_eq!(
        after.basis_state().geometry_origin,
        GeometryOrigin::PrimarySource
    );
    assert_eq!(
        after.basis_state().primary,
        Some(PrimarySource {
            asset: asset_id(),
            qualification: asset().source_qualification.unwrap()
        })
    );
    assert_eq!(
        transaction
            .forward
            .presentation
            .as_ref()
            .unwrap()
            .before
            .state,
        BasisState::provisional()
    );
    assert_eq!(transaction.duration_delta, 60);
    assert_eq!(after.marks(), marked.marks());
    assert!(transaction.forward.assets.is_empty());
}

#[test]
fn labels_asset_registration_empty_structure_and_noops_do_not_lock_rate() {
    let before = automatic();
    let (renamed, _) = edit(
        &before,
        Command::Rename {
            node: node("root"),
            label: "A sequence".into(),
        },
    );
    let (same, transaction) = edit(
        &renamed,
        Command::Rename {
            node: node("root"),
            label: "A sequence".into(),
        },
    );
    assert!(transaction.forward.nodes.is_empty());
    let (registered, _) = edit(
        &same,
        Command::AddAsset {
            id: asset_id(),
            asset: asset(),
        },
    );
    let (registered, repeated) = edit(&registered, import(None, None));
    assert!(repeated.forward.assets.is_empty());
    assert!(repeated.forward.presentation.is_none());
    let (with_empty, _) = edit(
        &registered,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("empty"),
                nodes: BTreeMap::from([(node("empty"), BeatNode::sequence("Empty", vec![]))]),
                overrides: BTreeMap::new(),
            },
        },
    );
    let (deleted, _) = edit(
        &with_empty,
        Command::Delete {
            node: node("empty"),
        },
    );
    assert_eq!(deleted.basis_state(), &BasisState::provisional());
}

#[test]
fn audio_secondary_picture_and_hold_insertions_lock_the_rate_and_delete_all_cannot_unlock() {
    for command in [
        hold(),
        import(Some(insertion("picture", 0, false)), None),
        import(Some(insertion("audio", 0, true)), None),
    ] {
        let (after, transaction) = edit(&automatic(), command);
        assert_eq!(after.basis_state().rate_origin, FrameRateOrigin::TimedEdit);
        assert_eq!(after.presentation_basis(), automatic().presentation_basis());
        assert!(transaction.forward.presentation.is_some());
        let child = after.children(after.root()).next().unwrap().clone();
        let (empty, _) = edit(&after, Command::Delete { node: child });
        assert_eq!(empty.duration().unwrap(), FrameDuration::ZERO);
        assert_eq!(empty.basis_state().rate_origin, FrameRateOrigin::TimedEdit);
        assert!(
            apply(
                &empty,
                &request(
                    &empty,
                    import(
                        Some(insertion("primary", 0, false)),
                        Some(PrimarySourceImport::Adopt { basis: candidate() })
                    )
                )
            )
            .is_err()
        );
    }
}

#[test]
fn all_project_clock_mark_spaces_lock_even_at_zero() {
    for coordinate in [
        Anchor::Sequence {
            frame: ProjectFrame(0),
        },
        Anchor::Local {
            node: node("root"),
            position: ExactRatio::ZERO,
        },
        Anchor::Occurrence {
            instance: InstancePath {
                node: node("root"),
                repeats: vec![],
            },
            position: ExactRatio::ZERO,
        },
    ] {
        let (after, _) = edit(&automatic(), mark(coordinate));
        assert_eq!(after.basis_state().rate_origin, FrameRateOrigin::TimedEdit);
        let (deleted, _) = edit(
            &after,
            Command::DeleteMark {
                id: MarkId::new("mark").unwrap(),
            },
        );
        assert_eq!(
            deleted.basis_state().rate_origin,
            FrameRateOrigin::TimedEdit
        );
    }
}

#[test]
fn locked_primary_recording_and_explicit_geometry_changes_preserve_all_time_and_media() {
    let (timed, _) = edit(&automatic(), hold());
    let (primary, transaction) = edit(
        &timed,
        import(
            Some(insertion("picture", 1, false)),
            Some(PrimarySourceImport::KeepBasis),
        ),
    );
    assert_eq!(primary.presentation_basis(), timed.presentation_basis());
    assert_eq!(
        primary.basis_state().rate_origin,
        FrameRateOrigin::TimedEdit
    );
    assert_eq!(
        primary.basis_state().geometry_origin,
        GeometryOrigin::Default
    );
    assert!(transaction.forward.presentation.is_some());
    let (marked, _) = edit(
        &primary,
        mark(Anchor::Sequence {
            frame: ProjectFrame(3),
        }),
    );
    for command in [
        Command::SetCanvas {
            width: 2048,
            height: 858,
        },
        Command::AdoptPrimaryGeometry {
            width: 1080,
            height: 1920,
        },
    ] {
        let (after, transaction) = edit(&marked, command.clone());
        assert_eq!(
            after.presentation_basis().frame_rate,
            marked.presentation_basis().frame_rate
        );
        assert_eq!(
            after.presentation_basis().color_policy,
            marked.presentation_basis().color_policy
        );
        assert_eq!(after.nodes(), marked.nodes());
        assert_eq!(after.assets(), marked.assets());
        assert_eq!(after.marks(), marked.marks());
        assert_eq!(after.duration(), marked.duration());
        assert_eq!(transaction.duration_delta, 0);
        assert!(transaction.changed_ids.is_empty());
        assert_eq!(
            after.basis_state().geometry_origin,
            if matches!(command, Command::SetCanvas { .. }) {
                GeometryOrigin::Explicit
            } else {
                GeometryOrigin::PrimarySource
            }
        );
    }
    let (secondary, _) = edit(
        &marked,
        import(Some(insertion("secondary", 2, false)), None),
    );
    assert_eq!(secondary.basis_state(), marked.basis_state());
    assert_eq!(secondary.presentation_basis(), marked.presentation_basis());
    assert!(
        apply(
            &secondary,
            &request(
                &secondary,
                import(
                    Some(insertion("other", 3, false)),
                    Some(PrimarySourceImport::KeepBasis)
                )
            )
        )
        .is_err()
    );
}

#[test]
fn canvas_choice_locks_provisional_rate_and_requires_even_bounded_geometry() {
    let before = automatic();
    let (after, _) = edit(
        &before,
        Command::SetCanvas {
            width: 1920,
            height: 1080,
        },
    );
    assert_eq!(after.basis_state(), &BasisState::explicit());
    let explicit = ProjectDocument::new(
        ProjectId::new("basis").unwrap(),
        RevisionId::new("r0").unwrap(),
        candidate(),
        node("root"),
    )
    .unwrap();
    assert_eq!(explicit.basis_state(), &BasisState::explicit());
    for (width, height) in [(0, 10), (10, 0), (1, 10), (10, 3), (65538, 2), (2, 65538)] {
        assert!(
            apply(
                &before,
                &request(&before, Command::SetCanvas { width, height })
            )
            .is_err()
        );
    }
    assert!(
        apply(
            &before,
            &request(
                &before,
                Command::AdoptPrimaryGeometry {
                    width: 1080,
                    height: 1920
                }
            )
        )
        .is_err()
    );
}

#[test]
fn primary_intent_requires_first_qualified_picture_insertion_and_compatible_policy() {
    let before = automatic();
    for command in [
        import(
            None,
            Some(PrimarySourceImport::Adopt { basis: candidate() }),
        ),
        import(
            Some(insertion("audio", 0, true)),
            Some(PrimarySourceImport::Adopt { basis: candidate() }),
        ),
        import(
            Some(insertion("picture", 0, false)),
            Some(PrimarySourceImport::KeepBasis),
        ),
    ] {
        assert!(apply(&before, &request(&before, command)).is_err());
    }
    let mut unqualified = import(
        Some(insertion("picture", 0, false)),
        Some(PrimarySourceImport::Adopt { basis: candidate() }),
    );
    if let Command::ImportSource { asset, .. } = &mut unqualified {
        asset.source_qualification = None;
    }
    assert!(apply(&before, &request(&before, unqualified)).is_err());
    let mut wrong_asset = insertion("picture", 0, false);
    wrong_asset.source.video = SourceVideo::Stream {
        asset: AssetId::new("other").unwrap(),
        span: span(),
    };
    assert!(
        apply(
            &before,
            &request(
                &before,
                import(
                    Some(wrong_asset),
                    Some(PrimarySourceImport::Adopt { basis: candidate() })
                )
            )
        )
        .is_err()
    );
}

#[test]
fn presentation_patch_guards_the_complete_pair_and_cannot_omit_timed_lock() {
    let before = automatic();
    let (_, transaction) = edit(&before, hold());
    let mut omitted = transaction.forward.clone();
    omitted.presentation = None;
    assert!(omitted.apply(&before).is_err());
    for mutate_basis in [false, true] {
        let mut forged = transaction.forward.clone();
        let change = forged.presentation.as_mut().unwrap();
        if mutate_basis {
            change.before.basis.width = 1280;
        } else {
            change.before.state.rate_origin = FrameRateOrigin::Explicit;
        }
        assert_eq!(
            forged.apply(&before).unwrap_err().code,
            EditErrorCode::PatchConflict
        );
    }
    let (canvas, _) = edit(
        &before,
        Command::SetCanvas {
            width: 640,
            height: 480,
        },
    );
    let mut forged = transaction.forward.rebased(
        canvas.revision_id().clone(),
        RevisionId::new("next").unwrap(),
    );
    assert_eq!(
        forged.apply(&canvas).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
    forged.presentation.as_mut().unwrap().before = PresentationState {
        basis: canvas.presentation_basis().clone(),
        state: canvas.basis_state().clone(),
    };
    assert!(forged.apply(&canvas).is_ok());
}

#[test]
fn malformed_serialized_policy_cannot_smuggle_authored_time_or_unqualified_primary() {
    let provisional = serde_json::to_value(automatic()).unwrap();
    for (path, value) in [
        ("rate_origin", json!("unknown")),
        ("geometry_origin", json!("explicit")),
        (
            "primary",
            json!({"asset":"missing", "qualification":"b".repeat(64)}),
        ),
        ("surprise", Value::Null),
    ] {
        let mut forged = provisional.clone();
        forged["basis_state"][path] = value;
        assert!(ProjectDocument::from_json(&forged.to_string()).is_err());
    }
    let mut forged = provisional.clone();
    forged["presentation_basis"]["frame_rate"]["numerator"] = json!(25);
    assert!(ProjectDocument::from_json(&forged.to_string()).is_err());
    forged = provisional.clone();
    forged.as_object_mut().unwrap().remove("basis_state");
    assert!(ProjectDocument::from_json(&forged.to_string()).is_err());
    for command in [
        hold(),
        mark(Anchor::Sequence {
            frame: ProjectFrame(0),
        }),
    ] {
        let (timed, _) = edit(&automatic(), command);
        let mut forged = serde_json::to_value(timed).unwrap();
        forged["basis_state"] = provisional["basis_state"].clone();
        assert!(ProjectDocument::from_json(&forged.to_string()).is_err());
    }
    let (primary, _) = edit(
        &automatic(),
        import(
            Some(insertion("picture", 0, false)),
            Some(PrimarySourceImport::Adopt { basis: candidate() }),
        ),
    );
    for mutate in [0, 1, 2] {
        let mut forged = serde_json::to_value(&primary).unwrap();
        match mutate {
            0 => forged["basis_state"]["primary"] = Value::Null,
            1 => forged["basis_state"]["primary"]["qualification"] = json!("c".repeat(64)),
            _ => forged["assets"]["original"]["source_qualification"] = Value::Null,
        }
        assert!(ProjectDocument::from_json(&forged.to_string()).is_err());
    }
}

#[test]
fn default_geometry_and_timed_rate_origins_require_their_actual_default_values() {
    let (timed, _) = edit(&automatic(), hold());
    for (field, value) in [("width", 1280), ("height", 720)] {
        let mut forged = serde_json::to_value(&timed).unwrap();
        forged["presentation_basis"][field] = json!(value);
        assert!(
            ProjectDocument::from_json(&forged.to_string()).is_err(),
            "default geometry cannot claim a changed {field}"
        );
    }
    for (field, value) in [("numerator", 25), ("denominator", 1001)] {
        let mut forged = serde_json::to_value(&timed).unwrap();
        forged["presentation_basis"]["frame_rate"][field] = json!(value);
        assert!(ProjectDocument::from_json(&forged.to_string()).is_err());
    }
    let mut explicit = serde_json::to_value(&timed).unwrap();
    explicit["basis_state"]["rate_origin"] = json!("explicit");
    assert!(ProjectDocument::from_json(&explicit.to_string()).is_err());
    let (primary, _) = edit(
        &automatic(),
        import(
            Some(insertion("picture", 0, false)),
            Some(PrimarySourceImport::Adopt { basis: candidate() }),
        ),
    );
    let mut forged = serde_json::to_value(primary).unwrap();
    forged["basis_state"]["geometry_origin"] = json!("default");
    forged["presentation_basis"]["width"] = json!(1920);
    forged["presentation_basis"]["height"] = json!(1080);
    assert!(ProjectDocument::from_json(&forged.to_string()).is_err());
    // Explicit geometry can have any valid canvas while retaining the locked
    // default rate. These are distinct decisions and must remain independent.
    let (canvas, _) = edit(
        &timed,
        Command::SetCanvas {
            width: 1280,
            height: 720,
        },
    );
    assert_eq!(canvas.basis_state().rate_origin, FrameRateOrigin::TimedEdit);
    assert_eq!(
        ProjectDocument::from_json(&canvas.to_json().unwrap()).unwrap(),
        canvas
    );
}

type DocumentAdapter = fn(&str) -> Result<ProjectDocument, DocumentError>;
type RequestAdapter = fn(&str) -> Result<CommandRequest, DocumentError>;
type EditAdapter = fn(&str, &EditTransaction) -> Result<bool, DocumentError>;
const ADAPTERS: [(DocumentAdapter, RequestAdapter, EditAdapter); 9] = [
    (
        |wire| legacy_v1::Document::from_json(wire)?.upgrade(),
        legacy_v1::upgrade_request,
        legacy_v1::matches_edit,
    ),
    (
        |wire| legacy_v2::Document::from_json(wire)?.upgrade(),
        legacy_v2::upgrade_request,
        legacy_v2::matches_edit,
    ),
    (
        |wire| legacy_v3::Document::from_json(wire)?.upgrade(),
        legacy_v3::upgrade_request,
        legacy_v3::matches_edit,
    ),
    (
        |wire| legacy_v4::Document::from_json(wire)?.upgrade(),
        legacy_v4::upgrade_request,
        legacy_v4::matches_edit,
    ),
    (
        |wire| legacy_v5::Document::from_json(wire)?.upgrade(),
        legacy_v5::upgrade_request,
        legacy_v5::matches_edit,
    ),
    (
        |wire| legacy_v6::Document::from_json(wire)?.upgrade(),
        legacy_v6::upgrade_request,
        legacy_v6::matches_edit,
    ),
    (
        |wire| legacy_v7::Document::from_json(wire)?.upgrade(),
        legacy_v7::upgrade_request,
        legacy_v7::matches_edit,
    ),
    (
        |wire| legacy_v8::Document::from_json(wire)?.upgrade(),
        legacy_v8::upgrade_request,
        legacy_v8::matches_edit,
    ),
    (
        |wire| legacy_v9::Document::from_json(wire)?.upgrade(),
        legacy_v9::upgrade_request,
        legacy_v9::matches_edit,
    ),
];
fn old_document(version: usize) -> Value {
    let mut old = serde_json::to_value(automatic()).unwrap();
    old["schema_version"] = json!(version);
    old.as_object_mut().unwrap().remove("basis_state");
    if version < 3 {
        old.as_object_mut().unwrap().remove("marks");
    }
    if version < 4 {
        old.as_object_mut().unwrap().remove("overrides");
    }
    old
}
#[test]
fn every_legacy_adapter_defaults_explicit_and_rejects_all_new_wire_vocabulary_even_null() {
    for (index, (document_adapter, request_adapter, edit_adapter)) in ADAPTERS.iter().enumerate() {
        let version = index + 1;
        let old = old_document(version);
        let upgraded = document_adapter(&old.to_string()).unwrap();
        assert_eq!(upgraded.basis_state(), &BasisState::explicit());
        for state in [
            Value::Null,
            serde_json::to_value(BasisState::explicit()).unwrap(),
        ] {
            let mut forged = old.clone();
            forged["basis_state"] = state;
            assert!(
                document_adapter(&forged.to_string()).is_err(),
                "schema {version}"
            );
        }
        for command in [
            Command::SetCanvas {
                width: 640,
                height: 480,
            },
            Command::AdoptPrimaryGeometry {
                width: 640,
                height: 480,
            },
        ] {
            let wire = serde_json::to_string(&request(&upgraded, command)).unwrap();
            assert!(request_adapter(&wire).is_err(), "schema {version}");
        }
        let mut new_primary = serde_json::to_value(request(&upgraded, import(None, None))).unwrap();
        new_primary["command"]["primary"] = Value::Null;
        assert!(
            request_adapter(&new_primary.to_string()).is_err(),
            "schema {version}"
        );
        let (_, transaction) = edit(
            &upgraded,
            Command::Rename {
                node: node("root"),
                label: "Renamed".into(),
            },
        );
        let mut wire = serde_json::to_value(&transaction).unwrap();
        for direction in ["forward", "inverse"] {
            if version < 3 {
                wire[direction].as_object_mut().unwrap().remove("marks");
            }
            if version < 4 {
                wire[direction].as_object_mut().unwrap().remove("overrides");
            }
        }
        assert!(edit_adapter(&wire.to_string(), &transaction).unwrap());
        for direction in ["forward", "inverse"] {
            let mut forged = wire.clone();
            forged[direction]["presentation"] = Value::Null;
            assert!(
                edit_adapter(&forged.to_string(), &transaction).is_err(),
                "schema {version}"
            );
        }
        for forward in [true, false] {
            let mut changed = transaction.clone();
            let patch = if forward {
                &mut changed.forward
            } else {
                &mut changed.inverse
            };
            let state = PresentationState {
                basis: upgraded.presentation_basis().clone(),
                state: upgraded.basis_state().clone(),
            };
            patch.presentation = Some(PresentationChange {
                before: state.clone(),
                after: state,
            });
            assert!(!edit_adapter(&wire.to_string(), &changed).unwrap());
        }
    }
}

#[test]
fn legacy_document_projection_cannot_hide_provisional_policy() {
    macro_rules! check {
        ($version:literal, $adapter:ident) => {
            assert!(
                !$adapter::Document::from_json(&old_document($version).to_string())
                    .unwrap()
                    .matches(&automatic())
            );
        };
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
}

#[test]
fn occurrence_edits_use_the_same_timed_lock_and_inverse() {
    let Command::Insert { subtree, .. } = hold() else {
        unreachable!()
    };
    let (after, _) = edit(
        &automatic(),
        Command::EditOccurrence {
            instance: InstancePath {
                node: node("root"),
                repeats: vec![],
            },
            edit: OccurrenceEdit::Insert { index: 0, subtree },
            identities: OccurrenceIdentities {
                nodes: vec![],
                marks: vec![],
            },
        },
    );
    assert_eq!(after.basis_state().rate_origin, FrameRateOrigin::TimedEdit);
    assert_eq!(after.duration().unwrap().frames(), 5);
}

#[test]
fn schema_nine_preserves_qualified_imports_without_inventing_a_primary() {
    let upgraded = legacy_v9::Document::from_json(&old_document(9).to_string())
        .unwrap()
        .upgrade()
        .unwrap();
    let command = import(Some(insertion("picture", 0, false)), None);
    let wire = serde_json::to_string(&request(&upgraded, command.clone())).unwrap();
    let upgraded_request = legacy_v9::upgrade_request(&wire).unwrap();
    assert_eq!(upgraded_request.command, command);
    let (current, transaction) = edit(&upgraded, upgraded_request.command);
    let mut old = serde_json::to_value(&current).unwrap();
    old["schema_version"] = json!(9);
    old.as_object_mut().unwrap().remove("basis_state");
    let legacy = legacy_v9::Document::from_json(&old.to_string()).unwrap();
    assert!(legacy.matches(&current));
    assert_eq!(legacy.clone().upgrade().unwrap(), current);
    assert!(
        legacy_v9::matches_edit(&serde_json::to_string(&transaction).unwrap(), &transaction)
            .unwrap()
    );
    assert!(current.basis_state().primary.is_none());
    let mut nonexplicit = serde_json::to_value(&current).unwrap();
    nonexplicit["basis_state"]["rate_origin"] = json!("timed_edit");
    assert!(!legacy.matches(&ProjectDocument::from_json(&nonexplicit.to_string()).unwrap()));
    let mut forged = serde_json::from_str::<Value>(&wire).unwrap();
    forged["command"]["primary"] = json!({"type":"keep_basis"});
    assert!(legacy_v9::upgrade_request(&forged.to_string()).is_err());
}
