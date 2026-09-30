use super::*;
use crate::render::{RenderAutomaticPolicy, RenderAutomaticSelection, RenderPolicy};

// Exact observation projection from the retained real bound-project run in
// tools/media-qualification/evidence/2026-09-30-encoder-runtime/bound-project.json.
// Added settings come from that run's actual binding. This fixture grants no
// live admission and these unit tests do not rerun or qualify its native media.
const MEASURED: &[u8] = include_bytes!("tests/measured-decision-v1.json");

fn fixture() -> RenderEncodingDecision {
    RenderEncodingDecision::from_json(MEASURED).unwrap()
}

fn intent(value: &RenderEncodingDecision) -> RenderIntent {
    RenderIntent {
        schema_version: 2,
        job_id: value.job_id.clone(),
        project_id: value.output.project_id.clone(),
        revision_id: value.output.revision_id.clone(),
        document_sha256: value.document_sha256.clone(),
        range: value.output.range,
        policy: RenderPolicy::Automatic(RenderAutomaticPolicy {
            schema_version: 1,
            selection: RenderAutomaticSelection::Automatic,
            algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
        }),
    }
}

fn selected_mut(value: &mut RenderEncodingDecision) -> &mut RenderProbeReport {
    match &mut value.probes.last_mut().unwrap().result {
        RenderProbeOutcome::Succeeded { report } => report,
        _ => panic!("measured fixture ends in success"),
    }
}

#[test]
fn measured_decision_preserves_full_success_rejection_and_output_evidence() {
    let value = fixture();
    value
        .validate_for(&intent(&value), &value.encoding_attempt_id)
        .unwrap();
    assert_eq!(value.probes.len(), 2);
    assert!(matches!(
        value.probes[0].result,
        RenderProbeOutcome::Rejected {
            failure: RenderProbeFailure {
                kind: RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::VideoTimestampOrder),
                ..
            },
            ..
        }
    ));
    let selected = value.selected().unwrap();
    assert_eq!(selected.spec.choice.mode, RenderEncoder::Hardware);
    assert_eq!(selected.spec.choice.b_frames, RenderBFrames::None);
    assert_eq!(selected.manifest.contract.picture.frame_count, 46);
    assert_eq!(
        selected.manifest.contract.picture.project_audio_end,
        AudioSample(73_674)
    );
    assert_eq!(value.output.frame_count, 128);
    assert_eq!(selected.verification.fresh_gop_frames, 46);
    assert_eq!(selected.content.markers[0].observed_samples, [100, 137]);
    assert_eq!(selected.settings.movie_timescale, 240_000);
    assert_eq!(
        selected.spec.document_sha256().unwrap(),
        selected.manifest.document_sha256
    );
    assert_eq!(
        RenderEncodingDecision::from_json(&serde_json::to_vec(&value).unwrap()).unwrap(),
        value
    );
}

#[test]
fn decision_binding_rejects_a_different_owner_document_or_committed_interval() {
    let value = fixture();
    let original = intent(&value);
    let mut changed = original.clone();
    changed.job_id = RequestId::new("other-job").unwrap();
    assert!(
        value
            .validate_for(&changed, &value.encoding_attempt_id)
            .is_err()
    );
    changed = original.clone();
    changed.document_sha256 = Sha256::new("0".repeat(64)).unwrap();
    assert!(
        value
            .validate_for(&changed, &value.encoding_attempt_id)
            .is_err()
    );
    changed = original.clone();
    changed.range = FrameRange::new(ProjectFrame(1), original.range.end()).unwrap();
    assert!(
        value
            .validate_for(&changed, &value.encoding_attempt_id)
            .is_err()
    );
    changed = original;
    changed.revision_id = RevisionId::new("different-revision").unwrap();
    assert!(
        value
            .validate_for(&changed, &value.encoding_attempt_id)
            .is_err()
    );
    assert!(
        value
            .validate_for(
                &intent(&value),
                &AttemptId::new("verifier-attempt").unwrap()
            )
            .is_err()
    );
}

#[test]
fn strict_decision_grammar_rejects_null_unknown_and_missing_fields() {
    let original: serde_json::Value = serde_json::from_slice(MEASURED).unwrap();
    for pointer in [
        "",
        "/output",
        "/runtime",
        "/probes/0",
        "/probes/1/result/report",
        "/probes/1/result/report/settings",
        "/probes/1/result/report/runtime/images/0/mapped",
        "/outcome",
    ] {
        let mut changed = original.clone();
        changed.pointer_mut(pointer).unwrap()["future_evidence"] = serde_json::Value::Null;
        assert!(
            RenderEncodingDecision::from_json(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{pointer}"
        );
    }
    for pointer in [
        "/runtime",
        "/probes/0/result/runtime",
        "/probes/1/result/report/settings",
        "/probes/1/result/report/content",
    ] {
        let mut changed = original.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        changed
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            RenderEncodingDecision::from_json(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{pointer}"
        );
    }
    for pointer in [
        "/schema_version",
        "/probes/1/result/report/schema_version",
        "/probes/1/result/report/verification/policy_version",
        "/probes/1/result/report/content/schema_version",
        "/probes/1/result/report/runtime/schema_version",
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = 99.into();
        assert!(
            RenderEncodingDecision::from_json(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut oversized = MEASURED.to_vec();
    oversized.resize(MAX_RENDER_DECISION_BYTES + 1, b' ');
    assert!(RenderEncodingDecision::from_json(&oversized).is_err());
}

#[test]
fn selected_decision_requires_the_ordered_typed_failure_and_runtime_binding() {
    let mut value = fixture();
    value.probes.swap(0, 1);
    assert!(value.validate().is_err());
    let mut value = fixture();
    value.probes[1].identity.attempt_id = value.probes[0].identity.attempt_id.clone();
    assert!(value.validate().is_err());
    let mut value = fixture();
    value.probes[1].ordinal = 9;
    assert!(value.validate().is_err());
    let mut value = fixture();
    value.outcome = RenderDecisionOutcome::Selected { probe_ordinal: 0 };
    assert!(value.validate().is_err());
    let mut value = fixture();
    value.runtime = None;
    assert!(value.validate().is_err());
    let mut value = fixture();
    if let RenderProbeOutcome::Rejected { failure, .. } = &mut value.probes[0].result {
        failure.kind = RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::Io);
    }
    assert!(value.validate().is_err());
    let mut value = fixture();
    if let RenderProbeOutcome::Rejected { runtime, .. } = &mut value.probes[0].result {
        *runtime = None;
    }
    assert!(value.validate().is_err());
    let mut value = fixture();
    selected_mut(&mut value).runtime.images[1].sha256 = Sha256::new("0".repeat(64)).unwrap();
    assert!(value.validate().is_err());
}

#[test]
fn saved_success_rejects_changed_controls_hashes_clocks_and_measured_events() {
    type Mutation = fn(&mut RenderProbeReport);
    let mutations: [Mutation; 13] = [
        |report| report.settings.video_bitrate += 1,
        |report| report.manifest.report.info.video_max_b_frames = 2,
        |report| report.manifest.report.faststart_read_closes = 0,
        |report| report.manifest.report.audio_eof = false,
        |report| report.verification.movie_sha256 = Sha256::new("0".repeat(64)).unwrap(),
        |report| report.verification.audio_edit_media_time = 0,
        |report| report.verification.fresh_gop_frames -= 1,
        |report| report.verification.maximum_b_run = 1,
        |report| report.content.markers[2].observed_samples[0] += 1,
        |report| report.content.markers[1].observed_peaks[1] = 0.8,
        |report| report.content.unexpected_audio_peak = f32::NAN,
        |report| report.content.worst_frame_mean_absolute_error_milli[0] = 1_501,
        |report| report.manifest.document_sha256 = Sha256::new("0".repeat(64)).unwrap(),
    ];
    for mutate in mutations {
        let mut value = fixture();
        mutate(selected_mut(&mut value));
        assert!(value.validate().is_err());
    }
}

#[test]
fn frozen_fallback_graph_never_uses_generic_failures_or_diagnostic_text() {
    let hardware = RenderEncoderChoice {
        mode: RenderEncoder::Hardware,
        b_frames: RenderBFrames::TargetTwo,
    };
    let none = RenderEncoderChoice {
        mode: RenderEncoder::Hardware,
        b_frames: RenderBFrames::None,
    };
    let software = RenderEncoderChoice {
        mode: RenderEncoder::Software,
        b_frames: RenderBFrames::TargetTwo,
    };
    assert_eq!(
        next_choice(
            &hardware,
            RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::VideoTimestampOrder),
            true
        ),
        Some(none.clone())
    );
    assert_eq!(
        next_choice(
            &hardware,
            RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::EncoderUnavailable),
            true
        ),
        Some(software.clone())
    );
    assert_eq!(
        next_choice(
            &none,
            RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::VideoTimestampOrder),
            true
        ),
        Some(software.clone())
    );
    assert_eq!(
        next_choice(
            &software,
            RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::VideoTimestampOrder),
            true
        ),
        Some(RenderEncoderChoice {
            mode: RenderEncoder::Software,
            b_frames: RenderBFrames::None
        })
    );
    for kind in [
        RenderEncodeFailureKind::Configuration,
        RenderEncodeFailureKind::Native,
        RenderEncodeFailureKind::Cancelled,
        RenderEncodeFailureKind::Deadline,
        RenderEncodeFailureKind::Io,
        RenderEncodeFailureKind::Capacity,
        RenderEncodeFailureKind::Evidence,
    ] {
        assert_eq!(
            next_choice(&hardware, RenderProbeFailureKind::Encoder(kind), true),
            None
        );
    }
    assert_eq!(
        next_choice(&hardware, RenderProbeFailureKind::Output, true),
        None
    );
    assert_eq!(
        next_choice(
            &software,
            RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::EncoderUnavailable),
            true
        ),
        None
    );
}

#[test]
fn aborted_final_runtime_change_remains_evidence_without_allowing_fallback() {
    let mut value = fixture();
    value.probes.pop();
    if let RenderProbeOutcome::Rejected {
        runtime: Some(runtime),
        ..
    } = &mut value.probes[0].result
    {
        runtime.images[0].sha256 = Sha256::new("0".repeat(64)).unwrap();
    }
    value.outcome = RenderDecisionOutcome::Aborted {
        failure: RenderAdmissionFailure {
            kind: RenderAdmissionFailureKind::WorkerFault,
            diagnostic: Diagnostic::new("runtime changed after rejection").unwrap(),
        },
    };
    value.validate().unwrap();
    assert!(!value.is_selected());
    assert!(!value.unresolved_cleanup());
    assert!(!value.cancelled());
    let mut restored_selection = fixture();
    restored_selection.probes[0] = value.probes[0].clone();
    assert!(restored_selection.validate().is_err());
}

#[test]
fn cancellation_without_runtime_preserves_absence_and_no_selected_evidence() {
    let mut value = fixture();
    value.probes.clear();
    value.runtime = None;
    value.outcome = RenderDecisionOutcome::Aborted {
        failure: RenderAdmissionFailure {
            kind: RenderAdmissionFailureKind::Cancelled,
            diagnostic: Diagnostic::new("cancelled before helper capture").unwrap(),
        },
    };
    value.validate().unwrap();
    assert!(value.cancelled());
    assert!(value.selected().is_none());
    assert!(!value.unresolved_cleanup());
    if let RenderDecisionOutcome::Aborted { failure } = &mut value.outcome {
        failure.kind = RenderAdmissionFailureKind::UnresolvedCleanup;
    }
    value.validate().unwrap();
    assert!(value.unresolved_cleanup());
    assert!(!value.cancelled());
}

#[test]
fn pinned_controls_recipe_and_project_origin_reject_silent_retiming() {
    let settings =
        RenderSdrSettings::automatic_sdr_v1([1920, 1080], [30_000, 1001], RenderBFrames::TargetTwo)
            .unwrap();
    assert_eq!(
        (
            settings.video_bitrate,
            settings.audio_bitrate,
            settings.gop_frames,
            settings.b_frames,
            settings.movie_timescale
        ),
        (8_000_000, 384_000, 15, 2, 240_000)
    );
    let high =
        RenderSdrSettings::automatic_sdr_v1([1920, 1080], [60_000, 1001], RenderBFrames::None)
            .unwrap();
    assert_eq!(
        (high.video_bitrate, high.gop_frames, high.movie_timescale),
        (12_000_000, 30, 240_000)
    );
    assert_eq!(
        RenderPictureContract::nearest_even_raster([319, 181]).unwrap(),
        [318, 180]
    );
    assert_eq!(
        RenderPictureContract::nearest_even_raster([1, 1]).unwrap(),
        [2, 2]
    );
    for rate in [[120, 1], [60_000, 2002], [0, 1], [1, 0], [u32::MAX, 1]] {
        assert!(
            RenderSdrSettings::automatic_sdr_v1([320, 180], rate, RenderBFrames::None).is_err()
        );
    }
    let mut value = fixture();
    value.output.range = FrameRange::new(ProjectFrame(1), ProjectFrame(2)).unwrap();
    value.output.frame_count = 1;
    value.output.terminal_pts = 1001;
    value.output.project_audio_start = AudioSample(1602);
    value.output.project_audio_end = AudioSample(3203);
    value.validate().unwrap();
    value.output.project_audio_end = AudioSample(3204);
    assert!(value.validate().is_err());
}
