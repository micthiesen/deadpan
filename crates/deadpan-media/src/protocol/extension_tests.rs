use super::*;
use deadpan_core::{BridgeInterpolation, ExtensionDirection, FrameDuration, FrameRate};
use serde_json::json;

pub(super) fn extension(direction: ExtensionDirection) -> ExtensionConversionRequest {
    ExtensionConversionRequest {
        protocol: EXTENSION_PROTOCOL_VERSION,
        operation: ExtensionOperation::SampleExtension,
        native: VideoContract {
            width: 4,
            height: 2,
            frames: 17,
            rate_num: 24,
            rate_den: 1,
        },
        sampling: ExtensionSamplingMap::new(
            direction,
            FrameRate::new(30_000, 1001).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            FrameDuration::new(9).unwrap(),
            FrameDuration::new(8).unwrap(),
            FrameDuration::new(30).unwrap(),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap(),
        input_byte_length: 1024,
        limits: ConversionLimits {
            max_input_bytes: 1024,
            max_output_bytes: 1024 * 1024,
            max_scratch_bytes: 4 * 2 * 3 * 17,
            timeout_ms: 5000,
        },
    }
}

#[test]
fn extension_request_retains_its_distinct_wire_and_native_scratch_budget() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let request = extension(direction);
        request.validate().unwrap();
        assert_eq!(request.output_video().unwrap().frames, 30);
        assert!(
            request.output_video().unwrap().scratch_bytes().unwrap()
                > request.limits.max_scratch_bytes
        );
        let worker = WorkerRequest::Extension(request.clone());
        let wire = serde_json::to_string(&worker).unwrap();
        assert!(wire.len() < MAX_REQUEST_BYTES);
        assert_eq!(
            serde_json::from_str::<WorkerRequest>(&wire).unwrap(),
            worker
        );
        assert_eq!(worker.native_video(), request.native);
        assert_eq!(worker.input_byte_length(), 1024);
        assert_eq!(
            worker.output_video().unwrap(),
            request.output_video().unwrap()
        );
        assert!(serde_json::from_str::<BridgeConversionRequest>(&wire).is_err());
        for extra in [
            "\"protocol\":3,",
            "\"operation\":\"sample_extension\",",
            "\"video\":{},",
            "\"path\":\"forbidden\",",
        ] {
            assert!(
                serde_json::from_str::<WorkerRequest>(&wire.replacen(
                    '{',
                    &format!("{{{extra}"),
                    1
                ))
                .is_err()
            );
        }
    }
}

#[test]
fn extension_conversion_leaves_provider_block_counts_to_the_planner() {
    let mut request = extension(ExtensionDirection::FromRight);
    request.sampling = ExtensionSamplingMap::new(
        ExtensionDirection::FromRight,
        FrameRate::new(24, 1).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        FrameDuration::new(10).unwrap(),
        FrameDuration::new(7).unwrap(),
        FrameDuration::new(3).unwrap(),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap();
    request.validate().unwrap();
    assert_eq!(request.sampling.generated_interval(), 0..7);
}

#[test]
fn extension_request_rejects_mismatched_protocol_counts_rates_and_budgets() {
    let request = extension(ExtensionDirection::FromLeft);
    let wire = serde_json::to_value(&request).unwrap();
    for (field, value) in [
        ("protocol", json!(BRIDGE_PROTOCOL_VERSION)),
        ("input_byte_length", json!(0)),
        ("input_byte_length", json!(1025)),
    ] {
        let mut changed = wire.clone();
        changed[field] = value;
        assert!(
            serde_json::from_value::<ExtensionConversionRequest>(changed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for (field, value) in [
        ("frames", 16),
        ("frames", 10_001),
        ("rate_num", 25),
        ("rate_den", 2),
        ("width", 4097),
    ] {
        let mut changed = wire.clone();
        changed["native"][field] = json!(value);
        assert!(
            serde_json::from_value::<ExtensionConversionRequest>(changed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for (field, value) in [
        ("context_frame_count", 8),
        ("generated_frame_count", 7),
        ("output_frame_count", 10_001),
    ] {
        let mut changed = wire.clone();
        changed["sampling"][field] = json!(value);
        assert!(
            serde_json::from_value::<ExtensionConversionRequest>(changed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut changed = request.clone();
    changed.limits.max_scratch_bytes -= 1;
    assert!(changed.validate().is_err());
    changed = request;
    changed.limits.timeout_ms = 0;
    assert!(changed.validate().is_err());
}

#[test]
fn extension_wire_rejects_ambiguous_policy_direction_and_intervals() {
    let wire = serde_json::to_value(extension(ExtensionDirection::FromRight)).unwrap();
    for (field, value) in [
        ("policy", json!("bridge_interior")),
        ("direction", json!("automatic")),
        ("schema_version", json!(2)),
        ("context_frame_count", json!(0)),
        ("generated_frame_count", json!(0)),
        ("output_frame_count", json!(0)),
        ("generated_start", json!(1)),
        ("generated_frame_count", json!(i64::MAX)),
    ] {
        let mut changed = wire.clone();
        changed["sampling"][field] = value;
        assert!(
            serde_json::from_value::<WorkerRequest>(changed).is_err(),
            "{field}"
        );
    }
    for operation in ["sample_bridge", "convert", "unknown"] {
        let mut changed = wire.clone();
        changed["operation"] = json!(operation);
        assert!(serde_json::from_value::<WorkerRequest>(changed).is_err());
    }
    let mut changed = wire;
    changed["sampling"]
        .as_object_mut()
        .unwrap()
        .remove("policy");
    assert!(serde_json::from_value::<WorkerRequest>(changed).is_err());
}
