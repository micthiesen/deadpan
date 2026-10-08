use super::*;
use deadpan_models::packs::approved_pack;

#[test]
fn extension_selection_uses_manifest_duration_and_exact_project_grid() {
    let pack = approved_pack(EXTENSION_PACK).unwrap();
    for mode in [
        ConditioningMode::ExtendFromLeft,
        ConditioningMode::ExtendFromRight,
    ] {
        for (frames, generated) in [(1, 8), (15, 8), (16, 16), (30, 24), (60, 48), (90, 72)] {
            let plan = plan_for_manifest(
                &pack,
                mode,
                FrameDuration::new(frames).unwrap(),
                FrameRate::new(30, 1).unwrap(),
            )
            .unwrap();
            let GenerationPlan::Extension(extension) = &plan else {
                panic!("wrong operation")
            };
            assert_eq!(extension.context_frame_count(), 9);
            assert_eq!(extension.generated_frame_count(), generated);
            assert_eq!(extension.native_frame_count(), 9 + generated);
            assert_eq!(plan.conditioning(), mode);
            let selected = selected_provider_for_manifest(&pack, &plan, 17).unwrap();
            assert!(matches!(selected, SelectedGenerationProvider::Extension(_)));
            assert_eq!(selected.selection().pack_id.as_str(), EXTENSION_PACK);
            assert_eq!(
                selected.selection().runtime_version.as_str(),
                "0.15.8+deadpan-extension1"
            );
            assert_eq!(selected.selection().seed, 17);
        }
        for (rate, maximum) in [
            (FrameRate::new(30, 1).unwrap(), 90),
            (FrameRate::new(30_000, 1001).unwrap(), 89),
            (FrameRate::new(120, 1).unwrap(), 180),
        ] {
            assert!(
                plan_for_manifest(&pack, mode, FrameDuration::new(maximum).unwrap(), rate).is_ok()
            );
            assert!(
                plan_for_manifest(&pack, mode, FrameDuration::new(maximum + 1).unwrap(), rate)
                    .is_err()
            );
        }
    }
}

#[test]
fn provider_selection_never_grants_another_operation_or_a_wider_capability() {
    let bridge = approved_pack(BRIDGE_PACK).unwrap();
    let extension = approved_pack(EXTENSION_PACK).unwrap();
    let rate = FrameRate::new(30, 1).unwrap();
    let duration = FrameDuration::new(60).unwrap();
    let bridge_plan = plan_for_manifest(&bridge, ConditioningMode::Bridge, duration, rate).unwrap();
    let extension_plan = plan_for_manifest(
        &extension,
        ConditioningMode::ExtendFromRight,
        duration,
        rate,
    )
    .unwrap();
    assert!(matches!(
        selected_provider_for_manifest(&bridge, &bridge_plan, 7).unwrap(),
        SelectedGenerationProvider::Bridge(_)
    ));
    assert!(selected_provider_for_manifest(&bridge, &extension_plan, 7).is_err());
    assert!(selected_provider_for_manifest(&extension, &bridge_plan, 7).is_err());
    assert!(plan_for_manifest(&bridge, ConditioningMode::ExtendFromLeft, duration, rate).is_err());
    assert!(plan_for_manifest(&extension, ConditioningMode::Bridge, duration, rate).is_err());

    let mut narrower = extension.clone();
    narrower
        .constraints
        .extension
        .as_mut()
        .unwrap()
        .maximum_project_frames = 30;
    assert!(selected_provider_for_manifest(&narrower, &extension_plan, 7).is_err());
    let mut wrong_runtime = extension;
    wrong_runtime.runtime_versions = vec!["0.15.8+deadpan-extension-dev2".into()];
    assert!(
        plan_for_manifest(
            &wrong_runtime,
            ConditioningMode::ExtendFromRight,
            duration,
            rate
        )
        .is_err()
    );
}
