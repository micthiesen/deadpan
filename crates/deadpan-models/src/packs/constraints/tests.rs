use super::*;
use crate::packs::{PackManifest, approved_pack};

fn bridge() -> PackManifest {
    approved_pack("ltx-2.3-q4-bridge").unwrap()
}

#[test]
fn declared_bridge_drives_the_qualified_count_and_raster_contract() {
    let pack = bridge();
    let bridge = pack.constraints.bridge.as_ref().unwrap();
    assert_eq!(
        bridge.motion_amounts,
        vec![
            deadpan_jobs::MotionAmount::Still,
            deadpan_jobs::MotionAmount::Subtle,
            deadpan_jobs::MotionAmount::Moderate,
        ]
    );
    assert_eq!(
        bridge.maximum_instruction_bytes as usize,
        deadpan_jobs::MAX_HOLD_INSTRUCTION_BYTES
    );
    assert!(
        pack.constraints
            .conditioning
            .contains(&Conditioning::UserInstructions)
    );
    let capability = bridge.capability().unwrap();
    let rate = deadpan_core::FrameRate::new(24, 1).unwrap();
    let raster = deadpan_jobs::NativeDimensions::new(768, 320).unwrap();
    let duration = deadpan_core::FrameDuration::new(24).unwrap();
    let plan =
        deadpan_jobs::BridgeGenerationPlan::new(duration, rate, &capability, raster).unwrap();
    assert_eq!(plan.native_frame_count(), 25);
    assert_eq!(capability.frame_counts().minimum(), 9);
    assert_eq!(capability.frame_counts().maximum(), 97);
    assert!(
        deadpan_jobs::BridgeGenerationPlan::new(
            duration,
            rate,
            &capability,
            deadpan_jobs::NativeDimensions::new(640, 320).unwrap(),
        )
        .is_err()
    );
    assert_eq!(bridge.maximum_project_frames, 180);
}

#[test]
fn missing_constraints_and_unknown_nested_fields_cannot_be_admitted() {
    let mut wire = serde_json::to_value(bridge()).unwrap();
    wire.as_object_mut().unwrap().remove("constraints");
    assert!(serde_json::from_value::<PackManifest>(wire).is_err());
    let mut wire = serde_json::to_value(bridge()).unwrap();
    wire["constraints"]["hardware"]["install_script"] = "run.sh".into();
    assert!(serde_json::from_value::<PackManifest>(wire).is_err());
    let mut old = bridge();
    old.schema = 3;
    assert!(old.validate().is_err());
}

#[test]
fn invalid_formulas_dimensions_and_incomplete_conditioning_fail_validation() {
    for mutate in [
        |pack: &mut PackManifest| pack.constraints.bridge.as_mut().unwrap().frame_counts.step = 0,
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .frame_counts
                .minimum = 1
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .frame_counts
                .maximum = u32::MAX
        },
        |pack: &mut PackManifest| pack.constraints.bridge.as_mut().unwrap().width.multiple = 0,
        |pack: &mut PackManifest| {
            pack.constraints.bridge.as_mut().unwrap().height.maximum = u32::MAX
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .maximum_project_frames = 0
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .motion_amounts
                .clear()
        },
        |pack: &mut PackManifest| {
            pack.constraints.bridge.as_mut().unwrap().motion_amounts = vec![
                deadpan_jobs::MotionAmount::Still,
                deadpan_jobs::MotionAmount::Still,
            ]
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .motion_amounts
                .push(deadpan_jobs::MotionAmount::Still)
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .motion_amounts
                .push(deadpan_jobs::MotionAmount::Subtle)
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .maximum_instruction_bytes = 0
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .maximum_instruction_bytes = 513
        },
        |pack: &mut PackManifest| {
            pack.constraints.conditioning.pop();
        },
        |pack: &mut PackManifest| pack.constraints.conditioning.push(Conditioning::MonoAudio),
        |pack: &mut PackManifest| pack.constraints.hardware.accelerators = vec![Accelerator::Cpu],
        |pack: &mut PackManifest| pack.constraints.weight_precisions.clear(),
        |pack: &mut PackManifest| {
            pack.constraints
                .weight_precisions
                .push(Precision::Quantized4)
        },
        |pack: &mut PackManifest| pack.constraints.hardware.minimum_macos.major = 0,
    ] {
        let mut pack = bridge();
        mutate(&mut pack);
        assert!(pack.validate().is_err(), "{pack:?}");
    }
}

#[test]
fn bridge_prompt_capabilities_are_required_manifest_fields() {
    let mut wire = serde_json::to_value(bridge()).unwrap();
    wire["constraints"]["bridge"]
        .as_object_mut()
        .unwrap()
        .remove("motion_amounts");
    assert!(serde_json::from_value::<PackManifest>(wire).is_err());

    let mut wire = serde_json::to_value(bridge()).unwrap();
    wire["constraints"]["bridge"]
        .as_object_mut()
        .unwrap()
        .remove("maximum_instruction_bytes");
    assert!(serde_json::from_value::<PackManifest>(wire).is_err());

    let mut wire = serde_json::to_value(bridge()).unwrap();
    wire["constraints"]["conditioning"] = serde_json::json!([
        "left_boundary_image",
        "right_boundary_image",
        "fixed_prompt"
    ]);
    let pack = serde_json::from_value::<PackManifest>(wire).unwrap();
    assert!(pack.validate().is_err());
}

#[test]
fn speech_constraints_keep_the_real_pcm_worker_bounds() {
    let mut pack = approved_pack("whisper-base-en").unwrap();
    let audio = pack.constraints.audio.unwrap();
    assert_eq!(audio.sample_rate, deadpan_jobs::transcription::SAMPLE_RATE);
    assert_eq!(
        audio.maximum_samples,
        deadpan_jobs::transcription::MAX_ANALYSIS_FRAMES
    );
    assert!(pack.constraints.bridge.is_none());
    pack.constraints.audio.as_mut().unwrap().channels = 2;
    assert!(pack.validate().is_err());
    pack.constraints.audio.as_mut().unwrap().channels = 1;
    pack.constraints.audio.as_mut().unwrap().maximum_samples += 1;
    assert!(pack.validate().is_err());
}
