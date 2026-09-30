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
