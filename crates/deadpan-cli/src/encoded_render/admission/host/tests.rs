use super::*;
use crate::encoded_render::protocol::EncodedFailure;
use deadpan_jobs::Diagnostic;

fn choice(mode: EncoderMode, b_frames: BFramePolicy) -> EncoderChoice {
    EncoderChoice { mode, b_frames }
}
fn reported(kind: EncodeFailureKind) -> EncodedRenderError {
    EncodedRenderError::WorkerFailure(EncodedFailure {
        kind: EncodedFailureKind::Encoder(kind),
        diagnostic: Diagnostic::new("opaque native detail").unwrap(),
    })
}

#[test]
fn selection_is_ordered_and_uses_only_exact_admitted_native_failures() {
    let hardware_b = choice(EncoderMode::Hardware, BFramePolicy::TargetTwo);
    let hardware_none = choice(EncoderMode::Hardware, BFramePolicy::None);
    let software_b = choice(EncoderMode::Software, BFramePolicy::TargetTwo);
    let software_none = choice(EncoderMode::Software, BFramePolicy::None);
    for (before, kind, after) in [
        (
            hardware_b,
            EncodeFailureKind::VideoTimestampOrder,
            Some(hardware_none),
        ),
        (
            hardware_none,
            EncodeFailureKind::VideoTimestampOrder,
            Some(software_b),
        ),
        (
            software_b,
            EncodeFailureKind::VideoTimestampOrder,
            Some(software_none),
        ),
        (software_none, EncodeFailureKind::VideoTimestampOrder, None),
        (
            hardware_b,
            EncodeFailureKind::EncoderUnavailable,
            Some(software_b),
        ),
        (
            hardware_none,
            EncodeFailureKind::EncoderUnavailable,
            Some(software_b),
        ),
        (software_b, EncodeFailureKind::EncoderUnavailable, None),
    ] {
        assert_eq!(next_choice(before, &reported(kind), true), after);
    }
    assert_eq!(
        next_choice(
            hardware_none,
            &reported(EncodeFailureKind::EncoderUnavailable),
            false
        ),
        Some(software_none)
    );
}

#[test]
fn every_noncapability_or_invalidated_failure_stops_selection() {
    let current = choice(EncoderMode::Hardware, BFramePolicy::TargetTwo);
    for kind in [
        EncodeFailureKind::Configuration,
        EncodeFailureKind::Input,
        EncodeFailureKind::Poisoned,
        EncodeFailureKind::Cancelled,
        EncodeFailureKind::Deadline,
        EncodeFailureKind::Capacity,
        EncodeFailureKind::Io,
        EncodeFailureKind::Evidence,
        EncodeFailureKind::Native,
    ] {
        assert_eq!(
            next_choice(current, &reported(kind), true),
            None,
            "{kind:?}"
        );
    }
    for stage in [
        EncodedFailureKind::Control,
        EncodedFailureKind::Contract,
        EncodedFailureKind::Source,
        EncodedFailureKind::Picture,
        EncodedFailureKind::Audio,
        EncodedFailureKind::Output,
    ] {
        let error = EncodedRenderError::WorkerFailure(EncodedFailure {
            kind: stage,
            diagnostic: Diagnostic::new("video_timestamp_order: encoder unavailable").unwrap(),
        });
        assert_eq!(next_choice(current, &error, true), None);
    }
    for error in [
        EncodedRenderError::Cancelled,
        EncodedRenderError::Deadline,
        EncodedRenderError::Worker("video_timestamp_order".into()),
        EncodedRenderError::WorkerFault {
            primary: Box::new(reported(EncodeFailureKind::VideoTimestampOrder)),
            fault: "late protocol failure".into(),
        },
    ] {
        assert_eq!(next_choice(current, &error, true), None);
    }
}

#[cfg(unix)]
#[test]
fn revoking_queued_owner_cancels_and_drains_an_active_probe() {
    use std::{cell::Cell, collections::BTreeMap, os::unix::fs::PermissionsExt};
    let scratch = tempfile::tempdir().unwrap();
    let helper = scratch.path().join("probe.py");
    std::fs::write(
        &helper,
        r#"#!/usr/bin/env python3
import json, pathlib, struct, sys
def read():
    size = struct.unpack('>I', sys.stdin.buffer.read(4))[0]
    assert 0 < size <= 262144
    return json.loads(sys.stdin.buffer.read(size))
def emit(value):
    body = json.dumps(value).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(body)) + body)
    sys.stdout.buffer.flush()
request = read()
assert request['op'] == 'probe'
emit({'event': 'progress', 'protocol': 2, 'identity': request['identity'],
      'completed_frames': 0, 'total_frames': 46})
cancel = read()
assert cancel['op'] == 'cancel'
assert cancel['identity'] == request['identity']
assert cancel['cancellation_token'] == request['cancellation_token']
pathlib.Path(__file__).with_suffix('.cancelled').write_text('acknowledged')
emit({'event': 'cancelled', 'protocol': 2, 'identity': request['identity']})
"#,
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let revoked = Cell::new(false);
    let result = qualify_guarded(
        &RenderWorkerRuntime {
            executable: helper.clone(),
            arguments: Vec::new(),
            environment: BTreeMap::new(),
        },
        AdmissionRequest {
            identity: RenderIdentity {
                request_id: deadpan_jobs::RequestId::new("guarded-job").unwrap(),
                attempt_id: deadpan_jobs::AttemptId::new("guarded-attempt").unwrap(),
            },
            cancellation_token: CancellationToken::new("guarded-cancel").unwrap(),
            raster: [320, 180],
            frame_rate: [30000, 1001],
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        AdmissionLimits::default(),
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(15),
        |_, _, _| revoked.set(true),
        || {
            if revoked.get() {
                Err(EncodedRenderError::Configuration("queued owner revoked"))
            } else {
                Ok(())
            }
        },
    );
    let failure = match result {
        Ok(_) => panic!("revoked owner admitted an encoder"),
        Err(failure) => failure,
    };
    assert!(revoked.get());
    assert!(failure.error.to_string().contains("queued owner revoked"));
    assert!(failure.error.cleanup_confirmed());
    assert!(
        failure.rejected.is_empty(),
        "ownership failure is not capability evidence"
    );
    assert_eq!(
        std::fs::read_to_string(helper.with_extension("cancelled")).unwrap(),
        "acknowledged"
    );
}
