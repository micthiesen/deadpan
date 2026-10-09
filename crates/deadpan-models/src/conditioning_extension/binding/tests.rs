use deadpan_core::{
    AssetId, FrameDuration, FrameRate, NodeId, SourceFrameId, SourceQualificationId, SourceSpan,
    SourceTimeBase, SourceTimestamp, TargetRegion,
};
use deadpan_jobs::{
    AxisLimits, DimensionLimits, ExtensionCapability, FrameCountFormula, GenerationRegionIdentity,
    NativeDimensions,
};

use super::*;

struct Fixture {
    plan: ExtensionGenerationPlan,
    binding: GenerationInputBinding,
    context: Vec<ExtensionContextPicture>,
    presentation: RasterRect,
    opposite: ExtensionOppositeSeam,
}

impl Fixture {
    fn new(direction: ExtensionDirection, count: u32) -> Self {
        let rate = FrameRate::new(30_000, 1001).unwrap();
        let plan = ExtensionGenerationPlan::new(
            direction,
            FrameDuration::new(3).unwrap(),
            rate,
            &ExtensionCapability::new(
                FrameRate::new(24, 1).unwrap(),
                count,
                FrameCountFormula::new(8, 0, 8, 8).unwrap(),
                DimensionLimits::new(
                    AxisLimits::new(512, 512, 1).unwrap(),
                    AxisLimits::new(320, 320, 1).unwrap(),
                ),
                FrameDuration::new(10).unwrap(),
            )
            .unwrap(),
            NativeDimensions::new(512, 320).unwrap(),
        )
        .unwrap();
        let step = ExactRatio::new(30_000, 1001 * 24).unwrap();
        let context: Vec<_> = (0..count)
            .map(|index| ExtensionContextPicture {
                picture: BoundaryPicture::AuthoredBlack {
                    clock: clock(
                        ExactRatio::integer(20)
                            .checked_add(
                                step.checked_mul(ExactRatio::integer(i64::from(index)))
                                    .unwrap(),
                            )
                            .unwrap(),
                    ),
                },
                frame: declaration(),
                content: None,
            })
            .collect();
        let anchor = match direction {
            ExtensionDirection::FromLeft => &context[context.len() - 1].picture,
            ExtensionDirection::FromRight => &context[0].picture,
        };
        let samples: Vec<_> = context
            .iter()
            .map(|item| RelativeGenerationPicture {
                position: clock_difference(anchor, &item.picture).unwrap(),
                picture: GenerationPictureIdentity::AuthoredBlack,
            })
            .collect();
        let terminal = samples.last().unwrap().clone();
        let support = if count == 1 {
            vec![]
        } else {
            vec![GenerationInputSupport {
                start: samples[0].position,
                end_exclusive: terminal.position,
                first: GenerationPictureIdentity::AuthoredBlack,
                last: GenerationPictureIdentity::AuthoredBlack,
            }]
        };
        let binding = GenerationInputBinding {
            duration: plan.project_frames(),
            frame_rate: rate,
            canvas: [1920, 1080],
            inputs: GenerationInputs::Extension {
                capture: GenerationCaptureSpec::Extension {
                    direction,
                    native_rate: plan.native_frame_rate(),
                    context_frames: count,
                    policy: ExtensionCapturePolicy::TemporalContextV1,
                },
                samples,
                opposite: None,
                support,
                terminal,
            },
            region: None,
        };
        Self {
            plan,
            binding,
            context,
            presentation: RasterRect::new(0, 16, 512, 288).unwrap(),
            opposite: ExtensionOppositeSeam::Absent,
        }
    }

    fn validate(&self) -> Result<(), QualificationError> {
        validate(
            &self.binding,
            &self.plan,
            &self.context,
            self.presentation,
            &self.opposite,
            &ExtensionRegionCapture::None,
        )
    }
}

fn clock(position: ExactRatio) -> BoundaryClock {
    BoundaryClock::Definition {
        project_id: ProjectId::new("project").unwrap(),
        revision_id: RevisionId::new("revision").unwrap(),
        definition: NodeId::new("sequence").unwrap(),
        position,
    }
}

fn declaration() -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new("inputs/frame.png").unwrap(),
        Sha256::new("a".repeat(64)).unwrap(),
        1,
    )
    .unwrap()
}

fn original(frame: u64) -> GenerationPictureIdentity {
    GenerationPictureIdentity::Original {
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        frame: SourceFrameId(frame),
    }
}

fn sampled_object(digest: &str) -> GeneratedObjectRef {
    GeneratedObjectRef::new(GeneratedContentId::new(digest.repeat(64)).unwrap(), 1).unwrap()
}

fn generated(frame: u64, aspect: Option<[u32; 2]>) -> GenerationPictureIdentity {
    GenerationPictureIdentity::Generated {
        sampled_object: sampled_object("b"),
        frame: SourceFrameId(frame),
        content_aspect: aspect,
    }
}

fn decoded() -> crate::DecodedBoundary {
    crate::DecodedBoundary {
        source_frame: SourceFrameId(7),
        pts: stamp(200),
        stream: crate::MeasuredStream {
            codec: "ffv1".into(),
            pixel_format: "rgb24".into(),
            width: 512,
            height: 320,
            clean_aperture: None,
            sample_aspect: [1, 1],
            rotation_quarter_turns: 0,
            decoded_sample_bits: 8,
            color: CANONICAL_BRIDGE_COLOR,
        },
        model_input: crate::ModelInputConversion::SrgbCodesUnchanged,
    }
}

fn original_boundary() -> BoundaryPicture {
    BoundaryPicture::Original {
        clock: clock(ExactRatio::integer(20)),
        asset: AssetId::new("original").unwrap(),
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        picture: decoded(),
    }
}

#[test]
fn exact_directional_coordinates_and_singleton_support_validate() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for count in [1, 9] {
            Fixture::new(direction, count).validate().unwrap();
        }
    }
}

#[test]
fn plan_capture_canvas_and_actual_relative_coordinates_cannot_drift() {
    let mut fixture = Fixture::new(ExtensionDirection::FromLeft, 9);
    fixture.binding.duration = FrameDuration::new(4).unwrap();
    assert!(fixture.validate().is_err());
    fixture.binding.duration = fixture.plan.project_frames();
    fixture.binding.frame_rate = FrameRate::new(30, 1).unwrap();
    assert!(fixture.validate().is_err());
    fixture.binding.frame_rate = fixture.plan.project_frame_rate();
    fixture.binding.canvas = [1080, 1920];
    assert!(fixture.validate().is_err());
    fixture.binding.canvas = [1920, 1080];
    fixture.presentation.x = 1;
    assert!(fixture.validate().is_err());
    fixture.presentation.x = 0;
    let GenerationInputs::Extension { capture, .. } = &mut fixture.binding.inputs else {
        unreachable!()
    };
    *capture = GenerationCaptureSpec::Bridge;
    assert!(fixture.validate().is_err());

    let mut fixture = Fixture::new(ExtensionDirection::FromRight, 9);
    let GenerationInputs::Extension { samples, .. } = &mut fixture.binding.inputs else {
        unreachable!()
    };
    samples[1].position = ExactRatio::ONE;
    assert!(fixture.validate().is_err());
    let mut fixture = Fixture::new(ExtensionDirection::FromLeft, 9);
    fixture.context[1].picture = BoundaryPicture::AuthoredBlack {
        clock: BoundaryClock::Project { frame: 21 },
    };
    assert!(fixture.validate().is_err());
}

#[test]
fn canvas_uses_half_up_size_and_floor_center_including_odd_canvases() {
    assert_eq!(
        canvas_presentation([3, 2], [4, 3]).unwrap(),
        RasterRect::new(0, 0, 4, 3).unwrap()
    );
    assert_eq!(
        canvas_presentation([2, 3], [4, 4]).unwrap(),
        RasterRect::new(0, 0, 3, 4).unwrap()
    );
    assert_eq!(
        canvas_presentation([1, 65_536], [512, 320]).unwrap(),
        RasterRect::new(255, 0, 1, 320).unwrap()
    );
    for canvas in [[0, 1], [1, 0], [65_537, 1], [1, 65_537]] {
        assert!(canvas_presentation(canvas, [512, 320]).is_err());
    }
}

#[test]
fn opposite_presence_identity_and_coordinate_must_agree() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let mut fixture = Fixture::new(direction, 1);
        let position = match direction {
            ExtensionDirection::FromLeft => ExactRatio::integer(24),
            ExtensionDirection::FromRight => ExactRatio::integer(16),
        };
        fixture.opposite = ExtensionOppositeSeam::PresentUnconditioned {
            picture: Box::new(BoundaryPicture::AuthoredBlack {
                clock: clock(position),
            }),
            frame: declaration(),
            content: None,
        };
        assert!(fixture.validate().is_err());
        let relative = position.checked_sub(ExactRatio::integer(20)).unwrap();
        let GenerationInputs::Extension { opposite, .. } = &mut fixture.binding.inputs else {
            unreachable!()
        };
        *opposite = Some(RelativeGenerationPicture {
            position: relative,
            picture: GenerationPictureIdentity::AuthoredBlack,
        });
        fixture.validate().unwrap();
        let GenerationInputs::Extension { opposite, .. } = &mut fixture.binding.inputs else {
            unreachable!()
        };
        opposite.as_mut().unwrap().position = ExactRatio::ZERO;
        assert!(fixture.validate().is_err());
        let GenerationInputs::Extension { opposite, .. } = &mut fixture.binding.inputs else {
            unreachable!()
        };
        *opposite = Some(RelativeGenerationPicture {
            position: relative,
            picture: original(7),
        });
        assert!(fixture.validate().is_err());
        fixture.opposite = ExtensionOppositeSeam::Absent;
        assert!(fixture.validate().is_err());
    }
}

#[test]
fn original_and_generated_identity_fields_are_checked_without_inventing_crop_proof() {
    let boundary = original_boundary();
    let sample = |picture| RelativeGenerationPicture {
        position: ExactRatio::ZERO,
        picture,
    };
    validate_sample(&sample(original(7)), &boundary, &boundary).unwrap();
    assert!(validate_sample(&sample(original(8)), &boundary, &boundary).is_err());
    assert!(
        validate_sample(
            &sample(GenerationPictureIdentity::Original {
                qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
                frame: SourceFrameId(7),
            }),
            &boundary,
            &boundary
        )
        .is_err()
    );
    assert!(
        validate_sample(
            &sample(GenerationPictureIdentity::AuthoredBlack),
            &boundary,
            &boundary
        )
        .is_err()
    );
    let boundary = BoundaryPicture::Generated {
        clock: boundary.clock().clone(),
        sampled_asset: AssetId::new("generated").unwrap(),
        sampled_object: sampled_object("b"),
        provenance: sampled_object("c"),
        picture: decoded(),
    };
    for aspect in [None, Some([16, 9]), Some([1, 1])] {
        validate_sample(&sample(generated(7, aspect)), &boundary, &boundary).unwrap();
    }
    for aspect in [[0, 1], [1, 0], [0, 0], [32, 18]] {
        assert!(
            validate_sample(&sample(generated(7, Some(aspect))), &boundary, &boundary).is_err()
        );
    }
    assert!(validate_sample(&sample(generated(8, None)), &boundary, &boundary).is_err());
    assert!(
        validate_sample(
            &sample(GenerationPictureIdentity::Generated {
                sampled_object: sampled_object("c"),
                frame: SourceFrameId(7),
                content_aspect: None,
            }),
            &boundary,
            &boundary
        )
        .is_err()
    );
}

fn supported_samples() -> Vec<RelativeGenerationPicture> {
    vec![
        RelativeGenerationPicture {
            position: ExactRatio::ZERO,
            picture: original(7),
        },
        RelativeGenerationPicture {
            position: ExactRatio::ONE,
            picture: original(8),
        },
        // The endpoint is outside half-open support and may change provider.
        RelativeGenerationPicture {
            position: ExactRatio::integer(2),
            picture: generated(90, None),
        },
    ]
}

fn support() -> GenerationInputSupport {
    GenerationInputSupport {
        start: ExactRatio::ZERO,
        end_exclusive: ExactRatio::integer(2),
        first: original(7),
        last: original(8),
    }
}

#[test]
fn half_open_support_requires_full_coverage_and_an_independent_terminal() {
    let samples = supported_samples();
    let terminal = samples.last().unwrap();
    validate_support(&samples, &[support()], terminal).unwrap();
    assert!(validate_support(&samples, &[], terminal).is_err());
    assert!(validate_support(&samples, &[support(), support()], terminal).is_err());
    for (start, end) in [(1, 2), (0, 1), (0, 3), (0, 0), (2, 1)] {
        let span = GenerationInputSupport {
            start: ExactRatio::integer(start),
            end_exclusive: ExactRatio::integer(end),
            ..support()
        };
        assert!(validate_support(&samples, &[span], terminal).is_err());
    }
    let mut wrong_terminal = terminal.clone();
    wrong_terminal.picture = original(9);
    assert!(validate_support(&samples, &[support()], &wrong_terminal).is_err());
    validate_support(&samples[2..], &[], terminal).unwrap();
    assert!(validate_support(&samples[2..], &[support()], terminal).is_err());
}

#[test]
fn span_providers_start_identities_and_interior_ordinals_are_checked() {
    let samples = supported_samples();
    let terminal = samples.last().unwrap();
    for (first, last) in [
        (original(6), original(8)),
        (original(7), original(7)),
        (original(7), generated(8, None)),
        (
            GenerationPictureIdentity::AuthoredBlack,
            GenerationPictureIdentity::AuthoredBlack,
        ),
    ] {
        assert!(
            validate_support(
                &samples,
                &[GenerationInputSupport {
                    first,
                    last,
                    ..support()
                }],
                terminal
            )
            .is_err()
        );
    }
    let mut samples = supported_samples();
    samples[1].picture = GenerationPictureIdentity::Original {
        qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
        frame: SourceFrameId(8),
    };
    assert!(validate_support(&samples, &[support()], samples.last().unwrap()).is_err());
    assert!(!same_provider(
        &generated(7, None),
        &generated(8, Some([16, 9]))
    ));
    assert!(inside_provider_span(
        &original(8),
        &original(10),
        &original(7)
    ));
}

fn stamp(ticks: i64) -> SourceTimestamp {
    SourceTimestamp {
        ticks,
        time_base: SourceTimeBase::new(1, 1000).unwrap(),
    }
}

fn target() -> AttentionTarget {
    AttentionTarget {
        label: "Hand".into(),
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(stamp(0), stamp(1000)).unwrap(),
        region: TargetRegion {
            center: [500_000, 500_000],
            size: [200_000, 300_000],
        },
        samples: vec![],
        corrections: vec![],
        provenance: None,
    }
}

#[test]
fn region_is_recomputed_from_retained_record_including_hash_and_geometry() {
    let mut fixture = Fixture::new(ExtensionDirection::FromLeft, 1);
    let presentation = RasterRect::new(0, 0, 512, 320).unwrap();
    let anchor = ExtensionContextPicture {
        picture: original_boundary(),
        frame: declaration(),
        content: Some(presentation),
    };
    let id = TargetId::new("hand").unwrap();
    let record = target();
    let region =
        ExtensionRegionCapture::new(id.clone(), &record, &anchor, presentation, [512, 320])
            .unwrap();
    fixture.binding.region = Some(GenerationRegionIdentity {
        id: id.clone(),
        record: Some(record.clone()),
    });
    let check = |binding: &GenerationInputBinding, region: &ExtensionRegionCapture| {
        validate_region(binding, region, &anchor, presentation, [512, 320])
    };
    check(&fixture.binding, &region).unwrap();
    assert!(check(&fixture.binding, &ExtensionRegionCapture::None).is_err());
    let mut changed = region.clone();
    let ExtensionRegionCapture::Selected { target_sha256, .. } = &mut changed else {
        unreachable!()
    };
    *target_sha256 = Sha256::new("f".repeat(64)).unwrap();
    assert!(check(&fixture.binding, &changed).is_err());
    let mut changed = region.clone();
    let ExtensionRegionCapture::Selected {
        anchor: captured, ..
    } = &mut changed
    else {
        unreachable!()
    };
    let CapturedRegionBoundary::Available {
        region: geometry, ..
    } = captured.as_mut()
    else {
        panic!("available")
    };
    geometry.center[0] += 1;
    assert!(check(&fixture.binding, &changed).is_err());
    fixture
        .binding
        .region
        .as_mut()
        .unwrap()
        .record
        .as_mut()
        .unwrap()
        .label = "Changed".into();
    assert!(check(&fixture.binding, &region).is_err());
    fixture.binding.region.as_mut().unwrap().record = None;
    assert!(check(&fixture.binding, &region).is_err());
    fixture.binding.region.as_mut().unwrap().record = Some(record);
    fixture.binding.region.as_mut().unwrap().id = TargetId::new("different").unwrap();
    assert!(check(&fixture.binding, &region).is_err());
    fixture.binding.region = None;
    assert!(check(&fixture.binding, &region).is_err());
    check(&fixture.binding, &ExtensionRegionCapture::None).unwrap();
}

#[test]
fn serialized_binding_limit_counts_json_escapes() {
    let mut fixture = Fixture::new(ExtensionDirection::FromLeft, 1);
    let mut record = target();
    record.label = "\n".repeat(MAX_INPUT_BINDING_BYTES / 2);
    fixture.binding.region = Some(GenerationRegionIdentity {
        id: TargetId::new("hand").unwrap(),
        record: Some(record),
    });
    assert!(validate_size(&fixture.binding).is_err());
    assert!(fixture.validate().is_err());
}

#[test]
fn unavailable_anchor_cannot_hide_malformed_target_observations() {
    use deadpan_core::{
        TargetCorrection, TargetProvenance, TargetRule, TargetSample, TargetStop, TrackState,
    };

    let mut fixture = Fixture::new(ExtensionDirection::FromLeft, 1);
    let id = TargetId::new("saved-target").unwrap();
    for defect in [
        "valid",
        "confidence",
        "outside",
        "order",
        "correction",
        "provenance",
    ] {
        let mut record = target();
        let sample = TargetSample {
            at: record.span.start().ticks,
            region: record.region,
            confidence: 900,
            state: TrackState::Tracked,
        };
        match defect {
            "valid" => {}
            "confidence" => record.samples.push(TargetSample {
                confidence: 1001,
                ..sample
            }),
            "outside" => record.samples.push(TargetSample {
                at: record.span.end().ticks,
                ..sample
            }),
            "order" => record.samples.extend([sample, sample]),
            "correction" => record.corrections.push(TargetCorrection {
                at: record.span.end().ticks,
                region: record.region,
            }),
            "provenance" => {
                record.provenance = Some(TargetProvenance {
                    rule: TargetRule::DeadpanTrack1,
                    engine: "hidden\0engine".into(),
                    stop: TargetStop::RangeEnd,
                })
            }
            _ => unreachable!(),
        }
        // Recompute the hash too: a matching hash must not substitute for
        // structural validation when black has no measurable target anchor.
        let captured = ExtensionRegionCapture::new(
            id.clone(),
            &record,
            &fixture.context[0],
            fixture.presentation,
            [512, 320],
        )
        .unwrap();
        fixture.binding.region = Some(GenerationRegionIdentity {
            id: id.clone(),
            record: Some(record),
        });
        let result = validate_region(
            &fixture.binding,
            &captured,
            &fixture.context[0],
            fixture.presentation,
            [512, 320],
        );
        assert_eq!(result.is_ok(), defect == "valid", "{defect}: {result:?}");
    }
}
