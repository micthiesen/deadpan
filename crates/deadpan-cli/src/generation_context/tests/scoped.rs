use super::*;
use deadpan_jobs::{
    ConditioningMode, HoldConstraints, MotionAmount, Relevance, RequestId, RequestVersion,
    TargetBinding, VideoSpec,
};
use deadpan_store::generation_preparations::{PreparationControls, PreparationOrigin};

fn fixture() -> ProjectDocument {
    document(
        &["speed"],
        vec![
            (
                "speed",
                node(NodeKind::Retime {
                    purpose: RetimePurpose::Edit,
                    child: id("repeat"),
                    duration: FrameDuration::new(78).unwrap(),
                    mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(78)).unwrap(),
                    pitch: PitchPolicy::Preserve,
                }),
            ),
            (
                "repeat",
                node(NodeKind::Repeat {
                    child: id("body"),
                    iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3).unwrap(),
                    gap: None,
                    escalation: None,
                }),
            ),
            (
                "body",
                BeatNode::sequence("Body", vec![id("left"), id("h"), id("right")]),
            ),
            ("left", source(10, 0)),
            ("h", hold(6)),
            ("right", source(10, 10)),
        ],
    )
}

fn scope(node: &str, play: Option<u32>) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: id(node),
        repeats: vec![RepeatEditStep {
            repeat: id("repeat"),
            branch: play.map_or(RepeatEditBranch::Default, |ordinal| {
                RepeatEditBranch::Play {
                    iteration: IterationId {
                        allocation: RevisionId::new("plays").unwrap(),
                        ordinal,
                    },
                }
            }),
        }],
    }
}

fn request(
    document: &ProjectDocument,
    target: ScopedNodeTarget,
    region: Option<TargetId>,
) -> StoredGenerationRequest {
    target.validate(document).unwrap();
    let request_id = RequestId::new("request").unwrap();
    StoredGenerationRequest {
        scope_id: deadpan_store::generation::GenerationScopeId::from_first_request(
            request_id.clone(),
        ),
        request_id,
        origin_revision: document.revision_id().clone(),
        origin_target: target.clone(),
        target: target.clone(),
        binding: TargetBinding {
            project_id: document.project_id().clone(),
            hold_id: target.node,
            request_version: RequestVersion::new(1).unwrap(),
            context_sha256: deadpan_jobs::Sha256::new("b".repeat(64)).unwrap(),
        },
        constraints: HoldConstraints {
            video: VideoSpec::new(
                FrameDuration::new(6).unwrap(),
                document.presentation_basis().frame_rate,
                512,
                320,
            )
            .unwrap(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
            instructions: None,
            region_target: region,
        },
        provider: crate::generation::development_provider(1),
        bridge_plan: None,
        relevance: Relevance::Current,
    }
}

fn current(request: &StoredGenerationRequest) -> ContextObservation {
    ContextObservation::Resolved(request.binding.context_sha256.clone())
}

fn changed(document: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
    apply_with_result(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        },
    )
    .unwrap()
    .1
}

fn isolate_sibling(document: &ProjectDocument) -> PreparedScopedEdit {
    let target = scope("left", Some(1));
    let edit = ScopedNodeEdit::Rename {
        label: "Play 2 left source".into(),
    };
    let requirements = document.scoped_edit_requirements(&target, &edit).unwrap();
    prepare_scoped_edit(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("isolated-play-2").unwrap(),
            command: Command::EditScoped {
                target,
                edit,
                identities: OccurrenceIdentities {
                    nodes: (0..requirements.nodes)
                        .map(|index| id(&format!("play2-node-{index}")))
                        .collect(),
                    marks: (0..requirements.marks)
                        .map(|index| MarkId::new(format!("play2-mark-{index}")).unwrap())
                        .collect(),
                },
            },
        },
    )
    .unwrap()
}

fn mapped_request(
    origin: &ProjectDocument,
    isolated: &PreparedScopedEdit,
    play: Option<u32>,
) -> StoredGenerationRequest {
    let mut request = request(origin, scope("h", play), None);
    request.target = isolated.map_target(origin, &request.origin_target).unwrap();
    request
}

fn preparation(origin: &ProjectDocument, target: ScopedNodeTarget) -> StoredGenerationPreparation {
    use deadpan_store::generation_preparations::{PreparationId, PreparationState};
    let object =
        GeneratedObjectRef::new(GeneratedContentId::new("a".repeat(64)).unwrap(), 1).unwrap();
    StoredGenerationPreparation {
        id: PreparationId::new("preparation".to_owned()).unwrap(),
        project_id: origin.project_id().clone(),
        origin_revision: origin.revision_id().clone(),
        current_revision: origin.revision_id().clone(),
        origin_target: target.clone(),
        target,
        duration: FrameDuration::new(6).unwrap(),
        origin: PreparationOrigin::AcceptedExtension {
            accepted: Box::new(GeneratedArtifact {
                sampled_asset: AssetId::new("sampled").unwrap(),
                native_asset: AssetId::new("native").unwrap(),
                sampled_object: object.clone(),
                native_object: object.clone(),
                provenance: object,
                sampling: BridgeSamplingMap::new(
                    FrameRate::new(30000, 1001).unwrap(),
                    FrameRate::new(24, 1).unwrap(),
                    FrameDuration::new(5).unwrap(),
                    FrameDuration::new(4).unwrap(),
                    BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
                )
                .unwrap(),
                content_aspect: None,
            }),
            controls: PreparationControls::AcceptedArtifact,
        },
        intent: deadpan_store::generation_intents::IntentBirthReceipt {
            schema_version: 1,
            history_id: 1,
            cause: deadpan_store::generation_intents::IntentCause::DurationExtension,
            authorization: deadpan_store::generation_intents::IntentAuthorization::AuthoredOrigin,
            fallback: deadpan_core::HoldFallback::Background,
            input_binding: deadpan_store::generation_intents::IntentInputBinding::Unavailable {
                cause:
                    deadpan_store::generation_intents::InputUnavailableCause::MissingQualification,
                detail: "The resolver fixture has no measured source receipt.".into(),
            },
        },
        state: PreparationState::Queued,
        claim_sequence: 0,
        reason: None,
        request_id: None,
    }
}

#[test]
fn inserted_preparation_has_saved_controls_without_a_package_or_model() {
    let document = fixture();
    let mut preparation = preparation(&document, scope("h", None));
    preparation.origin = PreparationOrigin::inserted_pause();
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let options = crate::generation::preparations::resolve_options(
        std::path::Path::new("/no-such-deadpan-preparation-package"),
        &preparation,
        &cancelled,
    )
    .unwrap();
    assert_eq!(options.motion, MotionAmount::Still);
    assert_eq!(options.instructions, None);
    assert_eq!(options.region_target, deadpan_jobs::GenerationTarget::None);
    let resolver = BoundaryContextResolver::default();
    let isolated = isolate_sibling(&document);
    preparation.target = isolated
        .map_target(&document, &preparation.origin_target)
        .unwrap();
    assert!(resolver.preparation_is_relevant(&document, &isolated.document, &preparation));
    cancelled.store(true, std::sync::atomic::Ordering::Release);
    assert!(
        crate::generation::preparations::resolve_options(
            std::path::Path::new("/no-such-deadpan-preparation-package"),
            &preparation,
            &cancelled,
        )
        .is_err()
    );
}

#[test]
fn replacement_preparation_keeps_raw_context_through_proven_sibling_isolation() {
    let origin = fixture();
    let isolated = isolate_sibling(&origin);
    let resolver = BoundaryContextResolver::default();
    for play in [None, Some(1), Some(2)] {
        let mut preparation = preparation(&origin, scope("h", play));
        preparation.target = isolated
            .map_target(&origin, &preparation.origin_target)
            .unwrap();
        assert!(resolver.preparation_is_relevant(&origin, &isolated.document, &preparation));
        let prepared = resolver.prepare_transition(&isolated.document).unwrap();
        assert!(prepared.preparation_is_relevant(&origin, &isolated.document, &preparation));
        let changed = changed(
            &isolated.document,
            "changed-source",
            Command::SetCutaways {
                node: isolated.target.node.clone(),
                cutaways: vec![cutaway(0, 10, false)],
            },
        );
        assert!(
            !prepared.preparation_is_relevant(&origin, &changed, &preparation),
            "a prepared resolver may not observe a different document"
        );
        assert_eq!(
            resolver.preparation_is_relevant(&origin, &changed, &preparation),
            play != Some(1),
            "only the edited play loses its raw boundary"
        );
    }
}

#[test]
fn sibling_isolation_keeps_default_and_other_plays_separate_from_the_mapped_hold() {
    let origin = fixture();
    let isolated = isolate_sibling(&origin);
    let resolver = BoundaryContextResolver::default();
    for play in [None, Some(1), Some(2)] {
        let request = mapped_request(&origin, &isolated, play);
        assert_eq!(request.origin_target, scope("h", play));
        assert_eq!(request.binding.hold_id, id("h"));
        assert_eq!(
            resolver.observe(&origin, &isolated.document, &request),
            current(&request)
        );
        if play == Some(1) {
            assert_ne!(request.target.node, request.origin_target.node);
            let mut unmapped = request.clone();
            unmapped.target = request.origin_target.clone();
            assert_eq!(
                resolver.observe(&origin, &isolated.document, &unmapped),
                ContextObservation::Unresolved
            );
            let mut wrong_origin = request.clone();
            wrong_origin.origin_target = request.target.clone();
            assert_eq!(
                resolver.observe(&origin, &isolated.document, &wrong_origin),
                ContextObservation::Unresolved
            );
        } else {
            assert_eq!(request.target, request.origin_target);
        }
    }
    // An unscoped structural Hold ID cannot stand in for any of these scopes.
    assert!(context_identity(&origin, &id("h")).is_none());
}

fn cutaway(start: i64, end: i64, removed: bool) -> Cutaway {
    Cutaway {
        range: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        asset: AssetId::new("video").unwrap(),
        selection: ExactSourceSpan::from(
            SourceSpan::new(
                SourceTimestamp {
                    ticks: 100 * 1001,
                    time_base: clock(),
                },
                SourceTimestamp {
                    ticks: 101 * 1001,
                    time_base: clock(),
                },
            )
            .unwrap(),
        ),
        fit: CutawayFit::Hold,
        removed,
    }
}

#[test]
fn changed_inner_source_or_boundary_cutaway_stales_only_its_isolated_play() {
    let origin = fixture();
    let isolated = isolate_sibling(&origin);
    let left = isolated.target.node.clone();
    let right = isolated
        .map_target(&origin, &scope("right", Some(1)))
        .unwrap()
        .node;
    let resolver = BoundaryContextResolver::default();
    // Source slipping inside Repeat/Retime is deliberately unavailable to
    // current commands. Reconstruct validated snapshots to test relevance of
    // changed source selections independently of that editing restriction.
    let source_changed = |node: &NodeId, first: i64, revision: &str| {
        let mut replacement = isolated.document.nodes()[node].clone();
        let NodeKind::Source { source: previous } = &mut replacement.kind else {
            panic!("source fixture")
        };
        let NodeKind::Source { source: shifted } = source(10, first).kind else {
            unreachable!()
        };
        previous.video = shifted.video;
        let mut wire = serde_json::to_value(&isolated.document).unwrap();
        wire["nodes"][node.as_str()] = serde_json::to_value(replacement).unwrap();
        wire["revision_id"] = serde_json::json!(revision);
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let mutations = [
        source_changed(&left, 2, "changed-left-span"),
        source_changed(&right, 12, "changed-right-span"),
        changed(
            &isolated.document,
            "boundary-cutaway",
            Command::SetCutaways {
                node: left.clone(),
                cutaways: vec![cutaway(9, 10, false)],
            },
        ),
        changed(
            &isolated.document,
            "removed-boundary",
            Command::SetCutaways {
                node: left.clone(),
                cutaways: vec![cutaway(9, 10, true)],
            },
        ),
    ];
    for (index, after) in mutations.into_iter().enumerate() {
        for play in [None, Some(1), Some(2)] {
            let request = mapped_request(&origin, &isolated, play);
            let expected = if play == Some(1) {
                ContextObservation::Unresolved
            } else {
                current(&request)
            };
            assert_eq!(
                resolver.observe(&origin, &after, &request),
                expected,
                "mutation {index}, play {play:?}"
            );
        }
    }
    let away_from_boundary = changed(
        &isolated.document,
        "earlier-cutaway",
        Command::SetCutaways {
            node: left,
            cutaways: vec![cutaway(0, 1, false)],
        },
    );
    let request = mapped_request(&origin, &isolated, Some(1));
    assert_eq!(
        resolver.observe(&origin, &away_from_boundary, &request),
        current(&request)
    );
}

#[test]
fn prepared_observations_bind_exact_snapshot_and_retry_reused_revision() {
    let origin = fixture();
    let isolated = isolate_sibling(&origin);
    let candidate = changed(
        &isolated.document,
        "prospective-revision",
        Command::Rename {
            node: id("root"),
            label: "Renamed root".into(),
        },
    );
    // A rejected write leaves its proposed revision ID available for a
    // different command. These two candidates must not share an after plan.
    let retry = changed(
        &isolated.document,
        "prospective-revision",
        Command::SetCutaways {
            node: isolated.target.node.clone(),
            cutaways: vec![cutaway(9, 10, false)],
        },
    );
    assert_eq!(candidate.project_id(), retry.project_id());
    assert_eq!(candidate.revision_id(), retry.revision_id());
    let resolver = BoundaryContextResolver::default();
    let prepared = resolver.prepare_transition(&candidate).unwrap();
    for play in [None, Some(1), Some(2), Some(1)] {
        let request = mapped_request(&origin, &isolated, play);
        assert_eq!(
            prepared.observe(&origin, &candidate, &request),
            current(&request)
        );
    }

    let default_request = mapped_request(&origin, &isolated, None);
    let equivalent = candidate.clone();
    assert_eq!(
        resolver.observe(&origin, &equivalent, &default_request),
        current(&default_request)
    );
    assert_eq!(
        prepared.observe(&origin, &equivalent, &default_request),
        ContextObservation::Unresolved,
        "even equal contents at another address are outside the prepared transition"
    );
    assert_eq!(
        prepared.observe(&origin, &retry, &default_request),
        ContextObservation::Unresolved,
        "the guard must reject a different snapshot even when this scope is unchanged"
    );

    let play2_request = mapped_request(&origin, &isolated, Some(1));
    assert_ne!(
        scoped_context_identity(&candidate, &play2_request.target),
        scoped_context_identity(&retry, &play2_request.target)
    );
    assert_eq!(
        resolver.observe(&origin, &candidate, &play2_request),
        current(&play2_request)
    );
    assert_eq!(
        resolver.observe(&origin, &retry, &play2_request),
        ContextObservation::Unresolved,
        "direct observations must also recompile a retried prospective revision"
    );
    let retried = resolver.prepare_transition(&retry).unwrap();
    for play in [None, Some(1), Some(2)] {
        let request = mapped_request(&origin, &isolated, play);
        let expected = if play == Some(1) {
            ContextObservation::Unresolved
        } else {
            current(&request)
        };
        assert_eq!(
            retried.observe(&origin, &retry, &request),
            expected,
            "retried play {play:?}"
        );
    }
}

#[test]
fn immutable_origin_plan_reuses_and_changes_at_project_or_revision() {
    let origin = fixture();
    let isolated = isolate_sibling(&origin);
    let resolver = BoundaryContextResolver::default();
    let first = mapped_request(&origin, &isolated, None);
    assert_eq!(
        resolver.observe(&origin, &isolated.document, &first),
        current(&first)
    );
    assert!(
        resolver.origin.lock().unwrap().is_some(),
        "observe must populate its origin cache"
    );
    let initial_origin = cached_plan(&resolver.origin, &origin).unwrap();
    let prepared = resolver.prepare_transition(&isolated.document).unwrap();
    for play in [Some(1), Some(2), None, Some(1)] {
        let request = mapped_request(&origin, &isolated, play);
        assert_eq!(
            prepared.observe(&origin, &isolated.document, &request),
            current(&request)
        );
        assert!(Arc::ptr_eq(
            &initial_origin,
            &cached_plan(&resolver.origin, &origin).unwrap()
        ));
    }

    let newer = changed(
        &origin,
        "next-origin-revision",
        Command::SetCutaways {
            node: id("left"),
            cutaways: vec![cutaway(9, 10, false)],
        },
    );
    assert_ne!(
        scoped_context_identity(&origin, &scope("h", None)),
        scoped_context_identity(&newer, &scope("h", None))
    );
    let newer_request = request(&newer, scope("h", None), None);
    assert_eq!(
        resolver.observe(&newer, &newer, &newer_request),
        current(&newer_request)
    );
    let newer_origin = cached_plan(&resolver.origin, &newer).unwrap();
    assert!(!Arc::ptr_eq(&initial_origin, &newer_origin));

    // Another project may reuse the revision and node IDs with different
    // source pictures. Reusing the prior project's plan would stale this job.
    let mut wire = serde_json::to_value(&origin).unwrap();
    wire["project_id"] = serde_json::json!("another-project");
    wire["revision_id"] = serde_json::to_value(newer.revision_id()).unwrap();
    let other_origin = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(newer.revision_id(), other_origin.revision_id());
    assert_ne!(
        scoped_context_identity(&newer, &scope("h", None)),
        scoped_context_identity(&other_origin, &scope("h", None))
    );
    let other_isolated = isolate_sibling(&other_origin);
    let other_request = mapped_request(&other_origin, &other_isolated, Some(1));
    assert_eq!(
        resolver.observe(&other_origin, &other_isolated.document, &other_request),
        current(&other_request)
    );
    let other_plan = cached_plan(&resolver.origin, &other_origin).unwrap();
    assert!(!Arc::ptr_eq(&newer_origin, &other_plan));
    assert_eq!(
        resolver.observe(&other_origin, &other_isolated.document, &other_request),
        current(&other_request)
    );
    assert!(Arc::ptr_eq(
        &other_plan,
        &cached_plan(&resolver.origin, &other_origin).unwrap()
    ));
}

#[test]
fn outer_retime_gain_and_camera_keep_the_raw_definition_inputs_relevant() {
    let origin = fixture();
    let resolver = BoundaryContextResolver::default();
    let framing = Framing::creep(
        FramingPose::identity(),
        FramingPose::new(
            ExactRatio::new(1, 3).unwrap(),
            ExactRatio::new(2, 3).unwrap(),
            ExactRatio::integer(2),
        )
        .unwrap(),
        FramingCurve::Linear,
    )
    .unwrap();
    let gain = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-6000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    let mut after = origin.clone();
    for (index, command) in [
        Command::SetRetime {
            node: id("speed"),
            duration: FrameDuration::new(7).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
        Command::SetFraming {
            node: id("speed"),
            framing: Some(framing.clone()),
        },
        Command::SetAudioTreatments {
            node: id("speed"),
            treatments: gain.clone(),
        },
        Command::SetFraming {
            node: id("left"),
            framing: Some(framing),
        },
        Command::SetAudioTreatments {
            node: id("h"),
            treatments: gain,
        },
    ]
    .into_iter()
    .enumerate()
    {
        after = changed(&after, &format!("presentation-{index}"), command);
        for play in [None, Some(1), Some(2)] {
            let request = request(&origin, scope("h", play), None);
            assert_eq!(
                resolver.observe(&origin, &after, &request),
                current(&request),
                "edit {index}, play {play:?}"
            );
        }
    }
    assert_ne!(
        origin.structural_duration().unwrap(),
        after.structural_duration().unwrap()
    );
    assert!(after.nodes()[&id("left")].framing.is_some());
    assert!(!after.nodes()[&id("h")].audio_treatments.is_empty());
}

fn region(document: &ProjectDocument) -> AttentionTarget {
    AttentionTarget {
        label: "Captured subject".into(),
        asset: AssetId::new("video").unwrap(),
        span: document.assets()[&AssetId::new("video").unwrap()]
            .video
            .unwrap(),
        region: TargetRegion {
            center: [400_000, 500_000],
            size: [200_000, 200_000],
        },
        samples: vec![],
        corrections: vec![],
        provenance: None,
    }
}

#[test]
fn selected_region_changes_stale_the_mapped_request_without_changing_its_pictures() {
    let bare = fixture();
    let saved = region(&bare);
    let origin = changed(
        &bare,
        "captured-region",
        Command::SetTarget {
            id: TargetId::new("subject").unwrap(),
            target: saved.clone(),
        },
    );
    let isolated = isolate_sibling(&origin);
    let mut request = mapped_request(&origin, &isolated, Some(1));
    request.constraints.region_target = Some(TargetId::new("subject").unwrap());
    let resolver = BoundaryContextResolver::default();
    assert_eq!(
        resolver.observe(&origin, &isolated.document, &request),
        current(&request)
    );
    let mut corrected = saved.clone();
    corrected.corrections.push(TargetCorrection {
        at: 9 * 1001,
        region: TargetRegion {
            center: [600_000, 500_000],
            size: [200_000, 200_000],
        },
    });
    for (index, command) in [
        Command::SetTarget {
            id: TargetId::new("subject").unwrap(),
            target: corrected,
        },
        Command::DeleteTarget {
            id: TargetId::new("subject").unwrap(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let after = changed(
            &isolated.document,
            &format!("region-change-{index}"),
            command,
        );
        assert_eq!(
            scoped_context_identity(&origin, &request.origin_target),
            scoped_context_identity(&after, &request.target)
        );
        assert_eq!(
            resolver.observe(&origin, &after, &request),
            ContextObservation::Unresolved
        );
    }
    let unrelated = changed(
        &isolated.document,
        "another-region",
        Command::SetTarget {
            id: TargetId::new("another-subject").unwrap(),
            target: saved,
        },
    );
    assert_eq!(
        resolver.observe(&origin, &unrelated, &request),
        current(&request)
    );
}

#[test]
fn a_missing_selected_region_is_never_filled_by_later_saved_targets() {
    let origin = fixture();
    let isolated = isolate_sibling(&origin);
    let later = changed(
        &isolated.document,
        "late-region",
        Command::SetTarget {
            id: TargetId::new("subject").unwrap(),
            target: region(&origin),
        },
    );
    let resolver = BoundaryContextResolver::default();
    for play in [None, Some(1)] {
        let mut request = mapped_request(&origin, &isolated, play);
        request.constraints.region_target = Some(TargetId::new("subject").unwrap());
        assert_eq!(
            resolver.observe(&origin, &isolated.document, &request),
            ContextObservation::Unresolved
        );
        assert_eq!(
            resolver.observe(&origin, &later, &request),
            ContextObservation::Unresolved
        );
        request.constraints.region_target = None;
        assert_eq!(
            resolver.observe(&origin, &later, &request),
            current(&request),
            "captured absence must stay absent"
        );
    }
}
