use deadpan_core::{ExactRatio, ExtensionDirection, FrameDuration};
use deadpan_jobs::NativeDimensions;

use super::*;
use crate::packs::{PackManifest, approved_pack, approved_packs};

pub(in crate::packs) fn extension_pack() -> PackManifest {
    let mut pack = approved_pack("ltx-2.3-q4-bridge").unwrap();
    pack.pack_id = "test-extension-only".into();
    pack.operations = vec![Operation::ExtensionHold];
    let bridge = pack.constraints.bridge.take().unwrap();
    pack.constraints.conditioning = vec![
        Conditioning::ChronologicalVideo,
        Conditioning::FixedPrompt,
        Conditioning::UserInstructions,
    ];
    pack.constraints.extension = Some(ExtensionConstraints {
        native_frame_rate: FrameRate::new(24, 1).unwrap(),
        context_frame_count: 9,
        generated_frame_counts: FrameCounts {
            step: 8,
            offset: 0,
            minimum: 24,
            maximum: 24,
        },
        width: bridge.width,
        height: bridge.height,
        maximum_project_frames: 180,
        maximum_requested_duration: ExactRatio::ONE,
        motion_amounts: bridge.motion_amounts,
        maximum_instruction_bytes: bridge.maximum_instruction_bytes,
    });
    pack
}

#[test]
fn extension_round_trip_declares_temporal_conditioning_without_bridge_authority() {
    let pack = extension_pack();
    pack.validate().unwrap();
    let wire = serde_json::to_value(&pack).unwrap();
    assert_eq!(wire["operations"], serde_json::json!(["extension_hold"]));
    assert_eq!(
        wire["constraints"]["conditioning"],
        serde_json::json!(["chronological_video", "fixed_prompt", "user_instructions"])
    );
    assert_eq!(serde_json::from_value::<PackManifest>(wire).unwrap(), pack);
    assert!(!pack.supports(Operation::BridgeHold));
    assert!(pack.constraints.bridge.is_none());
    assert!(approved_pack(&pack.pack_id).is_none());
    assert!(
        approved_packs()
            .iter()
            .filter(|pack| pack.supports(Operation::ExtensionHold))
            .all(|pack| pack.pack_id == "ltx-2.3-q4-extension")
    );
}

#[test]
fn extension_duration_is_floored_on_each_project_grid_without_clamping_requests() {
    let pack = extension_pack();
    let constraints = pack.constraints.extension.unwrap();
    let raster = NativeDimensions::new(768, 320).unwrap();
    for (rate, maximum) in [
        (FrameRate::new(30, 1).unwrap(), 30),
        (FrameRate::new(30_000, 1001).unwrap(), 29),
        (FrameRate::new(24, 1).unwrap(), 24),
        (FrameRate::new(120, 1).unwrap(), 120),
    ] {
        let capability = constraints.capability(rate).unwrap();
        assert_eq!(capability.maximum_output_frames().frames(), maximum);
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            let plan = constraints
                .plan(
                    direction,
                    FrameDuration::new(maximum).unwrap(),
                    rate,
                    raster,
                )
                .unwrap();
            assert_eq!(plan.project_frames().frames(), maximum);
            assert_eq!(plan.context_frame_count(), 9);
            assert_eq!(plan.generated_frame_count(), 24);
            assert_eq!(plan.native_frame_count(), 33);
            assert!(
                constraints
                    .plan(
                        direction,
                        FrameDuration::new(maximum + 1).unwrap(),
                        rate,
                        raster
                    )
                    .is_err()
            );
        }
    }
    let mut capped = constraints.clone();
    capped.maximum_project_frames = 12;
    assert_eq!(
        capped
            .capability(FrameRate::new(30, 1).unwrap())
            .unwrap()
            .maximum_output_frames()
            .frames(),
        12
    );
    let mut half_second = constraints.clone();
    half_second.maximum_requested_duration = ExactRatio::new(1, 2).unwrap();
    assert_eq!(
        half_second
            .capability(FrameRate::new(30_000, 1001).unwrap())
            .unwrap()
            .maximum_output_frames()
            .frames(),
        14
    );
    assert!(
        half_second
            .capability(FrameRate::new(1, 1).unwrap())
            .is_err()
    );
    assert!(
        constraints
            .capability(FrameRate::new(120_001, 1000).unwrap())
            .is_err()
    );
}

#[test]
fn extension_rejects_missing_cross_operation_and_false_endpoint_conditioning() {
    for mutate in [
        |pack: &mut PackManifest| pack.operations = vec![Operation::BridgeHold],
        |pack: &mut PackManifest| pack.operations.push(Operation::BridgeHold),
        |pack: &mut PackManifest| pack.constraints.extension = None,
        |pack: &mut PackManifest| {
            pack.constraints.bridge = approved_pack("ltx-2.3-q4-bridge")
                .unwrap()
                .constraints
                .bridge
        },
        |pack: &mut PackManifest| {
            pack.constraints.conditioning[0] = Conditioning::LeftBoundaryImage
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .conditioning
                .push(Conditioning::RightBoundaryImage)
        },
        |pack: &mut PackManifest| pack.constraints.conditioning.push(Conditioning::MonoAudio),
        |pack: &mut PackManifest| {
            pack.constraints.conditioning.pop();
        },
        |pack: &mut PackManifest| pack.constraints.hardware.accelerators = vec![Accelerator::Cpu],
    ] {
        let mut pack = extension_pack();
        mutate(&mut pack);
        assert!(pack.validate().is_err(), "{pack:?}");
    }
    // A pack may independently declare both operations; neither block grants
    // the other operation or conditioning inputs by implication.
    let mut combined = extension_pack();
    combined.operations.push(Operation::BridgeHold);
    combined.constraints.bridge = approved_pack("ltx-2.3-q4-bridge")
        .unwrap()
        .constraints
        .bridge;
    combined.constraints.conditioning.extend([
        Conditioning::LeftBoundaryImage,
        Conditioning::RightBoundaryImage,
    ]);
    combined.validate().unwrap();
}

#[test]
fn extension_rejects_invalid_counts_duration_rates_geometry_and_prompt_bounds() {
    for mutate in [
        |c: &mut ExtensionConstraints| c.context_frame_count = 0,
        |c: &mut ExtensionConstraints| c.context_frame_count = 2,
        |c: &mut ExtensionConstraints| c.context_frame_count = 65,
        |c: &mut ExtensionConstraints| c.generated_frame_counts.step = 0,
        |c: &mut ExtensionConstraints| c.generated_frame_counts.step = 4,
        |c: &mut ExtensionConstraints| c.generated_frame_counts.offset = 1,
        |c: &mut ExtensionConstraints| c.generated_frame_counts.minimum = 0,
        |c: &mut ExtensionConstraints| c.generated_frame_counts.minimum = 32,
        |c: &mut ExtensionConstraints| c.generated_frame_counts.maximum = 65_536,
        |c: &mut ExtensionConstraints| c.maximum_project_frames = 0,
        |c: &mut ExtensionConstraints| c.maximum_project_frames = 65_537,
        |c: &mut ExtensionConstraints| c.maximum_requested_duration = ExactRatio::ZERO,
        |c: &mut ExtensionConstraints| c.maximum_requested_duration = ExactRatio::integer(-1),
        |c: &mut ExtensionConstraints| c.maximum_requested_duration = ExactRatio::integer(6),
        |c: &mut ExtensionConstraints| {
            c.maximum_requested_duration = ExactRatio::new(1001, 1000).unwrap()
        },
        |c: &mut ExtensionConstraints| c.native_frame_rate = FrameRate::new(120_001, 1000).unwrap(),
        |c: &mut ExtensionConstraints| c.native_frame_rate = FrameRate::new(999, 1000).unwrap(),
        |c: &mut ExtensionConstraints| c.width.multiple = 0,
        |c: &mut ExtensionConstraints| c.height.maximum = 16_385,
        |c: &mut ExtensionConstraints| c.motion_amounts.clear(),
        |c: &mut ExtensionConstraints| {
            c.motion_amounts = vec![MotionAmount::Still, MotionAmount::Still]
        },
        |c: &mut ExtensionConstraints| c.maximum_instruction_bytes = 0,
        |c: &mut ExtensionConstraints| c.maximum_instruction_bytes = 513,
    ] {
        let mut pack = extension_pack();
        mutate(pack.constraints.extension.as_mut().unwrap());
        assert!(pack.validate().is_err(), "{pack:?}");
    }
}

#[test]
fn extension_fields_are_required_and_unknown_fields_never_imply_support() {
    for field in [
        "context_frame_count",
        "generated_frame_counts",
        "maximum_requested_duration",
        "motion_amounts",
        "maximum_instruction_bytes",
    ] {
        let mut wire = serde_json::to_value(extension_pack()).unwrap();
        wire["constraints"]["extension"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<PackManifest>(wire).is_err(),
            "{field}"
        );
    }
    let mut wire = serde_json::to_value(extension_pack()).unwrap();
    wire["constraints"]["extension"]["directions"] = serde_json::json!(["from_left"]);
    assert!(serde_json::from_value::<PackManifest>(wire).is_err());
    let mut wire = serde_json::to_value(extension_pack()).unwrap();
    wire["constraints"]["extension"]["maximum_requested_duration"]["denominator"] = "0".into();
    assert!(serde_json::from_value::<PackManifest>(wire).is_err());
}

#[test]
fn existing_approved_manifest_values_and_absent_extension_wire_stay_unchanged() {
    for text in [
        include_str!("../../../../../models/packs/whisper-base-en-2.json"),
        include_str!("../../../../../models/packs/ltx-2.3-q4-bridge-1.json"),
    ] {
        let original: serde_json::Value = serde_json::from_str(text).unwrap();
        let pack: PackManifest = serde_json::from_str(text).unwrap();
        pack.validate().unwrap();
        assert!(pack.constraints.extension.is_none());
        let serialized = serde_json::to_value(&pack).unwrap();
        assert_eq!(serialized, original);
        assert!(serialized["constraints"].get("extension").is_none());
    }
}
