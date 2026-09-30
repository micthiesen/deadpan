use deadpan_encode::{EncodeError, EncodeFailureKind};

fn native(code: &str, message: &str) -> EncodeError {
    EncodeError::Native {
        code: code.into(),
        message: message.into(),
    }
}

#[test]
fn only_exact_video_capability_codes_receive_specific_classification() {
    for (code, expected) in [
        (
            "video_encoder_unavailable",
            EncodeFailureKind::EncoderUnavailable,
        ),
        (
            "video_timestamp_order",
            EncodeFailureKind::VideoTimestampOrder,
        ),
        ("audio_encoder_unavailable", EncodeFailureKind::Native),
        ("encoder_unavailable", EncodeFailureKind::Native),
        ("encoder_failure", EncodeFailureKind::Native),
        ("encoder_unsupported", EncodeFailureKind::Evidence),
        ("invalid_packet", EncodeFailureKind::Evidence),
        ("future_encoder_error", EncodeFailureKind::Native),
    ] {
        assert_eq!(
            native(code, "retained native detail").kind(),
            expected,
            "{code}"
        );
    }
}

#[test]
fn diagnostic_prose_never_promotes_an_ordinary_failure_to_capability_evidence() {
    for message in [
        "open selected encoder: Function not implemented",
        "open selected encoder: Invalid argument",
        "VideoToolbox H264 encoder unavailable",
        "video_encoder_unavailable",
        "pts (1001) < dts (2002)",
        "video_timestamp_order",
    ] {
        let error = native("encoder_failure", message);
        assert_eq!(error.kind(), EncodeFailureKind::Native);
        assert_eq!(
            error.to_string(),
            format!("native encoder encoder_failure: {message}")
        );
    }
}

#[test]
fn capacity_io_input_and_control_failures_keep_distinct_noncapability_kinds() {
    for (code, expected) in [
        ("packet_limit", EncodeFailureKind::Capacity),
        ("output_too_large", EncodeFailureKind::Capacity),
        ("allocation_failure", EncodeFailureKind::Capacity),
        ("output_io", EncodeFailureKind::Io),
        ("output_seek", EncodeFailureKind::Io),
        ("invalid_descriptor", EncodeFailureKind::Configuration),
        ("invalid_pcm", EncodeFailureKind::Input),
        ("incomplete_input", EncodeFailureKind::Input),
        ("cancelled", EncodeFailureKind::Cancelled),
        ("deadline_exceeded", EncodeFailureKind::Deadline),
        ("runtime_mismatch", EncodeFailureKind::Native),
        ("external_io_denied", EncodeFailureKind::Native),
        ("internal_error", EncodeFailureKind::Native),
    ] {
        assert_eq!(
            native(code, "diagnostic retained separately").kind(),
            expected,
            "{code}"
        );
    }
    assert_eq!(
        EncodeError::Configuration("invalid frame rate").kind(),
        EncodeFailureKind::Configuration
    );
    assert_eq!(
        EncodeError::Input("wrong ordinal").kind(),
        EncodeFailureKind::Input
    );
    assert_eq!(EncodeError::Poisoned.kind(), EncodeFailureKind::Poisoned);
    assert_eq!(EncodeError::Cancelled.kind(), EncodeFailureKind::Cancelled);
    assert_eq!(EncodeError::Deadline.kind(), EncodeFailureKind::Deadline);
    assert_eq!(
        EncodeError::Evidence("changed native contract").kind(),
        EncodeFailureKind::Evidence
    );
    assert_eq!(
        EncodeError::Io(std::io::Error::other("disk failure")).kind(),
        EncodeFailureKind::Io
    );
}

#[test]
fn serialized_failure_kind_has_a_closed_vocabulary() {
    for (kind, json) in [
        (
            EncodeFailureKind::EncoderUnavailable,
            "\"encoder_unavailable\"",
        ),
        (
            EncodeFailureKind::VideoTimestampOrder,
            "\"video_timestamp_order\"",
        ),
        (EncodeFailureKind::Capacity, "\"capacity\""),
        (EncodeFailureKind::Native, "\"native\""),
    ] {
        assert_eq!(serde_json::to_string(&kind).unwrap(), json);
        assert_eq!(
            serde_json::from_str::<EncodeFailureKind>(json).unwrap(),
            kind
        );
    }
    for json in [
        "\"auto_fallback\"",
        "\"unknown\"",
        "\"video_timestamp_order: pts < dts\"",
        "{\"video_timestamp_order\":\"extra evidence\"}",
        "{\"kind\":\"encoder_unavailable\",\"allowed\":true}",
    ] {
        assert!(
            serde_json::from_str::<EncodeFailureKind>(json).is_err(),
            "{json}"
        );
    }
}
