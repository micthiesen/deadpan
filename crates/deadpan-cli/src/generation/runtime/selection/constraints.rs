//! Enforce captured controls before decoding and again at the launch boundary.

use deadpan_jobs::{ConditioningMode, GenerationOptions, HoldConstraints};
use deadpan_models::packs::{PackManifest, constraints::ImageAxis};

use super::{manifest_mode, pack_id, plan_for_manifest};

pub fn validate_controls_for_manifest(
    manifest: &PackManifest,
    mode: ConditioningMode,
    options: &GenerationOptions,
) -> Result<(), String> {
    if pack_id(manifest_mode(manifest)?) != pack_id(mode) {
        return Err("generation controls use another selected pack operation".into());
    }
    options
        .validate_resolved_conditioning(mode)
        .map_err(|error| error.to_string())?;
    let (motion, instructions) = match mode {
        ConditioningMode::Bridge => {
            let constraints = manifest
                .constraints
                .bridge
                .as_ref()
                .ok_or("selected pack has no Bridge controls")?;
            (
                &constraints.motion_amounts,
                constraints.maximum_instruction_bytes,
            )
        }
        ConditioningMode::ExtendFromLeft | ConditioningMode::ExtendFromRight => {
            let constraints = manifest
                .constraints
                .extension
                .as_ref()
                .ok_or("selected pack has no Extension controls")?;
            (
                &constraints.motion_amounts,
                constraints.maximum_instruction_bytes,
            )
        }
    };
    if !motion.contains(&options.motion) {
        return Err("selected pack does not support the requested motion control".into());
    }
    if options
        .instructions
        .as_ref()
        .is_some_and(|value| value.as_str().len() > instructions as usize)
    {
        return Err("instructions exceed the selected pack's byte limit".into());
    }
    Ok(())
}

pub fn validate_constraints_for_manifest(
    manifest: &PackManifest,
    constraints: &HoldConstraints,
) -> Result<(), String> {
    let mode = constraints.conditioning;
    validate_controls_for_manifest(
        manifest,
        mode,
        &GenerationOptions::from_constraints(constraints),
    )?;
    let video = &constraints.video;
    // Planning enforces the exact project-rate, output-frame and authored
    // duration limits independently of any candidate's requested plan.
    plan_for_manifest(manifest, mode, video.frames(), video.frame_rate())?;
    let (width, height) = match mode {
        ConditioningMode::Bridge => {
            let capability = manifest
                .constraints
                .bridge
                .as_ref()
                .ok_or("selected pack has no Bridge raster")?;
            (capability.width, capability.height)
        }
        ConditioningMode::ExtendFromLeft | ConditioningMode::ExtendFromRight => {
            let capability = manifest
                .constraints
                .extension
                .as_ref()
                .ok_or("selected pack has no Extension raster")?;
            (capability.width, capability.height)
        }
    };
    if !axis_admits(width, video.width()) || !axis_admits(height, video.height()) {
        return Err("requested raster differs from the selected pack's bounds".into());
    }
    Ok(())
}

fn axis_admits(axis: ImageAxis, value: u32) -> bool {
    value >= axis.minimum && value <= axis.maximum && value.is_multiple_of(axis.multiple)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{FrameDuration, FrameRate};
    use deadpan_jobs::{HoldInstructions, MotionAmount, VideoSpec};
    use deadpan_models::packs::approved_pack;

    #[test]
    fn narrower_controls_refuse_motion_and_count_instruction_bytes_for_each_operation() {
        for (id, mode) in [
            ("ltx-2.3-q4-bridge", ConditioningMode::Bridge),
            ("ltx-2.3-q4-extension", ConditioningMode::ExtendFromRight),
        ] {
            let mut manifest = approved_pack(id).unwrap();
            if let Some(c) = manifest.constraints.bridge.as_mut() {
                c.motion_amounts = vec![MotionAmount::Still];
                c.maximum_instruction_bytes = 4;
            }
            if let Some(c) = manifest.constraints.extension.as_mut() {
                c.motion_amounts = vec![MotionAmount::Still];
                c.maximum_instruction_bytes = 4;
            }
            let mut options = GenerationOptions {
                mode: mode.into(),
                instructions: Some(HoldInstructions::new("éé").unwrap()),
                ..Default::default()
            };
            assert!(validate_controls_for_manifest(&manifest, mode, &options).is_ok());
            options.instructions = Some(HoldInstructions::new("ééa").unwrap());
            assert!(
                validate_controls_for_manifest(&manifest, mode, &options)
                    .unwrap_err()
                    .contains("byte")
            );
            options.instructions = None;
            options.motion = MotionAmount::Moderate;
            assert!(
                validate_controls_for_manifest(&manifest, mode, &options)
                    .unwrap_err()
                    .contains("motion")
            );
            options.motion = MotionAmount::Still;
            options.mode = if mode == ConditioningMode::Bridge {
                ConditioningMode::ExtendFromLeft.into()
            } else {
                ConditioningMode::Bridge.into()
            };
            assert!(validate_controls_for_manifest(&manifest, mode, &options).is_err());
        }
    }

    #[test]
    fn launch_constraints_cannot_bypass_raster_duration_or_operation_limits() {
        let manifest = approved_pack("ltx-2.3-q4-extension").unwrap();
        let mut constraints = HoldConstraints {
            video: VideoSpec::new(
                FrameDuration::new(90).unwrap(),
                FrameRate::new(30, 1).unwrap(),
                768,
                320,
            )
            .unwrap(),
            conditioning: ConditioningMode::ExtendFromLeft,
            motion: MotionAmount::Still,
            instructions: None,
            region_target: None,
        };
        assert!(validate_constraints_for_manifest(&manifest, &constraints).is_ok());
        for (frames, width, rate) in [
            (91, 768, 30),
            (90, 832, 30),
            (90, 768, 121),
            (181, 768, 120),
        ] {
            constraints.video = VideoSpec::new(
                FrameDuration::new(frames).unwrap(),
                FrameRate::new(rate, 1).unwrap(),
                width,
                320,
            )
            .unwrap();
            assert!(validate_constraints_for_manifest(&manifest, &constraints).is_err());
        }
        constraints.conditioning = ConditioningMode::Bridge;
        assert!(validate_constraints_for_manifest(&manifest, &constraints).is_err());
    }
}
