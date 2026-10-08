use deadpan_core::{
    AssetId, ProjectId, SourceQualificationId, SourceSpan, SourceTimeBase, SourceTimestamp,
    TargetRegion,
};
use deadpan_jobs::{
    GenerationCaptureSpec, GenerationInputBinding, GenerationInputSupport, GenerationInputs,
    GenerationPictureIdentity, GenerationPlan, GenerationRegionIdentity, GenerationTarget,
    HoldInstructions, RelativeGenerationPicture,
};
use deadpan_models::CapturedRegionBoundary;

use super::*;

fn stamp(ticks: i64) -> SourceTimestamp {
    SourceTimestamp {
        ticks,
        time_base: SourceTimeBase::new(1, 24).unwrap(),
    }
}

fn prepared(position: ExactRatio, ordinal: i64, region: (u32, u32)) -> PreparedBoundary {
    let rgba = RgbaImage::from_pixel(
        4,
        2,
        image::Rgba([u8::try_from(ordinal).unwrap(), 90, 170, 255]),
    );
    let (raster, content_rect) = contain(Some(&rgba), region).unwrap();
    PreparedBoundary {
        png: encode(&raster).unwrap(),
        picture: BoundaryPicture::Original {
            clock: BoundaryClock::Definition {
                project_id: ProjectId::new("project").unwrap(),
                revision_id: RevisionId::new("revision").unwrap(),
                definition: NodeId::new("definition").unwrap(),
                position,
            },
            asset: AssetId::new("original").unwrap(),
            qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
            picture: DecodedBoundary {
                source_frame: SourceFrameId(u64::try_from(ordinal).unwrap()),
                pts: stamp(ordinal),
                stream: MeasuredStream {
                    codec: "ffv1".into(),
                    pixel_format: "rgb24".into(),
                    width: 4,
                    height: 2,
                    sample_aspect: [1, 1],
                    rotation_quarter_turns: 0,
                    decoded_sample_bits: 8,
                    color: MODEL_COLOR_SPACE,
                },
                model_input: ModelInputConversion::SrgbCodesUnchanged,
            },
        },
        content_rect,
    }
}

fn fixture(
    direction: ExtensionDirection,
    rate: FrameRate,
    opposite: bool,
) -> (ExtensionGenerationPlan, HoldConstraints, PreparedExtension) {
    let duration = FrameDuration::new(3).unwrap();
    let plan = development_plan(direction, duration, rate).unwrap();
    let step = ExactRatio::new(
        i128::from(rate.numerator()),
        i128::from(rate.denominator()) * 24,
    )
    .unwrap();
    let region = canvas_region([1080, 1920]);
    let context = (0..9)
        .map(|index| {
            prepared(
                ExactRatio::integer(20)
                    .checked_add(step.checked_mul(ExactRatio::integer(index)).unwrap())
                    .unwrap(),
                100 + index,
                region,
            )
        })
        .collect::<Vec<_>>();
    let anchor = &context[match direction {
        ExtensionDirection::FromLeft => 8,
        ExtensionDirection::FromRight => 0,
    }];
    let BoundaryClock::Definition { position, .. } = anchor.picture.clock() else {
        unreachable!()
    };
    let opposite = opposite.then(|| {
        prepared(
            match direction {
                ExtensionDirection::FromLeft => {
                    position.checked_add(ExactRatio::integer(4)).unwrap()
                }
                ExtensionDirection::FromRight => {
                    position.checked_sub(ExactRatio::integer(4)).unwrap()
                }
            },
            120,
            region,
        )
    });
    let constraints = HoldConstraints {
        video: VideoSpec::new(duration, rate, NATIVE_WIDTH, NATIVE_HEIGHT).unwrap(),
        conditioning: mode(direction),
        motion: MotionAmount::Still,
        instructions: None,
        region_target: None,
    };
    let continuity = fixture_continuity(&plan, &context, opposite.as_ref(), None);
    (
        plan,
        constraints,
        PreparedExtension {
            context,
            opposite,
            presentation: RasterRect::centered(region.0, region.1, [NATIVE_WIDTH, NATIVE_HEIGHT])
                .unwrap(),
            continuity,
        },
    )
}

fn fixture_continuity(
    plan: &ExtensionGenerationPlan,
    context: &[PreparedBoundary],
    opposite: Option<&PreparedBoundary>,
    target: Option<(&TargetId, &AttentionTarget)>,
) -> ExtensionContinuityEvidence {
    let anchor = match plan.direction() {
        ExtensionDirection::FromLeft => context.last().unwrap(),
        ExtensionDirection::FromRight => &context[0],
    };
    let BoundaryClock::Definition {
        position: anchor_position,
        ..
    } = anchor.picture.clock()
    else {
        unreachable!()
    };
    let identity = |ordinal| GenerationPictureIdentity::Original {
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        frame: SourceFrameId(ordinal),
    };
    let sample = |frame: &PreparedBoundary| {
        let BoundaryPicture::Original {
            clock: BoundaryClock::Definition { position, .. },
            picture,
            ..
        } = &frame.picture
        else {
            unreachable!()
        };
        RelativeGenerationPicture {
            position: position.checked_sub(*anchor_position).unwrap(),
            picture: identity(picture.source_frame.0),
        }
    };
    let samples = context.iter().map(sample).collect::<Vec<_>>();
    let binding = GenerationInputBinding {
        duration: plan.project_frames(),
        frame_rate: plan.project_frame_rate(),
        canvas: [1080, 1920],
        inputs: GenerationInputs::Extension {
            capture: GenerationCaptureSpec::from_plan(&GenerationPlan::Extension(plan.clone())),
            support: vec![GenerationInputSupport {
                start: samples[0].position,
                end_exclusive: samples[8].position,
                first: identity(100),
                last: identity(107),
            }],
            terminal: samples[8].clone(),
            samples,
            opposite: opposite.map(sample),
        },
        region: target.map(|(id, record)| GenerationRegionIdentity {
            id: id.clone(),
            record: Some(record.clone()),
        }),
    };
    // Synthetic measurement bytes for assembly tests. Decoder-backed capture
    // tests separately bind actual source pictures to the retained signatures.
    let signature =
        deadpan_analysis::PictureSignature::from_rgba(&[0; 32 * 18 * 4], 32, 18, 128).unwrap();
    let bytes = deadpan_analysis::encode_context_signatures(&vec![signature; 121]).unwrap();
    ExtensionContinuityEvidence::new(
        binding,
        (0..121).map(identity).collect(),
        vec![Some(121), Some(121)],
        artifact("inputs/continuity.bin", &bytes).unwrap(),
    )
    .unwrap()
}

fn control(cancelled: &AtomicBool) -> CaptureControl<'_> {
    CaptureControl {
        cancelled,
        deadline: Instant::now() + CAPTURE_TIMEOUT,
    }
}

#[test]
fn development_envelope_is_exact_at_integer_and_fractional_rates() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for (rate, maximum) in [
            (FrameRate::new(30, 1).unwrap(), 10),
            (FrameRate::new(30_000, 1001).unwrap(), 9),
            (FrameRate::new(24, 1).unwrap(), 8),
        ] {
            for frames in [1, maximum] {
                let plan =
                    development_plan(direction, FrameDuration::new(frames).unwrap(), rate).unwrap();
                assert_eq!(plan.project_frames().frames(), frames);
                assert_eq!(plan.project_frame_rate(), rate);
                assert_eq!(
                    (
                        plan.context_frame_count(),
                        plan.generated_frame_count(),
                        plan.native_frame_count()
                    ),
                    (9, 8, 17)
                );
                assert_eq!(plan.native_dimensions(), native_dimensions());
            }
            assert!(
                development_plan(direction, FrameDuration::new(maximum + 1).unwrap(), rate)
                    .unwrap_err()
                    .contains("1/3 second")
            );
        }
        assert!(
            development_plan(
                direction,
                FrameDuration::ZERO,
                FrameRate::new(30, 1).unwrap()
            )
            .is_err()
        );
        assert!(
            development_plan(
                direction,
                FrameDuration::new(1).unwrap(),
                FrameRate::new(1, 1).unwrap()
            )
            .is_err()
        );
    }
}

#[test]
fn development_project_rate_matches_worker_envelope_without_rounding() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let plan = development_plan(
            direction,
            FrameDuration::new(40).unwrap(),
            FrameRate::new(120, 1).unwrap(),
        )
        .unwrap();
        assert_eq!(plan.project_frame_rate(), FrameRate::new(120, 1).unwrap());
        assert_eq!(plan.project_frames().frames(), 40);
        for rate in [
            FrameRate::new(120_001, 1000).unwrap(),
            FrameRate::new(240, 1).unwrap(),
            FrameRate::new(999, 1000).unwrap(),
        ] {
            let error =
                development_plan(direction, FrameDuration::new(1).unwrap(), rate).unwrap_err();
            assert!(error.contains("1 through 120 fps"), "{rate:?}: {error}");
        }
        validate_project_rate(FrameRate::new(1, 1).unwrap()).unwrap();
    }
}

#[test]
fn assembly_preserves_chronological_pngs_definition_clocks_geometry_and_absence() {
    let cancelled = AtomicBool::new(false);
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for rate in [
            FrameRate::new(30, 1).unwrap(),
            FrameRate::new(30_000, 1001).unwrap(),
        ] {
            for present in [false, true] {
                let (plan, constraints, prepared) = fixture(direction, rate, present);
                let expected_clocks = prepared
                    .context
                    .iter()
                    .map(|frame| frame.picture.clock().clone())
                    .collect::<Vec<_>>();
                let presentation = prepared.presentation;
                let content = prepared.context[0].content_rect.unwrap();
                let result =
                    assemble_captured(&plan, &constraints, prepared, None, &control(&cancelled))
                        .unwrap();
                let manifest: ExtensionContext = serde_json::from_slice(&result.manifest).unwrap();
                assert_eq!(manifest.plan(), &plan);
                assert_eq!(manifest.presentation(), presentation);
                assert_eq!(result.manifest_sha256, sha256(&result.manifest).unwrap());
                assert!(result.manifest.len() <= MAXIMUM_MANIFEST_BYTES);
                assert_eq!(result.context_pngs.len(), 9);
                for (index, (png, picture)) in result
                    .context_pngs
                    .iter()
                    .zip(manifest.context())
                    .enumerate()
                {
                    assert_eq!(picture.picture.clock(), &expected_clocks[index]);
                    assert_eq!(
                        picture.frame.reference().as_str(),
                        format!("inputs/context-{index:03}.png")
                    );
                    assert_eq!(picture.frame.sha256(), &sha256(png).unwrap());
                    assert_eq!(
                        picture.frame.byte_length(),
                        u64::try_from(png.len()).unwrap()
                    );
                    assert_eq!(picture.content, Some(content));
                    let decoded = image::load_from_memory(png).unwrap().to_rgb8();
                    assert_eq!(decoded.dimensions(), (768, 320));
                    assert_eq!(
                        decoded.get_pixel(384, 160).0,
                        [100 + u8::try_from(index).unwrap(), 90, 170]
                    );
                    assert_eq!(decoded.get_pixel(0, 160).0, [0, 0, 0]);
                }
                match (manifest.opposite(), &result.opposite_png) {
                    (ExtensionOppositeSeam::Absent, None) => assert!(!present),
                    (ExtensionOppositeSeam::PresentUnconditioned { frame, .. }, Some(png)) => {
                        assert!(present);
                        assert_eq!(frame.reference().as_str(), OPPOSITE);
                        assert_eq!(frame.sha256(), &sha256(png).unwrap());
                    }
                    _ => panic!("opposite seam presence differs from captured input"),
                }
            }
        }
    }
}

#[test]
fn selected_region_uses_direction_anchor_and_preserves_captured_controls() {
    let cancelled = AtomicBool::new(false);
    let target_id = TargetId::new("hand").unwrap();
    let target = AttentionTarget {
        label: "Hand".into(),
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(stamp(90), stamp(120)).unwrap(),
        region: TargetRegion {
            center: [500_000, 500_000],
            size: [500_000, 500_000],
        },
        samples: Vec::new(),
        corrections: Vec::new(),
        provenance: None,
    };
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let (plan, mut constraints, mut prepared) =
            fixture(direction, FrameRate::new(30, 1).unwrap(), true);
        prepared.continuity = fixture_continuity(
            &plan,
            &prepared.context,
            prepared.opposite.as_ref(),
            Some((&target_id, &target)),
        );
        let options = GenerationOptions {
            mode: mode(direction).into(),
            motion: MotionAmount::Subtle,
            instructions: Some(HoldInstructions::new("Keep the hand still.").unwrap()),
            region_target: GenerationTarget::Saved(target_id.clone()),
        };
        options.apply_to(&mut constraints);
        let before = constraints.clone();
        let result = assemble_captured(
            &plan,
            &constraints,
            prepared,
            Some((&target_id, &target)),
            &control(&cancelled),
        )
        .unwrap();
        assert_eq!(constraints, before);
        let context: ExtensionContext = serde_json::from_slice(&result.manifest).unwrap();
        let ExtensionRegionCapture::Selected { target, anchor, .. } = context.region() else {
            panic!("selected target retained")
        };
        assert_eq!(target, &target_id);
        let CapturedRegionBoundary::Available { point, .. } = anchor.as_ref() else {
            panic!("visible anchor region")
        };
        assert_eq!(
            point.ticks,
            ExactRatio::integer(match direction {
                ExtensionDirection::FromLeft => 108,
                ExtensionDirection::FromRight => 100,
            })
        );
    }
}

#[test]
fn assembly_rejects_wrong_scope_order_opposite_clock_and_controls() {
    let cancelled = AtomicBool::new(false);
    for defect in [
        "scope", "order", "seam", "count", "mode", "video", "region", "geometry",
    ] {
        let (plan, mut constraints, mut prepared) = fixture(
            ExtensionDirection::FromLeft,
            FrameRate::new(30, 1).unwrap(),
            true,
        );
        match defect {
            "scope" => {
                if let BoundaryPicture::Original {
                    clock: BoundaryClock::Definition { definition, .. },
                    ..
                } = &mut prepared.context[0].picture
                {
                    *definition = NodeId::new("outer-repeat").unwrap();
                }
            }
            "order" => prepared.context.swap(0, 1),
            "seam" => {
                prepared.opposite.as_mut().unwrap().picture = prepared.context[8].picture.clone()
            }
            "count" => {
                prepared.context.pop();
            }
            "mode" => constraints.conditioning = ConditioningMode::ExtendFromRight,
            "video" => {
                constraints.video = VideoSpec::new(
                    FrameDuration::new(4).unwrap(),
                    FrameRate::new(30, 1).unwrap(),
                    NATIVE_WIDTH,
                    NATIVE_HEIGHT,
                )
                .unwrap()
            }
            "region" => constraints.region_target = Some(TargetId::new("unsupplied").unwrap()),
            "geometry" => prepared.context[0].content_rect = None,
            _ => unreachable!(),
        }
        assert!(
            assemble_captured(&plan, &constraints, prepared, None, &control(&cancelled)).is_err(),
            "{defect}"
        );
    }
}

#[test]
fn aggregate_png_limit_includes_opposite_and_cancellation_deadline_win_before_open() {
    let cancelled = AtomicBool::new(false);
    let (plan, constraints, mut prepared) = fixture(
        ExtensionDirection::FromLeft,
        FrameRate::new(30, 1).unwrap(),
        true,
    );
    let existing = prepared
        .context
        .iter()
        .map(|frame| frame.png.len())
        .sum::<usize>();
    prepared.opposite.as_mut().unwrap().png =
        vec![0; usize::try_from(MAXIMUM_EXTENSION_INPUT_BYTES).unwrap() - existing + 1];
    assert!(
        assemble_captured(&plan, &constraints, prepared, None, &control(&cancelled))
            .err()
            .unwrap()
            .contains("aggregate 16 MiB")
    );
    let mut total = MAXIMUM_EXTENSION_INPUT_BYTES - 1;
    add_input_bytes(&mut total, 1).unwrap();
    assert_eq!(total, MAXIMUM_EXTENSION_INPUT_BYTES);
    assert!(add_input_bytes(&mut total, 1).is_err());
    assert!(
        CaptureControl {
            cancelled: &cancelled,
            deadline: Instant::now()
        }
        .check()
        .unwrap_err()
        .contains("deadline")
    );
    let error = prepare_extension_scoped_with_options(
        Path::new("/does-not-exist.deadpan"),
        &RevisionId::new("revision").unwrap(),
        &ScopedNodeTarget {
            node: NodeId::new("hold").unwrap(),
            repeats: Vec::new(),
        },
        ExtensionDirection::FromLeft,
        &GenerationOptions::default(),
        &AtomicBool::new(true),
    )
    .unwrap_err();
    assert!(error.contains("cancelled"), "{error}");
}
