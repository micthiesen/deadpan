use super::*;
use serde_json::json;

fn mapped(
    before: &ProjectDocument,
    request: &CommandRequest,
) -> (EditTransaction, ProjectDocument, ScopedIsolationRecord) {
    let (transaction, after) = apply_with_result(before, request).unwrap();
    let proof = derive_scoped_isolation(before, request, &after).unwrap();
    let record = proof.record().clone();
    let wire = serde_json::to_vec(&record).unwrap();
    let decoded: ScopedIsolationRecord = serde_json::from_slice(&wire).unwrap();
    assert_eq!(decoded, record);
    decoded.validate(before, request, &after).unwrap();
    (transaction, after, record)
}

#[test]
fn nested_isolation_maps_every_descendant_and_reverses_new_captures() {
    let before = fixture();
    let selected = target(Some(1), Some(1));
    let command = request(&before, selected.clone(), rename("edited"));
    let (transaction, after, record) = mapped(&before, &command);
    assert_eq!(record.steps().len(), 2);
    let proof = record.validate(&before, &command, &after).unwrap();
    for original in [
        selected.clone(),
        ScopedNodeTarget {
            node: node("b"),
            ..selected.clone()
        },
    ] {
        let forward = proof.map_forward(&original).unwrap();
        assert_ne!(forward.node, original.node);
        assert_ne!(forward.repeats[1].repeat, original.repeats[1].repeat);
        // A newly created request in the isolated document has no historical
        // row to restore; its scope alone is sufficient for inverse mapping.
        assert_eq!(proof.map_backward(&forward).unwrap(), original);
    }
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    let prepared = prepare_scoped_edit(&before, &command).unwrap();
    assert_eq!(prepared.isolation(), &record);
    assert_eq!(
        prepared.map_target(&before, &selected).unwrap(),
        prepared.target
    );
}

#[test]
fn selected_play_never_remaps_default_or_other_shared_plays() {
    let before = fixture();
    let command = request(&before, target(Some(1), Some(1)), rename("play two"));
    let (_, after, record) = mapped(&before, &command);
    let proof = record.validate(&before, &command, &after).unwrap();
    for unaffected in [
        target(None, None),
        target(None, Some(1)),
        target(Some(0), Some(1)),
        target(Some(2), Some(1)),
    ] {
        assert_eq!(proof.map_forward(&unaffected).unwrap(), unaffected);
        assert_eq!(proof.map_backward(&unaffected).unwrap(), unaffected);
    }
    let sibling_play = target(Some(1), Some(0));
    let mapped = proof.map_forward(&sibling_play).unwrap();
    // The entire outer subtree was cloned, but only inner play two received
    // the second clone. The sibling still follows the new outer definition.
    assert_ne!(mapped.node, sibling_play.node);
    assert_eq!(proof.map_backward(&mapped).unwrap(), sibling_play);
}

#[test]
fn default_ancestor_maps_the_same_definition_in_its_concrete_plays() {
    let before = fixture();
    let command = request(&before, target(None, Some(1)), rename("shared inner two"));
    let (_, after, record) = mapped(&before, &command);
    assert_eq!(record.steps().len(), 1);
    let proof = record.validate(&before, &command, &after).unwrap();
    for outer in [None, Some(0), Some(1), Some(2)] {
        let original = target(outer, Some(1));
        let mapped = proof.map_forward(&original).unwrap();
        assert_ne!(mapped.node, original.node);
        assert_eq!(mapped.repeats, original.repeats);
        assert_eq!(proof.map_backward(&mapped).unwrap(), original);
        let default = target(outer, None);
        assert_eq!(proof.map_forward(&default).unwrap(), default);
    }
}

fn many_request(before: &ProjectDocument, edits: Vec<ScopedTargetEdit>) -> CommandRequest {
    let requirements = before.scoped_many_requirements(&edits).unwrap();
    let identities = requirements
        .iter()
        .enumerate()
        .map(|(i, requirements)| OccurrenceIdentities {
            nodes: (0..requirements.nodes)
                .map(|n| node(&format!("many-{i}-{n}")))
                .collect(),
            marks: (0..requirements.marks)
                .map(|n| mark(&format!("many-{i}-{n}")))
                .collect(),
        })
        .collect();
    CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("many"),
        command: Command::EditScopedMany { edits, identities },
    }
}

#[test]
fn many_targets_retain_sequential_maps_and_inverse_order() {
    let before = fixture();
    let command = many_request(
        &before,
        vec![
            ScopedTargetEdit {
                target: target(Some(1), Some(0)),
                edit: rename("inner one"),
            },
            ScopedTargetEdit {
                target: target(Some(1), Some(1)),
                edit: rename("inner two"),
            },
        ],
    );
    let (_, after, record) = mapped(&before, &command);
    assert_eq!(record.steps().len(), 3);
    assert_eq!(record.steps()[0].before_prefix().len(), 1);
    assert_ne!(record.steps()[1].before_prefix()[1].repeat, node("inner"));
    let proof = record.validate(&before, &command, &after).unwrap();
    for inner in [None, Some(0), Some(1)] {
        for name in ["a", "b"] {
            let original = ScopedNodeTarget {
                node: node(name),
                ..target(Some(1), inner)
            };
            let mapped = proof.map_forward(&original).unwrap();
            assert_eq!(proof.map_backward(&mapped).unwrap(), original);
        }
    }
}

#[test]
fn later_concrete_edit_follows_an_earlier_shared_default_isolation() {
    let before = fixture();
    let command = many_request(
        &before,
        vec![
            ScopedTargetEdit {
                target: target(None, Some(1)),
                edit: rename("shared"),
            },
            ScopedTargetEdit {
                target: target(Some(2), Some(1)),
                edit: rename("third outer"),
            },
        ],
    );
    let (_, after, record) = mapped(&before, &command);
    let proof = record.validate(&before, &command, &after).unwrap();
    for outer in [0, 1, 2] {
        let original = target(Some(outer), Some(1));
        let mapped = proof.map_forward(&original).unwrap();
        assert_eq!(
            after.nodes()[&mapped.node].label,
            if outer == 2 { "third outer" } else { "shared" }
        );
        assert_eq!(proof.map_backward(&mapped).unwrap(), original);
    }
}

#[test]
fn direct_and_default_value_edits_have_empty_proofs_but_noops_refuse() {
    let before = fixture();
    for command in [
        request(&before, target(None, None), rename("definition")),
        CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("direct"),
            command: Command::Rename {
                node: node("root"),
                label: "renamed root".into(),
            },
        },
    ] {
        let (_, after, record) = mapped(&before, &command);
        assert!(record.is_empty());
        assert_eq!(
            record
                .validate(&before, &command, &after)
                .unwrap()
                .map_forward(&target(None, None))
                .unwrap(),
            target(None, None)
        );
    }
    let noop = request(&before, target(Some(1), Some(1)), rename("a"));
    assert!(apply(&before, &noop).is_err());
    assert!(derive_scoped_isolation(&before, &noop, &before).is_err());
    assert_eq!(before, fixture());
}

#[test]
fn scoped_reversion_restores_the_captured_recipe_only_in_selected_plays() {
    let original = fixture();
    let (artifact, assets) = generated();
    let acceptance = CommandRequest {
        project_id: original.project_id().clone(),
        expected_revision: original.revision_id().clone(),
        new_revision: revision("accepted"),
        command: Command::AcceptGeneratedHold {
            node: node("a"),
            artifact,
            assets,
        },
    };
    let (_, accepted) = apply_with_result(&original, &acceptance).unwrap();
    for (outer, inner, expected_clones) in [
        (None, None, 0),
        (None, Some(1), 3),
        (Some(1), None, 7),
        (Some(1), Some(1), 10),
    ] {
        let selected = target(outer, inner);
        let command = request(
            &accepted,
            selected.clone(),
            ScopedNodeEdit::RevertGeneratedHold,
        );
        let reverted = prepare(&accepted, &command);
        assert_eq!(
            reverted.document.nodes().len(),
            accepted.nodes().len() + expected_clones
        );
        assert_eq!(
            reverted.document.nodes()[&reverted.target.node],
            original.nodes()[&node("a")]
        );
        assert_eq!(reverted.transaction.duration_delta, 0);
        if expected_clones != 0 {
            assert_eq!(
                reverted.document.nodes()[&node("a")],
                accepted.nodes()[&node("a")]
            );
        }
        let proof = derive_scoped_isolation(&accepted, &command, &reverted.document).unwrap();
        assert_eq!(proof.map_backward(&reverted.target).unwrap(), selected);
    }
}

#[test]
fn scoped_fallback_reassertion_records_intent_without_manufacturing_overrides() {
    let before = fixture();
    for (outer, inner) in [
        (None, None),
        (None, Some(1)),
        (Some(1), None),
        (Some(1), Some(1)),
    ] {
        let selected = target(outer, inner);
        assert_eq!(
            before
                .scoped_edit_requirements(&selected, &ScopedNodeEdit::RevertGeneratedHold)
                .unwrap(),
            ScopedEditRequirements {
                nodes: 0,
                marks: 0,
                unchanged: false
            },
        );
        let command = request(
            &before,
            selected.clone(),
            ScopedNodeEdit::RevertGeneratedHold,
        );
        let reaffirmed = prepare(&before, &command);
        assert_ne!(reaffirmed.document.revision_id(), before.revision_id());
        assert_eq!(reaffirmed.document.nodes(), before.nodes());
        assert_eq!(reaffirmed.document.overrides(), before.overrides());
        assert_eq!(reaffirmed.document.gap_overrides(), before.gap_overrides());
        assert_eq!(reaffirmed.target, selected);
        assert!(reaffirmed.isolation().steps().is_empty());
    }
    let non_hold = ScopedNodeTarget {
        node: node("inside"),
        ..target(Some(1), Some(1))
    };
    assert_eq!(
        before
            .scoped_edit_requirements(&non_hold, &ScopedNodeEdit::RevertGeneratedHold)
            .unwrap_err()
            .code,
        EditErrorCode::WrongNodeKind
    );
}

fn holds_without_generated_fallback() -> Vec<ProjectDocument> {
    let (artifact, assets) = generated();
    let asset = artifact.sampled_asset;
    let span = assets[&asset].video.unwrap();
    [
        HoldVideo::Accepted {
            asset: asset.clone(),
            frames: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
        },
        HoldVideo::Reverse {
            asset: asset.clone(),
            span,
        },
        HoldVideo::Play { asset, span },
    ]
    .into_iter()
    .map(|video| {
        let mut document = fixture();
        document.assets = assets.clone();
        let NodeKind::Hold { recipe } = &mut document.nodes.get_mut(&node("a")).unwrap().kind
        else {
            unreachable!()
        };
        recipe.video = video;
        document.validate().unwrap();
        document
    })
    .collect()
}

#[test]
fn direct_reversion_rejects_other_picture_providers_without_an_edit() {
    for before in holds_without_generated_fallback() {
        let original = before.clone();
        let request = CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("refused-revert"),
            command: Command::RevertGeneratedHold { node: node("a") },
        };
        let error = apply_with_result(&before, &request).unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(
            error
                .message
                .contains("no saved generated-picture fallback")
        );
        assert_eq!(before, original);
    }
}

#[test]
fn scoped_reversion_rejects_other_picture_providers_before_identity_allocation() {
    for before in holds_without_generated_fallback() {
        let original = before.clone();
        for (outer, inner) in [
            (None, None),
            (None, Some(1)),
            (Some(1), None),
            (Some(1), Some(1)),
        ] {
            let selected = target(outer, inner);
            let error = before
                .scoped_edit_requirements(&selected, &ScopedNodeEdit::RevertGeneratedHold)
                .unwrap_err();
            assert_eq!(error.code, EditErrorCode::InvalidCommand);
            let request = CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: revision("refused-scoped-revert"),
                command: Command::EditScoped {
                    target: selected,
                    edit: ScopedNodeEdit::RevertGeneratedHold,
                    identities: OccurrenceIdentities {
                        nodes: Vec::new(),
                        marks: Vec::new(),
                    },
                },
            };
            let error = prepare_scoped_edit(&before, &request).unwrap_err();
            assert_eq!(error.code, EditErrorCode::InvalidCommand);
            assert!(
                error
                    .message
                    .contains("no saved generated-picture fallback")
            );
            assert!(apply_with_result(&before, &request).is_err());
            assert_eq!(before, original);
        }
    }
}

#[test]
fn occurrence_commands_capture_their_actual_isolation_too() {
    let before = fixture();
    let mut command = request(&before, target(Some(1), Some(1)), rename("occurrence"));
    let Command::EditScoped { identities, .. } = &command.command else {
        unreachable!()
    };
    command.command = Command::EditOccurrence {
        instance: instance(1, 1),
        edit: OccurrenceEdit::Rename {
            label: "occurrence".into(),
        },
        identities: identities.clone(),
    };
    let (_, after, record) = mapped(&before, &command);
    assert_eq!(record.steps().len(), 2);
    let proof = record.validate(&before, &command, &after).unwrap();
    let original = target(Some(1), Some(1));
    let mapped = proof.map_forward(&original).unwrap();
    assert_eq!(after.nodes()[&mapped.node].label, "occurrence");
    assert_eq!(proof.map_backward(&mapped).unwrap(), original);
}

#[test]
fn first_play_attachment_isolation_also_carries_unrelated_pending_descendants() {
    let mut before = fixture();
    add_owned_mark(
        &mut before,
        "local",
        Anchor::Local {
            node: node("a"),
            position: ExactRatio::ONE,
        },
        MarkState::Bound,
    );
    before.validate().unwrap();
    let count = before.first_play_attachment_nodes(&node("outer")).unwrap();
    let command = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("attachments"),
        command: Command::KeepFirstPlayAttachments {
            node: node("outer"),
            identities: OccurrenceIdentities {
                nodes: (0..count).map(|n| node(&format!("attached-{n}"))).collect(),
                marks: vec![],
            },
        },
    };
    let (_, after, record) = mapped(&before, &command);
    assert_eq!(record.steps().len(), 1);
    let proof = record.validate(&before, &command, &after).unwrap();
    let pending = ScopedNodeTarget {
        node: node("b"),
        ..target(Some(0), None)
    };
    let mapped = proof.map_forward(&pending).unwrap();
    assert_ne!(mapped.node, pending.node);
    assert_eq!(proof.map_backward(&mapped).unwrap(), pending);
    let shared = target(None, None);
    assert_eq!(proof.map_forward(&shared).unwrap(), shared);
}

#[test]
fn serialized_mapping_forgery_never_obtains_a_validated_capability() {
    let before = fixture();
    let command = request(&before, target(Some(1), Some(1)), rename("change"));
    let (_, after, record) = mapped(&before, &command);
    let wire = json!(record);
    for mutation in [
        "destination",
        "before-prefix",
        "after-prefix",
        "order",
        "empty",
        "revision",
    ] {
        let mut value = wire.clone();
        match mutation {
            "destination" => {
                let key = value["steps"][0]["nodes"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .next()
                    .unwrap()
                    .clone();
                value["steps"][0]["nodes"][key] = json!("forged-fresh");
            }
            "before-prefix" => {
                value["steps"][0]["before_prefix"][0]["branch"] = json!(RepeatEditBranch::Play {
                    iteration: play("outer", 2)
                })
            }
            "after-prefix" => {
                value["steps"][0]["after_prefix"][0]["branch"] = json!(RepeatEditBranch::Play {
                    iteration: play("outer", 2)
                })
            }
            "order" => value["steps"].as_array_mut().unwrap().reverse(),
            "empty" => value["steps"] = json!([]),
            "revision" => value["after"] = json!("other-revision"),
            _ => unreachable!(),
        }
        let forged: ScopedIsolationRecord = serde_json::from_value(value).unwrap();
        assert!(
            forged.validate(&before, &command, &after).is_err(),
            "{mutation}"
        );
    }
    let mut duplicate = wire.clone();
    let keys: Vec<_> = duplicate["steps"][0]["nodes"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    duplicate["steps"][0]["nodes"][&keys[1]] = duplicate["steps"][0]["nodes"][&keys[0]].clone();
    assert!(serde_json::from_value::<ScopedIsolationRecord>(duplicate).is_err());
    let mut excessive = wire.clone();
    excessive["steps"][0]["before_prefix"] = json!(vec![
        record.steps()[0].before_prefix()[0]
            .clone();
        MAX_DOCUMENT_DEPTH + 1
    ]);
    assert!(serde_json::from_value::<ScopedIsolationRecord>(excessive).is_err());
    let mut unknown = wire;
    unknown["unknown"] = json!(true);
    assert!(serde_json::from_value::<ScopedIsolationRecord>(unknown).is_err());
}

#[test]
fn exact_command_and_full_result_are_part_of_mapping_admission() {
    let before = fixture();
    let command = request(&before, target(Some(1), Some(1)), rename("change"));
    let (_, after, record) = mapped(&before, &command);
    let mut altered = command.clone();
    let Command::EditScoped { edit, .. } = &mut altered.command else {
        unreachable!()
    };
    *edit = rename("different value");
    assert!(record.validate(&before, &altered, &after).is_err());
    let mut altered = after.clone();
    altered.nodes.get_mut(&node("root")).unwrap().label = "unrelated forged change".into();
    altered.validate().unwrap();
    assert!(record.validate(&before, &command, &altered).is_err());
}

#[test]
fn compound_leaf_isolations_compose_and_reverse_as_one_history_entry() {
    let before = fixture();
    let first = request(&before, target(Some(1), Some(0)), rename("first leaf"));
    let (_, staged, first_record) = mapped(&before, &first);
    let next_target = first_record
        .validate(&before, &first, &staged)
        .unwrap()
        .map_forward(&target(Some(1), Some(1)))
        .unwrap();
    let second = request(&staged, next_target, rename("second leaf"));
    let transaction = ResolvedTransaction::new(
        0,
        BTreeMap::new(),
        vec![
            ResolvedStep::Edit {
                edit: LeafEdit::new(first.new_revision.clone(), first.command.clone()).unwrap(),
            },
            ResolvedStep::Edit {
                edit: LeafEdit::new(second.new_revision.clone(), second.command.clone()).unwrap(),
            },
        ],
    )
    .unwrap();
    let command = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("compound-end"),
        command: Command::Compound { transaction },
    };
    let (transaction, after, record) = mapped(&before, &command);
    assert_eq!(record.steps().len(), 3);
    let proof = record.validate(&before, &command, &after).unwrap();
    for inner in [Some(0), Some(1), None] {
        let original = ScopedNodeTarget {
            node: node("b"),
            ..target(Some(1), inner)
        };
        let current = proof.map_forward(&original).unwrap();
        assert_eq!(proof.map_backward(&current).unwrap(), original);
    }
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
}

fn generated() -> (GeneratedArtifact, BTreeMap<AssetId, AssetRecord>) {
    let object = |digit: char| {
        GeneratedObjectRef::new(
            GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
            100,
        )
        .unwrap()
    };
    let sampled_object = object('a');
    let native_object = object('b');
    let sampled_asset = AssetId::new("sampled").unwrap();
    let native_asset = AssetId::new("native").unwrap();
    let artifact = GeneratedArtifact {
        sampled_asset: sampled_asset.clone(),
        sampled_object: sampled_object.clone(),
        native_asset: native_asset.clone(),
        native_object: native_object.clone(),
        provenance: object('c'),
        sampling: BridgeSamplingMap::new(
            FrameRate::new(30000, 1001).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            frames(5),
            frames(4),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap()
        .into(),
        content_aspect: Some([640, 480]),
    };
    let time_base = SourceTimeBase::new(1, 1000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 10000,
            time_base,
        },
    )
    .unwrap();
    let asset = |object: &GeneratedObjectRef, count| AssetRecord {
        label: "generated".into(),
        content_hash: object.content().to_string(),
        video: Some(span),
        audio: None,
        still_image: false,
        frame_count: Some(frames(count)),
        source_qualification: None,
    };
    (
        artifact,
        BTreeMap::from([
            (sampled_asset, asset(&sampled_object, 4)),
            (native_asset, asset(&native_object, 5)),
        ]),
    )
}

#[test]
fn scoped_generated_acceptance_preserves_the_recipe_and_other_plays() {
    let mut before = fixture();
    let NodeKind::Hold { recipe } = &mut before.nodes.get_mut(&node("a")).unwrap().kind else {
        unreachable!()
    };
    recipe.audio = HoldAudio::Tone {
        frequency_hz: 440,
        level: GainDb::new(-6000).unwrap(),
    };
    recipe.picture_context = Some(
        CapturedFraming::new(vec![CapturedCanvas {
            width: 640,
            height: 480,
            fit: CapturedFit::Fill,
            layers: vec![],
        }])
        .unwrap(),
    );
    let original = recipe.clone();
    before.validate().unwrap();
    let (artifact, assets) = generated();
    let selected = target(Some(1), Some(1));
    let command = request(
        &before,
        selected.clone(),
        ScopedNodeEdit::AcceptGeneratedHold {
            artifact: artifact.clone(),
            assets: assets.clone(),
        },
    );
    let prepared = prepare(&before, &command);
    let NodeKind::Hold { recipe } = &prepared.document.nodes()[&prepared.target.node].kind else {
        unreachable!()
    };
    assert_eq!(recipe.duration, original.duration);
    assert_eq!(recipe.audio, original.audio);
    assert_eq!(recipe.picture_context, original.picture_context);
    let HoldVideo::Generated { accepted } = &recipe.video else {
        panic!("accepted provider")
    };
    assert_eq!(accepted.artifact, artifact);
    assert_eq!(accepted.fallback, HoldFallback::Background);
    assert_eq!(
        prepared.document.nodes()[&node("a")].kind,
        NodeKind::Hold { recipe: original }
    );
    assert_eq!(prepared.transaction.duration_delta, 0);
    let proof = derive_scoped_isolation(&before, &command, &prepared.document).unwrap();
    assert_eq!(proof.map_backward(&prepared.target).unwrap(), selected);
    let same = request(
        &prepared.document,
        prepared.target.clone(),
        ScopedNodeEdit::AcceptGeneratedHold { artifact, assets },
    );
    assert!(apply(&prepared.document, &same).is_err());
}

#[test]
fn invalid_scoped_acceptance_cannot_admit_assets_or_isolate() {
    let before = fixture();
    let (artifact, assets) = generated();
    let mut command = request(
        &before,
        target(Some(1), Some(1)),
        ScopedNodeEdit::AcceptGeneratedHold {
            artifact: artifact.clone(),
            assets: assets.clone(),
        },
    );
    let Command::EditScoped { edit, .. } = &mut command.command else {
        unreachable!()
    };
    *edit = ScopedNodeEdit::AcceptGeneratedHold {
        artifact: artifact.clone(),
        assets: BTreeMap::new(),
    };
    assert!(apply(&before, &command).is_err());
    let mut bad = assets;
    bad.insert(
        AssetId::new("unrelated").unwrap(),
        bad.values().next().unwrap().clone(),
    );
    assert!(
        before
            .scoped_edit_requirements(
                &target(Some(1), Some(1)),
                &ScopedNodeEdit::AcceptGeneratedHold {
                    artifact: artifact.clone(),
                    assets: bad
                }
            )
            .is_err()
    );
    let non_hold = ScopedNodeTarget {
        node: node("inside"),
        ..target(Some(1), Some(1))
    };
    assert!(
        before
            .scoped_edit_requirements(
                &non_hold,
                &ScopedNodeEdit::AcceptGeneratedHold {
                    artifact,
                    assets: BTreeMap::new()
                }
            )
            .is_err()
    );
    assert!(before.assets().is_empty());
    assert!(before.overrides().is_empty());
    assert_eq!(before, fixture());
}
