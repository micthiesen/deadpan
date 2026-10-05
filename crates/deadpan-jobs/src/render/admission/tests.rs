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
            RenderAutomaticAlgorithm::AutomaticSdrV1,
            &hardware,
            RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::VideoTimestampOrder),
            true
        ),
        Some(none.clone())
    );
    assert_eq!(
        next_choice(
            RenderAutomaticAlgorithm::AutomaticSdrV1,
            &hardware,
            RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::EncoderUnavailable),
            true
        ),
        Some(software.clone())
    );
    assert_eq!(
        next_choice(
            RenderAutomaticAlgorithm::AutomaticSdrV1,
            &none,
            RenderProbeFailureKind::Encoder(RenderEncodeFailureKind::VideoTimestampOrder),
            true
        ),
        Some(software.clone())
    );
    assert_eq!(
        next_choice(
            RenderAutomaticAlgorithm::AutomaticSdrV1,
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
            next_choice(
                RenderAutomaticAlgorithm::AutomaticSdrV1,
                &hardware,
                RenderProbeFailureKind::Encoder(kind),
                true
            ),
            None
        );
    }
    assert_eq!(
        next_choice(
            RenderAutomaticAlgorithm::AutomaticSdrV1,
            &hardware,
            RenderProbeFailureKind::Output,
            true
        ),
        None
    );
    assert_eq!(
        next_choice(
            RenderAutomaticAlgorithm::AutomaticSdrV1,
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

fn sha256_hex(bytes: &[u8]) -> String {
    Hasher::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The retained SDR decision must keep its exact serialized bytes and probe
/// recipe identity after HDR fields were added (all optional and skipped).
#[test]
fn sdr_decision_bytes_and_probe_recipe_are_unchanged_by_hdr_support() {
    let value = fixture();
    // Captured before HDR support from this same fixture.
    assert_eq!(
        sha256_hex(&serde_json::to_vec(&value).unwrap()),
        "c9f3344d4ddca1c0d9dcacdcd26ff30656fd176ec113dad2adcad163e9585048"
    );
    let selected = value.selected().unwrap();
    assert_eq!(
        selected.spec.document_sha256().unwrap().as_str(),
        "87d0c71a8c941e10975b3b8148cd21c88c10567c96e84da6ec886d8322a849f0"
    );
    assert_eq!(
        serde_json::to_string(&selected.spec).unwrap(),
        r#"{"raster":[320,180],"frame_rate":[30000,1001],"choice":{"mode":"hardware","b_frames":"none"}}"#
    );
    assert!(
        !String::from_utf8(serde_json::to_vec(&value).unwrap())
            .unwrap()
            .contains("mastering_display")
    );
}

/// Derive a self-consistent synthetic HDR decision from the measured SDR one.
/// This is a grammar fixture, not measured HDR evidence.
fn hdr_fixture(color: ColorPolicy) -> RenderEncodingDecision {
    let mut value = fixture();
    value.algorithm = RenderAutomaticAlgorithm::AutomaticHdrV1;
    value.output.color_policy = color;
    value.output.mastering_display =
        (color == ColorPolicy::HdrRec2020Pq).then_some(HDR_PROBE_MASTERING);
    for probe in &mut value.probes {
        probe.spec.color_policy = color;
        if let RenderProbeOutcome::Succeeded { report } = &mut probe.result {
            report.spec.color_policy = color;
            let contract = report.spec.contract().unwrap();
            let recipe = report.spec.document_sha256().unwrap();
            let settings = report.spec.settings().unwrap();
            report.manifest.contract = contract.clone();
            report.verification.contract = contract;
            report.manifest.document_sha256 = recipe.clone();
            report.verification.document_sha256 = recipe;
            report.manifest.report.info.video_bitrate = settings.video_bitrate;
            report.manifest.report.info.video_profile = 2;
            report.settings = settings;
            report.content.schema_version = 2;
            report.verification.content_light =
                (color == ColorPolicy::HdrRec2020Pq).then_some(RenderContentLightEvidence {
                    declared_max_cll: 1005,
                    declared_max_fall: 167,
                    decoded_bound_max_cll_millinits: 1_003_401,
                    decoded_bound_max_fall_millinits: 165_461,
                });
        }
    }
    value
}

fn hdr_intent(value: &RenderEncodingDecision) -> RenderIntent {
    let mut intent = intent(value);
    intent.policy = RenderPolicy::Automatic(RenderAutomaticPolicy {
        schema_version: 1,
        selection: RenderAutomaticSelection::Automatic,
        algorithm: RenderAutomaticAlgorithm::AutomaticHdrV1,
    });
    intent
}

#[test]
fn hdr_decisions_validate_and_round_trip_for_pq_and_hlg() {
    for color in [ColorPolicy::HdrRec2020Pq, ColorPolicy::HdrRec2020Hlg] {
        let value = hdr_fixture(color);
        value
            .validate_for(&hdr_intent(&value), &value.encoding_attempt_id)
            .unwrap();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(RenderEncodingDecision::from_json(&bytes).unwrap(), value);
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(
            text.contains("mastering_display"),
            color == ColorPolicy::HdrRec2020Pq
        );
        let selected = value.selected().unwrap();
        let sdr = fixture();
        let sdr_selected = sdr.selected().unwrap();
        assert_eq!(
            selected.settings.video_bitrate,
            (sdr_selected.settings.video_bitrate * 5 + 2) / 4
        );
        assert_eq!(selected.manifest.contract.picture.color_policy, color);
        assert_ne!(
            selected.spec.document_sha256().unwrap(),
            sdr_selected.spec.document_sha256().unwrap()
        );
        // An SDR intent cannot own an HDR decision, nor vice versa.
        assert!(
            value
                .validate_for(&intent(&value), &value.encoding_attempt_id)
                .is_err()
        );
        assert!(
            sdr.validate_for(&hdr_intent(&sdr), &sdr.encoding_attempt_id)
                .is_err()
        );
    }
    let pq = hdr_fixture(ColorPolicy::HdrRec2020Pq)
        .selected()
        .unwrap()
        .spec
        .clone();
    let hlg = hdr_fixture(ColorPolicy::HdrRec2020Hlg)
        .selected()
        .unwrap()
        .spec
        .clone();
    assert_ne!(
        pq.document_sha256().unwrap(),
        hlg.document_sha256().unwrap()
    );
    assert!(
        serde_json::to_string(&pq)
            .unwrap()
            .contains(r#""color_policy":"hdr_rec2020_pq""#)
    );
}

#[test]
fn hdr_decision_rejects_mismatched_algorithm_color_metadata_and_evidence() {
    type Mutation = fn(&mut RenderEncodingDecision);
    let mutations: [Mutation; 9] = [
        // Algorithm and output color must agree.
        |value| value.algorithm = RenderAutomaticAlgorithm::AutomaticSdrV1,
        |value| value.output.color_policy = ColorPolicy::SdrRec709,
        // Mastering only with PQ, and only a valid volume.
        |value| value.output.mastering_display = Some(HDR_PROBE_MASTERING),
        |value| value.probes[0].spec.color_policy = ColorPolicy::SdrRec709,
        |value| selected_mut(value).manifest.report.info.video_profile = 100,
        |value| selected_mut(value).content.schema_version = 1,
        |value| selected_mut(value).settings.video_bitrate -= 1,
        |value| {
            let report = selected_mut(value);
            report.manifest.document_sha256 = RenderProbeSpec {
                color_policy: ColorPolicy::HdrRec2020Pq,
                ..report.spec.clone()
            }
            .document_sha256()
            .unwrap();
        },
        |value| selected_mut(value).content.maximum_plane_error[0] = 193,
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut value = hdr_fixture(ColorPolicy::HdrRec2020Hlg);
        mutate(&mut value);
        assert!(value.validate().is_err(), "mutation {index}");
    }
    let mut invalid = hdr_fixture(ColorPolicy::HdrRec2020Pq);
    invalid.output.mastering_display = Some(MasteringDisplay {
        max_luminance: 0,
        ..HDR_PROBE_MASTERING
    });
    assert!(invalid.validate().is_err());
    let mut missing_light = hdr_fixture(ColorPolicy::HdrRec2020Pq);
    selected_mut(&mut missing_light).verification.content_light = None;
    assert!(missing_light.validate().is_err());
    let mut hlg_light = hdr_fixture(ColorPolicy::HdrRec2020Hlg);
    selected_mut(&mut hlg_light).verification.content_light =
        hdr_fixture(ColorPolicy::HdrRec2020Pq)
            .selected()
            .unwrap()
            .verification
            .content_light;
    assert!(hlg_light.validate().is_err());
    // Sparse highlights: the decoded mean bound may exceed the decoded
    // percentile bound; only the declared pair must keep FALL <= CLL.
    let light_with = |declared: [u16; 2], decoded: [u32; 2]| {
        let mut value = hdr_fixture(ColorPolicy::HdrRec2020Pq);
        selected_mut(&mut value).verification.content_light = Some(RenderContentLightEvidence {
            declared_max_cll: declared[0],
            declared_max_fall: declared[1],
            decoded_bound_max_cll_millinits: decoded[0],
            decoded_bound_max_fall_millinits: decoded[1],
        });
        value.validate()
    };
    light_with([10_000, 25], [0, 2_470]).unwrap();
    assert!(light_with([25, 26], [0, 2_470]).is_err());
    assert!(light_with([10_000, 25], [0, 10_000_001]).is_err());
    assert!(light_with([10_001, 25], [0, 2_470]).is_err());
    let mut absent = hdr_fixture(ColorPolicy::HdrRec2020Pq);
    absent.output.mastering_display = None;
    absent.validate().unwrap();
}

#[test]
fn hdr_settings_are_the_frozen_sdr_controls_with_scaled_bitrate() {
    for (raster, rate) in [([1920, 1080], [30_000, 1001]), ([3840, 2160], [60, 1])] {
        let sdr =
            RenderSdrSettings::automatic_sdr_v1(raster, rate, RenderBFrames::TargetTwo).unwrap();
        let hdr =
            RenderSdrSettings::automatic_hdr_v1(raster, rate, RenderBFrames::TargetTwo).unwrap();
        assert_eq!(hdr.video_bitrate, (sdr.video_bitrate * 5 + 2) / 4);
        assert_eq!(
            RenderSdrSettings {
                video_bitrate: sdr.video_bitrate,
                ..hdr.clone()
            },
            sdr
        );
        assert_eq!(
            RenderSdrSettings::automatic(
                ColorPolicy::HdrRec2020Hlg,
                raster,
                rate,
                RenderBFrames::TargetTwo
            )
            .unwrap(),
            hdr
        );
    }
    assert_eq!(
        RenderSdrSettings::automatic_hdr_v1([1920, 1080], [30_000, 1001], RenderBFrames::None)
            .unwrap()
            .video_bitrate,
        10_000_000
    );
}
