use super::*;
use deadpan_encode::{BFramePolicy, EncoderMode};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, process::WorkerProtocol};
use protocol::{HostMessage, PROTOCOL_VERSION, ProbeProtocol};

fn spec() -> ProbeSpec {
    ProbeSpec {
        raster: [320, 180],
        frame_rate: [30_000, 1001],
        choice: EncoderChoice {
            mode: EncoderMode::Hardware,
            b_frames: BFramePolicy::TargetTwo,
        },
        color_policy: ColorPolicy::SdrRec709,
    }
}
fn request() -> HostMessage {
    let loaded = crate::encoded_render::runtime::test_fingerprint();
    HostMessage::Probe {
        protocol: PROTOCOL_VERSION,
        identity: RenderIdentity {
            request_id: RequestId::new("job").unwrap(),
            attempt_id: AttemptId::new("probe").unwrap(),
        },
        cancellation_token: CancellationToken::new("cancel").unwrap(),
        spec: spec(),
        limits: AdmissionLimits::default().encode,
        timeout_millis: 120_000,
        expected_runtime: AdmissionRuntime {
            helper_sha256: loaded.helper().sha256.clone(),
            helper_bytes: loaded.helper().mapped.file_size,
            system: loaded.system,
            kernel_release: loaded.kernel_release,
            kernel_build: loaded.kernel_build,
            machine: loaded.machine,
        },
    }
}

#[test]
fn probe_contract_has_its_own_exact_recipe_clock_and_identity() {
    let spec = spec();
    let contract = spec.contract().unwrap();
    let native = contract.native_contract().unwrap();
    assert_eq!(native.video_frames(), 46);
    assert_eq!(native.audio_samples(), 73_674);
    assert_eq!(contract.picture.range.start(), ProjectFrame(0));
    assert_eq!(contract.picture.project_audio_end, AudioSample(73_674));
    assert_eq!(contract.picture.terminal_pts, 46_046);
    assert_ne!(
        spec.document_sha256().unwrap(),
        ProbeSpec {
            frame_rate: [60, 1],
            ..spec.clone()
        }
        .document_sha256()
        .unwrap()
    );
    assert_eq!(
        spec.document_sha256().unwrap(),
        ProbeSpec {
            choice: EncoderChoice {
                mode: EncoderMode::Software,
                b_frames: BFramePolicy::None
            },
            ..spec
        }
        .document_sha256()
        .unwrap()
    );
}

#[test]
fn probe_rejects_ambiguous_picture_geometry_before_starting_a_worker() {
    for raster in [[2, 2], [12, 48], [14, 14], [15, 48], [0, 180]] {
        assert!(
            ProbeSpec { raster, ..spec() }.contract().is_err(),
            "{raster:?}"
        );
    }
    assert!(
        ProbeSpec {
            raster: [14, 16],
            ..spec()
        }
        .contract()
        .is_ok()
    );
    assert!(
        ProbeSpec {
            frame_rate: [1, 10],
            ..spec()
        }
        .contract()
        .is_err()
    );
}

#[test]
fn wire_admission_rejects_loose_fields_versions_and_work_before_execution() {
    use serde_json::json;
    let original = serde_json::to_value(request()).unwrap();
    for (path, value) in [
        ("/protocol", json!(0)),
        ("/protocol", json!(1)),
        ("/protocol", json!(3)),
        ("/timeout_millis", json!(0)),
        ("/timeout_millis", json!(120_001)),
        ("/spec/frame_rate", json!([120, 1])),
        ("/spec/raster", json!([2, 2])),
        ("/limits/maximum_output_bytes", json!(MAX_PROBE_BYTES + 1)),
        ("/expected_runtime/helper_bytes", json!(0)),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = value;
        let parsed = serde_json::from_value::<HostMessage>(changed);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "{path}"
        );
    }
    for path in ["", "/spec", "/spec/choice", "/limits", "/expected_runtime"] {
        let mut changed = original.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("trust_me".into(), json!(true));
        assert!(
            serde_json::from_value::<HostMessage>(changed).is_err(),
            "{path}"
        );
    }
    let protocol = ProbeProtocol::from_request(&request()).unwrap();
    let HostMessage::Probe { identity, .. } = request() else {
        unreachable!()
    };
    let valid = protocol::WorkerMessage::Progress {
        protocol: PROTOCOL_VERSION,
        identity: identity.clone(),
        completed_frames: 0,
        total_frames: 46,
    };
    assert!(protocol.classify(&valid).is_ok());
    for changed in [
        protocol::WorkerMessage::Progress {
            protocol: PROTOCOL_VERSION,
            identity: identity.clone(),
            completed_frames: 47,
            total_frames: 46,
        },
        protocol::WorkerMessage::Progress {
            protocol: PROTOCOL_VERSION,
            identity: RenderIdentity {
                attempt_id: AttemptId::new("stale").unwrap(),
                ..identity
            },
            completed_frames: 0,
            total_frames: 46,
        },
    ] {
        assert!(protocol.classify(&changed).is_err());
    }
}

#[test]
fn hdr_probe_specs_have_distinct_recipes_contracts_and_skip_sdr_color_bytes() {
    let sdr = spec();
    let wire = serde_json::to_string(&sdr).unwrap();
    assert!(!wire.contains("color_policy"), "{wire}");
    assert_eq!(serde_json::from_str::<ProbeSpec>(&wire).unwrap(), sdr);
    let pq = ProbeSpec {
        color_policy: ColorPolicy::HdrRec2020Pq,
        ..sdr.clone()
    };
    let hlg = ProbeSpec {
        color_policy: ColorPolicy::HdrRec2020Hlg,
        ..sdr.clone()
    };
    let recipes = [&sdr, &pq, &hlg].map(|spec| spec.document_sha256().unwrap());
    assert_ne!(recipes[0], recipes[1]);
    assert_ne!(recipes[0], recipes[2]);
    assert_ne!(recipes[1], recipes[2]);
    for (spec, mastering) in [(&pq, true), (&hlg, false)] {
        let contract = spec.contract().unwrap();
        assert_eq!(contract.picture.color_policy, spec.color_policy);
        assert_eq!(contract.picture.mastering_display.is_some(), mastering);
        let native = contract.native_contract().unwrap();
        assert!(native.video_format().is_hdr());
        // Same clocks as SDR; HDR bitrate is the frozen x1.25 policy.
        let sdr_native = sdr.contract().unwrap().native_contract().unwrap();
        assert_eq!(native.video_frames(), sdr_native.video_frames());
        assert_eq!(
            native.policy().video_bitrate,
            (sdr_native.policy().video_bitrate * 5 + 2) / 4
        );
        // The pure jobs mirror derives the identical recipe and contract.
        let mirror: deadpan_jobs::render::admission::RenderProbeSpec =
            serde_json::from_value(serde_json::to_value(spec).unwrap()).unwrap();
        assert_eq!(
            mirror.document_sha256().unwrap(),
            spec.document_sha256().unwrap()
        );
        assert_eq!(
            serde_json::to_value(mirror.contract().unwrap()).unwrap(),
            serde_json::to_value(&contract).unwrap()
        );
    }
    let mirror: deadpan_jobs::render::admission::RenderProbeSpec =
        serde_json::from_value(serde_json::to_value(&sdr).unwrap()).unwrap();
    assert_eq!(
        mirror.document_sha256().unwrap(),
        sdr.document_sha256().unwrap()
    );
    let encode = deadpan_encode::probe::HDR_PROBE_MASTERING;
    let jobs = deadpan_jobs::render::admission::HDR_PROBE_MASTERING;
    assert_eq!(
        (
            encode.primaries,
            encode.white_point,
            encode.max_luminance,
            encode.min_luminance
        ),
        (
            jobs.primaries,
            jobs.white_point,
            jobs.max_luminance,
            jobs.min_luminance
        )
    );
}

#[test]
fn pq_probe_declares_its_exact_per_pixel_input_light() {
    let pq = ProbeSpec {
        color_policy: ColorPolicy::HdrRec2020Pq,
        ..spec()
    };
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let light = content::declared_light(&pq.generator().unwrap(), &cancelled, deadline)
        .unwrap()
        .unwrap();
    // The 1000 cd/m² PQ code 723 is 1004.19 cd/m² exactly; frame means are
    // dominated by the black-to-1000 cd/m² ramp.
    assert_eq!(light.max_cll, 1005);
    assert!((150..=200).contains(&light.max_fall), "{light:?}");
    for color in [ColorPolicy::SdrRec709, ColorPolicy::HdrRec2020Hlg] {
        let other = ProbeSpec {
            color_policy: color,
            ..spec()
        };
        assert!(
            content::declared_light(&other.generator().unwrap(), &cancelled, deadline)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn adversarial_admission_probe_frames() {
    let initial = request();
    let cancel = ProbeProtocol::from_request(&initial)
        .unwrap()
        .cancellation();
    let HostMessage::Probe { identity, spec, .. } = &initial else {
        unreachable!()
    };
    let responses = vec![
        protocol::WorkerMessage::Progress {
            protocol: PROTOCOL_VERSION,
            identity: identity.clone(),
            completed_frames: 0,
            total_frames: spec.contract().unwrap().picture.frame_count,
        },
        protocol::WorkerMessage::Cancelled {
            protocol: PROTOCOL_VERSION,
            identity: identity.clone(),
        },
    ];
    crate::adversarial::protocol::<ProbeProtocol>(
        "cli-admission-probe-protocol",
        vec![initial, cancel],
        responses,
        |reader| protocol::read_host_message(reader),
    );
}
