use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;

use deadpan_analysis::{PictureSignature, encode_context_signatures};
use deadpan_core::{FrameDuration, FrameRate, NodeId};
use deadpan_jobs::{
    AttemptId, AxisLimits, CancellationToken, ConditioningMode, ContextArtifact, DimensionLimits,
    ExtensionCapability, ExtensionCapturePolicy, FrameCountFormula, GenerationCaptureSpec,
    GenerationInputBinding, GenerationInputSupport, GenerationInputs, GenerationPictureIdentity,
    GenerationRegionIdentity, HoldConstraints, HoldTarget, MessageIdentity, MotionAmount,
    NativeDimensions, ProtocolVersion, ProviderPackId, ProviderPackVersion, ProviderSelection,
    RelativeGenerationPicture, RequestId, RequestVersion, RuntimeId, RuntimeVersion, VideoSpec,
};
use sha2::Digest;

use super::*;

#[test]
fn adversarial_extension_conditioning_and_receipts() {
    use deadpan_chaos::{Target, Verdict, fuzz, reject};

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(untagged)]
    enum Input {
        Context(Box<ExtensionContext>),
        Receipt(Box<ExtensionConditioningReceipt>),
    }

    let mut seeds = Vec::new();
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for opposite in [false, true] {
            let fixture = Fixture::new(direction, opposite);
            let retained = fixture.capture().unwrap();
            seeds.push(serde_json::to_vec(retained.context()).unwrap());
            seeds.push(serde_json::to_vec(retained.receipt()).unwrap());
        }
    }
    fuzz(
        Target::json("models-extension-conditioning").iterations(400),
        seeds,
        |bytes| {
            let value: Input = match serde_json::from_slice(bytes) {
                Ok(value) => value,
                Err(error) => return reject(error),
            };
            let encoded = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
            let decoded: Input =
                serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
            if value != decoded {
                return Err("extension input round trip changed its identity".into());
            }
            Ok(Verdict::Accepted)
        },
    )
    .assert_clean();
}

fn declaration(reference: &str, bytes: &[u8]) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(reference).unwrap(),
        Sha256::new(
            sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
        .unwrap(),
        u64::try_from(bytes.len()).unwrap(),
    )
    .unwrap()
}

fn plan(direction: ExtensionDirection, rate: FrameRate) -> ExtensionGenerationPlan {
    ExtensionGenerationPlan::new(
        direction,
        FrameDuration::new(3).unwrap(),
        rate,
        &ExtensionCapability::new(
            FrameRate::new(24, 1).unwrap(),
            9,
            FrameCountFormula::new(8, 0, 8, 8).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(4, 4, 1).unwrap(),
                AxisLimits::new(2, 2, 1).unwrap(),
            ),
            FrameDuration::new(10).unwrap(),
        )
        .unwrap(),
        NativeDimensions::new(4, 2).unwrap(),
    )
    .unwrap()
}

fn clock(position: ExactRatio) -> BoundaryClock {
    BoundaryClock::Definition {
        project_id: ProjectId::new("project").unwrap(),
        revision_id: RevisionId::new("revision").unwrap(),
        definition: NodeId::new("definition").unwrap(),
        position,
    }
}

fn context(
    direction: ExtensionDirection,
    rate: FrameRate,
    opposite_present: bool,
) -> ExtensionContext {
    let step = ExactRatio::new(
        i128::from(rate.numerator()),
        i128::from(rate.denominator()) * 24,
    )
    .unwrap();
    let start = ExactRatio::integer(20);
    let frames = (0..9)
        .map(|index| ExtensionContextPicture {
            picture: BoundaryPicture::AuthoredBlack {
                clock: clock(
                    start
                        .checked_add(step.checked_mul(ExactRatio::integer(index)).unwrap())
                        .unwrap(),
                ),
            },
            frame: declaration(
                &format!("inputs/frame-{index}.png"),
                format!("opaque frame {index}").as_bytes(),
            ),
            content: None,
        })
        .collect::<Vec<_>>();
    let anchor = match direction {
        ExtensionDirection::FromLeft => &frames[8],
        ExtensionDirection::FromRight => &frames[0],
    };
    let BoundaryClock::Definition { position, .. } = anchor.picture.clock() else {
        unreachable!()
    };
    let opposite = if opposite_present {
        ExtensionOppositeSeam::PresentUnconditioned {
            picture: Box::new(BoundaryPicture::AuthoredBlack {
                clock: clock(match direction {
                    ExtensionDirection::FromLeft => {
                        position.checked_add(ExactRatio::integer(4)).unwrap()
                    }
                    ExtensionDirection::FromRight => {
                        position.checked_sub(ExactRatio::integer(4)).unwrap()
                    }
                }),
            }),
            frame: declaration("inputs/opposite.png", b"opaque opposite"),
            content: None,
        }
    } else {
        ExtensionOppositeSeam::Absent
    };
    let plan = plan(direction, rate);
    let continuity = continuity_metadata(&plan, &frames, &opposite, [4, 2], None);
    ExtensionContext::new(
        plan,
        frames,
        RasterRect::new(0, 0, 4, 2).unwrap(),
        opposite,
        "opaque test inputs, not qualified PNGs",
        ExtensionRegionCapture::None,
        continuity,
    )
    .unwrap()
}

fn picture_identity(picture: &BoundaryPicture) -> GenerationPictureIdentity {
    match picture {
        BoundaryPicture::Original {
            qualification,
            picture,
            ..
        } => GenerationPictureIdentity::Original {
            qualification: qualification.clone(),
            frame: picture.source_frame,
        },
        BoundaryPicture::Generated {
            sampled_object,
            picture,
            ..
        } => GenerationPictureIdentity::Generated {
            sampled_object: sampled_object.clone(),
            frame: picture.source_frame,
            content_aspect: None,
        },
        BoundaryPicture::AuthoredBlack { .. } => GenerationPictureIdentity::AuthoredBlack,
    }
}

// All-black fixtures need only the binary header. The Original-anchor tests
// below inspect shape only, so their declared physical counts deliberately do
// not establish source admission or enough signatures for capture.
fn continuity_metadata(
    plan: &ExtensionGenerationPlan,
    frames: &[ExtensionContextPicture],
    opposite: &ExtensionOppositeSeam,
    canvas: [u32; 2],
    region: Option<GenerationRegionIdentity>,
) -> ExtensionContinuityEvidence {
    let anchor = match plan.direction() {
        ExtensionDirection::FromLeft => &frames.last().unwrap().picture,
        ExtensionDirection::FromRight => &frames[0].picture,
    };
    let observe = |picture: &BoundaryPicture| RelativeGenerationPicture {
        position: clock_difference(anchor, picture).unwrap(),
        picture: picture_identity(picture),
    };
    let samples: Vec<_> = frames.iter().map(|item| observe(&item.picture)).collect();
    let terminal = samples.last().unwrap().clone();
    let support = if samples.len() == 1 {
        vec![]
    } else {
        vec![GenerationInputSupport {
            start: samples[0].position,
            end_exclusive: terminal.position,
            first: samples[0].picture.clone(),
            last: samples[samples.len() - 2].picture.clone(),
        }]
    };
    let count = |picture: &GenerationPictureIdentity| match picture {
        GenerationPictureIdentity::Original { frame, .. }
        | GenerationPictureIdentity::Generated { frame, .. } => {
            Some(usize::try_from(frame.0 + 1).unwrap())
        }
        GenerationPictureIdentity::AuthoredBlack => None,
    };
    let counts = support
        .iter()
        .map(|span| count(&span.last))
        .chain(std::iter::once(count(&terminal.picture)))
        .collect();
    ExtensionContinuityEvidence::new(
        GenerationInputBinding {
            duration: plan.project_frames(),
            frame_rate: plan.project_frame_rate(),
            canvas,
            inputs: GenerationInputs::Extension {
                capture: GenerationCaptureSpec::Extension {
                    direction: plan.direction(),
                    native_rate: plan.native_frame_rate(),
                    context_frames: plan.context_frame_count(),
                    policy: ExtensionCapturePolicy::TemporalContextV1,
                },
                samples,
                opposite: match opposite {
                    ExtensionOppositeSeam::Absent => None,
                    ExtensionOppositeSeam::PresentUnconditioned { picture, .. } => {
                        Some(observe(picture))
                    }
                },
                support,
                terminal,
            },
            region,
        },
        vec![],
        counts,
        declaration(
            "inputs/continuity.bin",
            &encode_context_signatures(&[]).unwrap(),
        ),
    )
    .unwrap()
}

fn request(context: &ExtensionContext, manifest: &WorkspaceArtifact) -> HostMessage {
    let dimensions = context.plan.native_dimensions();
    HostMessage::GenerateExtension {
        protocol: ProtocolVersion::V3,
        identity: MessageIdentity::new(
            RequestId::new("request").unwrap(),
            AttemptId::new("attempt").unwrap(),
        ),
        cancellation_token: CancellationToken::new("cancel").unwrap(),
        project_id: ProjectId::new("project").unwrap(),
        revision_id: RevisionId::new("revision").unwrap(),
        target: HoldTarget {
            hold_id: NodeId::new("hold").unwrap(),
            request_version: RequestVersion::new(1).unwrap(),
        },
        input: ContextArtifact {
            manifest: manifest.reference().clone(),
            sha256: manifest.sha256().clone(),
        },
        output_workspace: WorkspaceRef::new("outputs").unwrap(),
        constraints: HoldConstraints {
            video: VideoSpec::new(
                context.plan.project_frames(),
                context.plan.project_frame_rate(),
                dimensions.width(),
                dimensions.height(),
            )
            .unwrap(),
            conditioning: match context.plan.direction() {
                ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
                ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
            },
            motion: MotionAmount::Still,
            instructions: None,
            region_target: context.region.target_id().cloned(),
        },
        provider: Box::new(ProviderSelection {
            pack_id: ProviderPackId::new("pack").unwrap(),
            pack_version: ProviderPackVersion::new("1").unwrap(),
            runtime_id: RuntimeId::new("runtime").unwrap(),
            runtime_version: RuntimeVersion::new("1").unwrap(),
            seed: 1,
        }),
        plan: Box::new(context.plan.clone()),
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    context: ExtensionContext,
    manifest: WorkspaceArtifact,
}

impl Fixture {
    fn new(direction: ExtensionDirection, opposite: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("inputs")).unwrap();
        fs::create_dir(directory.path().join("outputs")).unwrap();
        let context = context(direction, FrameRate::new(30, 1).unwrap(), opposite);
        for index in 0..9 {
            fs::write(
                directory.path().join(format!("inputs/frame-{index}.png")),
                format!("opaque frame {index}"),
            )
            .unwrap();
        }
        fs::write(
            directory.path().join("inputs/opposite.png"),
            b"opaque opposite",
        )
        .unwrap();
        fs::write(
            directory.path().join("inputs/continuity.bin"),
            encode_context_signatures(&[]).unwrap(),
        )
        .unwrap();
        let bytes = serde_json::to_vec(&context).unwrap();
        fs::write(directory.path().join("inputs/context.json"), &bytes).unwrap();
        let manifest = declaration("inputs/context.json", &bytes);
        Self {
            directory,
            context,
            manifest,
        }
    }

    fn request(&self) -> HostMessage {
        request(&self.context, &self.manifest)
    }
    fn capture(&self) -> Result<RetainedExtensionConditioning, QualificationError> {
        self.capture_with(
            &self.request(),
            ConditioningLimits::new(1024 * 1024, 16 * 1024 * 1024, 30_000).unwrap(),
            &AtomicBool::new(false),
        )
    }
    fn capture_with(
        &self,
        request: &HostMessage,
        limits: ConditioningLimits,
        cancelled: &AtomicBool,
    ) -> Result<RetainedExtensionConditioning, QualificationError> {
        capture_extension_conditioning(
            &ArtifactWorkspace::open(self.directory.path()).unwrap(),
            request,
            &self.manifest,
            &WorkspaceRef::new("inputs").unwrap(),
            limits,
            cancelled,
        )
    }
    fn replace_manifest(&mut self, bytes: &[u8]) {
        fs::write(self.directory.path().join("inputs/context.json"), bytes).unwrap();
        self.manifest = declaration("inputs/context.json", bytes);
    }
}

#[test]
fn both_directions_retain_exact_native_spacing_and_optional_opposite_seams() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for rate in [
            FrameRate::new(30, 1).unwrap(),
            FrameRate::new(30_000, 1001).unwrap(),
        ] {
            for present in [false, true] {
                let value = context(direction, rate, present);
                let wire = serde_json::to_value(&value).unwrap();
                assert_eq!(wire["operation"], "extension");
                assert_eq!(wire["schema_version"], 2);
                assert_eq!(
                    serde_json::from_value::<ExtensionContext>(wire).unwrap(),
                    value
                );
                assert_eq!(
                    value.anchor(),
                    &value.context[if direction == ExtensionDirection::FromLeft {
                        8
                    } else {
                        0
                    }]
                );
                let expected_step = ExactRatio::new(
                    i128::from(rate.numerator()),
                    i128::from(rate.denominator()) * 24,
                )
                .unwrap();
                for pair in value.context.windows(2) {
                    assert_eq!(
                        clock_difference(&pair[0].picture, &pair[1].picture).unwrap(),
                        expected_step
                    );
                }
                if let ExtensionOppositeSeam::PresentUnconditioned { picture, .. } = &value.opposite
                {
                    let span = match direction {
                        ExtensionDirection::FromLeft => {
                            clock_difference(&value.anchor().picture, picture)
                        }
                        ExtensionDirection::FromRight => {
                            clock_difference(picture, &value.anchor().picture)
                        }
                    }
                    .unwrap();
                    assert_eq!(span, ExactRatio::integer(4));
                }
            }
        }
    }
}

#[test]
fn context_rejects_clock_order_origin_geometry_and_wire_contradictions() {
    let valid = context(
        ExtensionDirection::FromLeft,
        FrameRate::new(30, 1).unwrap(),
        true,
    );
    let mut changed = valid.clone();
    changed.context.swap(0, 1);
    assert!(changed.validate_shape().is_err());
    let mut changed = valid.clone();
    changed.context.pop();
    assert!(changed.validate_shape().is_err());
    let mut changed = valid.clone();
    changed.context[1].picture = changed.context[0].picture.clone();
    assert!(changed.validate_shape().is_err());
    for field in ["project_id", "revision_id", "definition", "position"] {
        let mut wire = serde_json::to_value(&valid).unwrap();
        wire["context"][1]["picture"]["authored_black"]["clock"][field] = match field {
            "position" => serde_json::to_value(ExactRatio::integer(21)).unwrap(),
            _ => serde_json::json!("different"),
        };
        assert!(
            serde_json::from_value::<ExtensionContext>(wire).is_err(),
            "{field}"
        );
    }
    let mut changed = valid.clone();
    changed.context[0].picture = BoundaryPicture::AuthoredBlack {
        clock: BoundaryClock::Project { frame: 20 },
    };
    assert!(changed.validate_shape().is_err());
    let mut changed = valid.clone();
    changed.context[0].content = Some(changed.presentation);
    assert!(changed.validate_shape().is_err());
    let mut changed = valid.clone();
    changed.presentation.x = 1;
    assert!(changed.validate_shape().is_err());
    let mut changed = valid.clone();
    if let ExtensionOppositeSeam::PresentUnconditioned { picture, .. } = &mut changed.opposite {
        **picture = valid.anchor().picture.clone();
    }
    assert!(changed.validate_shape().is_err());
    let wire = serde_json::to_value(&valid).unwrap();
    for field in [
        "schema_version",
        "operation",
        "region",
        "opposite",
        "presentation",
        "continuity",
    ] {
        let mut changed = wire.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ExtensionContext>(changed).is_err(),
            "missing {field}"
        );
    }
    for (field, value) in [
        ("schema_version", serde_json::json!(1)),
        ("operation", serde_json::json!("bridge")),
        ("extra", serde_json::json!(true)),
    ] {
        let mut changed = wire.clone();
        changed[field] = value;
        assert!(
            serde_json::from_value::<ExtensionContext>(changed).is_err(),
            "{field}"
        );
    }
    let mut changed = wire.clone();
    changed["context"][0]
        .as_object_mut()
        .unwrap()
        .remove("content");
    assert!(serde_json::from_value::<ExtensionContext>(changed).is_err());
    let mut changed = wire;
    changed["context"] = serde_json::json!(vec![valid.context[0].clone(); 65]);
    assert!(serde_json::from_value::<ExtensionContext>(changed).is_err());
}

#[test]
fn selected_anchor_unavailability_is_preserved_and_binds_request_target() {
    let mut fixture = Fixture::new(ExtensionDirection::FromRight, false);
    let stamp = |ticks| deadpan_core::SourceTimestamp {
        ticks,
        time_base: deadpan_core::SourceTimeBase::new(1, 1000).unwrap(),
    };
    let target = AttentionTarget {
        label: "Face".into(),
        asset: deadpan_core::AssetId::new("original").unwrap(),
        span: deadpan_core::SourceSpan::new(stamp(0), stamp(1000)).unwrap(),
        region: deadpan_core::TargetRegion {
            center: [500_000, 500_000],
            size: [200_000, 300_000],
        },
        samples: vec![],
        corrections: vec![],
        provenance: None,
    };
    let id = TargetId::new("face").unwrap();
    fixture.context.region = ExtensionRegionCapture::new(
        id.clone(),
        &target,
        fixture.context.anchor(),
        fixture.context.presentation,
        [4, 2],
    )
    .unwrap();
    fixture.context.continuity = continuity_metadata(
        &fixture.context.plan,
        &fixture.context.context,
        &fixture.context.opposite,
        [4, 2],
        Some(GenerationRegionIdentity {
            id,
            record: Some(target),
        }),
    );
    assert!(
        matches!(&fixture.context.region, ExtensionRegionCapture::Selected { anchor, .. }
        if matches!(anchor.as_ref(), CapturedRegionBoundary::Unavailable { reason: RegionCaptureUnavailable::NotOriginal }))
    );
    let bytes = serde_json::to_vec(&fixture.context).unwrap();
    fixture.replace_manifest(&bytes);
    let retained = fixture.capture().unwrap();
    assert_eq!(retained.context().region(), &fixture.context.region);
    let wire = serde_json::to_value(retained.context()).unwrap();
    assert!(wire["region"].get("anchor").is_some());
    assert!(wire["region"].get("left").is_none());
    let mut request = fixture.request();
    let HostMessage::GenerateExtension { constraints, .. } = &mut request else {
        unreachable!()
    };
    constraints.region_target = None;
    assert!(retained.validate_for(&request).is_err());
    let mut wire = wire;
    wire["region"]["left"] = wire["region"]["anchor"].clone();
    assert!(serde_json::from_value::<ExtensionContext>(wire).is_err());
}

#[test]
fn selected_region_captures_one_exact_original_anchor_and_rejects_a_moved_seed() {
    use crate::{DecodedBoundary, MeasuredStream, ModelInputConversion};
    use deadpan_core::{
        AssetId, SourceFrameId, SourceQualificationId, SourceSpan, SourceTimeBase, SourceTimestamp,
        TargetRegion,
    };

    let stamp = |ticks| SourceTimestamp {
        ticks,
        time_base: SourceTimeBase::new(1, 1000).unwrap(),
    };
    let mut value = context(
        ExtensionDirection::FromLeft,
        FrameRate::new(30, 1).unwrap(),
        false,
    );
    let mut plan_wire = serde_json::to_value(&value.plan).unwrap();
    plan_wire["native_dimensions"] = serde_json::json!({"width":768,"height":320});
    value.plan = serde_json::from_value(plan_wire).unwrap();
    value.presentation = RasterRect::new(0, 0, 768, 320).unwrap();
    value.context[8].content = Some(value.presentation);
    value.context[8].picture = BoundaryPicture::Original {
        clock: value.context[8].picture.clock().clone(),
        asset: AssetId::new("original").unwrap(),
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        picture: DecodedBoundary {
            source_frame: SourceFrameId(7),
            pts: stamp(200),
            stream: MeasuredStream {
                codec: "ffv1".into(),
                pixel_format: "rgb24".into(),
                width: 768,
                height: 320,
                sample_aspect: [1, 1],
                rotation_quarter_turns: 0,
                decoded_sample_bits: 8,
                color: CANONICAL_BRIDGE_COLOR,
            },
            model_input: ModelInputConversion::SrgbCodesUnchanged,
        },
    };
    value.continuity = continuity_metadata(
        &value.plan,
        &value.context,
        &value.opposite,
        [768, 320],
        None,
    );
    let mut approximate = value.clone();
    let BoundaryPicture::Original { picture, .. } = &mut approximate.context[8].picture else {
        unreachable!()
    };
    picture.stream.color.transfer = crate::BridgeTransfer::Bt709;
    picture.model_input = ModelInputConversion::Rec709CodesAsSrgb;
    assert!(approximate.validate_shape().is_err());
    if let BoundaryPicture::Original { picture, .. } = &mut approximate.context[8].picture {
        picture.model_input = ModelInputConversion::Rec709ToSrgb;
    }
    assert!(approximate.validate_shape().is_ok());
    let target = AttentionTarget {
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
    };
    value.region = ExtensionRegionCapture::new(
        TargetId::new("hand").unwrap(),
        &target,
        value.anchor(),
        value.presentation,
        [768, 320],
    )
    .unwrap();
    value.continuity = continuity_metadata(
        &value.plan,
        &value.context,
        &value.opposite,
        [768, 320],
        Some(GenerationRegionIdentity {
            id: TargetId::new("hand").unwrap(),
            record: Some(target),
        }),
    );
    value.validate_shape().unwrap();
    let seed = value
        .region
        .seed(value.anchor(), value.presentation, [768, 320])
        .unwrap()
        .unwrap();
    assert!((seed.x() - 0.4).abs() < 1e-12);
    assert!((seed.y() - 0.35).abs() < 1e-12);
    assert!((seed.width() - 0.2).abs() < 1e-12);
    assert!((seed.height() - 0.3).abs() < 1e-12);
    assert!(value.region.unavailable_reason().is_none());
    let ExtensionRegionCapture::Selected { anchor, .. } = &mut value.region else {
        unreachable!()
    };
    let CapturedRegionBoundary::Available { point, .. } = anchor.as_mut() else {
        panic!("qualified anchor region")
    };
    assert_eq!(point.ticks, ExactRatio::integer(200));
    point.ticks = ExactRatio::integer(201);
    assert!(
        value.validate_shape().is_err(),
        "region cannot drift to another decoded source PTS"
    );
    assert!(
        value
            .region
            .seed(value.anchor(), value.presentation, [768, 320])
            .is_err()
    );
}

#[test]
fn immutable_snapshots_and_blake3_receipts_cover_every_input_in_both_directions() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for opposite in [false, true] {
            let fixture = Fixture::new(direction, opposite);
            let retained = fixture.capture().unwrap();
            retained.validate_for(&fixture.request()).unwrap();
            assert_eq!(retained.context_frames().len(), 9);
            assert_eq!(retained.opposite().is_some(), opposite);
            let receipt = retained.receipt().clone();
            assert_eq!(receipt.schema_version(), 2);
            assert!(retained.measurements().is_empty());
            assert_eq!(
                retained.signatures().declaration(),
                fixture.context.continuity.signatures()
            );
            assert_eq!(
                serde_json::from_value::<ExtensionConditioningReceipt>(
                    serde_json::to_value(&receipt).unwrap()
                )
                .unwrap(),
                receipt
            );
            fs::write(
                fixture.directory.path().join("inputs/frame-4.png"),
                b"changed later",
            )
            .unwrap();
            fs::remove_file(fixture.directory.path().join("inputs/context.json")).unwrap();
            fs::remove_file(fixture.directory.path().join("inputs/opposite.png")).unwrap();
            fs::write(
                fixture.directory.path().join("inputs/continuity.bin"),
                b"changed signatures",
            )
            .unwrap();
            retained.validate_for(&fixture.request()).unwrap();
            let (mut manifest, frames, opposite, mut signatures) = retained.into_parts();
            let mut bytes = Vec::new();
            manifest.read_to_end(&mut bytes).unwrap();
            assert_eq!(
                serde_json::from_slice::<ExtensionContext>(&bytes).unwrap(),
                fixture.context
            );
            for (index, mut frame) in frames.into_iter().enumerate() {
                bytes.clear();
                frame.read_to_end(&mut bytes).unwrap();
                assert_eq!(bytes, format!("opaque frame {index}").as_bytes());
                assert_eq!(
                    receipt.context()[index].object().content().digest(),
                    blake3::hash(&bytes).to_hex().as_str()
                );
            }
            if let Some(mut frame) = opposite {
                bytes.clear();
                frame.read_to_end(&mut bytes).unwrap();
                assert_eq!(bytes, b"opaque opposite");
                assert_eq!(
                    receipt.opposite().unwrap().object().content().digest(),
                    blake3::hash(&bytes).to_hex().as_str()
                );
            }
            bytes.clear();
            signatures.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, encode_context_signatures(&[]).unwrap());
            assert_eq!(
                receipt.signatures().object().content().digest(),
                blake3::hash(&bytes).to_hex().as_str()
            );
        }
    }
}

#[test]
fn capture_rejects_changed_origin_plan_protocol_direction_and_scope() {
    let fixture = Fixture::new(ExtensionDirection::FromLeft, true);
    let retained = fixture.capture().unwrap();
    let limits = ConditioningLimits::new(1024 * 1024, 16 * 1024 * 1024, 30_000).unwrap();
    for field in [
        "revision",
        "project",
        "protocol",
        "direction",
        "plan",
        "manifest",
    ] {
        let mut changed = fixture.request();
        let HostMessage::GenerateExtension {
            revision_id,
            project_id,
            protocol,
            constraints,
            plan: changed_plan,
            input,
            ..
        } = &mut changed
        else {
            unreachable!()
        };
        match field {
            "revision" => *revision_id = RevisionId::new("later").unwrap(),
            "project" => *project_id = ProjectId::new("different").unwrap(),
            "protocol" => *protocol = ProtocolVersion::V2,
            "direction" => constraints.conditioning = ConditioningMode::ExtendFromRight,
            "plan" => {
                **changed_plan = plan(
                    ExtensionDirection::FromRight,
                    FrameRate::new(30, 1).unwrap(),
                );
                constraints.conditioning = ConditioningMode::ExtendFromRight;
            }
            "manifest" => input.sha256 = Sha256::new("f".repeat(64)).unwrap(),
            _ => unreachable!(),
        }
        assert!(retained.validate_for(&changed).is_err(), "{field}");
        assert!(
            fixture
                .capture_with(&changed, limits, &AtomicBool::new(false))
                .is_err(),
            "{field}"
        );
    }
    for output in ["inputs", "inputs/child", "in"] {
        let mut changed = fixture.request();
        let HostMessage::GenerateExtension {
            output_workspace, ..
        } = &mut changed
        else {
            unreachable!()
        };
        *output_workspace = WorkspaceRef::new(output).unwrap();
        let result = fixture.capture_with(&changed, limits, &AtomicBool::new(false));
        assert_eq!(result.is_err(), output != "in", "component scope {output}");
    }
}

#[test]
fn aggregate_bounds_aliases_cancellation_and_corrupt_files_fail() {
    let mut fixture = Fixture::new(ExtensionDirection::FromLeft, true);
    let limits = ConditioningLimits::new(1024 * 1024, 16 * 1024 * 1024, 30_000).unwrap();
    assert!(matches!(
        fixture.capture_with(&fixture.request(), limits, &AtomicBool::new(true)),
        Err(QualificationError::Cancelled)
    ));
    assert!(matches!(
        check_control(&AtomicBool::new(false), Instant::now()),
        Err(QualificationError::Deadline)
    ));
    for small in [
        ConditioningLimits::new(1, 16 * 1024 * 1024, 30_000).unwrap(),
        ConditioningLimits::new(1024 * 1024, 1, 30_000).unwrap(),
    ] {
        assert!(
            fixture
                .capture_with(&fixture.request(), small, &AtomicBool::new(false))
                .is_err()
        );
    }
    let mut oversized = fixture.context.clone();
    oversized.context[0].frame = WorkspaceArtifact::new(
        WorkspaceRef::new("inputs/huge").unwrap(),
        Sha256::new("a".repeat(64)).unwrap(),
        MAXIMUM_EXTENSION_INPUT_BYTES,
    )
    .unwrap();
    assert!(oversized.validate_shape().is_err());
    let mut alias = fixture.context.clone();
    alias.context[1].frame = alias.context[0].frame.clone();
    assert!(
        alias.validate_shape().is_ok(),
        "same declared bytes may be used at distinct clock positions"
    );
    alias.context[1].frame = WorkspaceArtifact::new(
        alias.context[0].frame.reference().clone(),
        Sha256::new("f".repeat(64)).unwrap(),
        alias.context[0].frame.byte_length(),
    )
    .unwrap();
    assert!(alias.validate_shape().is_err());
    let mut alias = fixture.context.clone();
    alias.context[0].frame = fixture.manifest.clone();
    let bytes = serde_json::to_vec(&alias).unwrap();
    fixture.replace_manifest(&bytes);
    assert!(
        fixture.capture().is_err(),
        "manifest cannot name itself as a frame"
    );
    let bytes = serde_json::to_vec(&fixture.context).unwrap();
    fixture.replace_manifest(&bytes);
    fs::write(
        fixture.directory.path().join("inputs/frame-2.png"),
        b"corrupt",
    )
    .unwrap();
    assert!(fixture.capture().is_err());
    fs::write(
        fixture.directory.path().join("inputs/frame-2.png"),
        b"opaque frame 2",
    )
    .unwrap();
    fs::remove_file(fixture.directory.path().join("inputs/frame-3.png")).unwrap();
    assert!(fixture.capture().is_err());
    symlink(
        fixture.directory.path().join("inputs/frame-2.png"),
        fixture.directory.path().join("inputs/frame-3.png"),
    )
    .unwrap();
    assert!(fixture.capture().is_err());
}

#[test]
fn strict_capture_rejects_duplicate_json_keys_and_receipt_alias_tampering() {
    let mut fixture = Fixture::new(ExtensionDirection::FromLeft, false);
    let retained = fixture.capture().unwrap();
    let mut receipt = serde_json::to_value(retained.receipt()).unwrap();
    receipt["context"][1] = receipt["context"][0].clone();
    receipt["context"][1]["object"]["content"]["digest"] = serde_json::json!("c".repeat(64));
    assert!(serde_json::from_value::<ExtensionConditioningReceipt>(receipt).is_err());
    let bytes = serde_json::to_string(&fixture.context).unwrap();
    let duplicate = bytes.replacen("{", "{\"schema_version\":2,", 1);
    fixture.replace_manifest(duplicate.as_bytes());
    assert!(fixture.capture().is_err());
}

#[test]
fn continuity_metadata_and_descriptor_tampering_rejects_context_deserialization() {
    let value = context(
        ExtensionDirection::FromLeft,
        FrameRate::new(30, 1).unwrap(),
        false,
    );
    let wire = serde_json::to_value(&value).unwrap();
    for path in [
        "/opposite",
        "/region",
        "/continuity/binding/inputs/samples/0/picture",
    ] {
        let mut changed = wire.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("hidden".into(), serde_json::json!(true));
        assert!(
            serde_json::from_value::<ExtensionContext>(changed).is_err(),
            "hidden field at {path}"
        );
    }
    for (path, field) in [
        ("/continuity/binding", "region"),
        ("/continuity/binding/inputs", "opposite"),
    ] {
        let mut changed = wire.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<ExtensionContext>(changed).is_err(),
            "missing {field}"
        );
    }
    for (pointer, replacement) in [
        (
            "/continuity/capture_policy",
            serde_json::json!("later-policy"),
        ),
        ("/continuity/shot_rule", serde_json::json!("unknown-shots")),
        (
            "/continuity/signature_encoding",
            serde_json::json!("unknown-encoding"),
        ),
        (
            "/continuity/source_picture_counts",
            serde_json::json!([null]),
        ),
        (
            "/continuity/source_picture_counts",
            serde_json::json!([1, 1]),
        ),
        (
            "/continuity/pictures",
            serde_json::json!([{"kind":"authored_black"}]),
        ),
        (
            "/continuity/binding/duration",
            serde_json::to_value(FrameDuration::new(4).unwrap()).unwrap(),
        ),
        ("/continuity/binding/canvas", serde_json::json!([2, 4])),
        (
            "/continuity/binding/inputs/samples/1/position",
            serde_json::to_value(ExactRatio::ZERO).unwrap(),
        ),
        (
            "/continuity/binding/inputs/terminal/position",
            serde_json::to_value(ExactRatio::ONE).unwrap(),
        ),
        ("/continuity/binding/inputs/support", serde_json::json!([])),
    ] {
        let mut changed = wire.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            serde_json::from_value::<ExtensionContext>(changed).is_err(),
            "{pointer}"
        );
    }
    for missing in [
        "binding",
        "pictures",
        "source_picture_counts",
        "signatures",
        "capture_policy",
        "shot_rule",
        "signature_encoding",
    ] {
        let mut changed = wire.clone();
        changed["continuity"]
            .as_object_mut()
            .unwrap()
            .remove(missing);
        assert!(
            serde_json::from_value::<ExtensionContext>(changed).is_err(),
            "missing {missing}"
        );
    }
    let mut alias = value.clone();
    alias.continuity = ExtensionContinuityEvidence::new(
        value.continuity.binding().clone(),
        vec![],
        vec![None, None],
        declaration(
            value.context[0].frame.reference().as_str(),
            &encode_context_signatures(&[]).unwrap(),
        ),
    )
    .unwrap();
    assert!(
        alias.validate_shape().is_err(),
        "signatures cannot alias picture inputs"
    );
}

#[test]
fn signature_files_are_hashed_parsed_bounded_and_required_by_receipts() {
    let mut fixture = Fixture::new(ExtensionDirection::FromRight, true);
    let receipt = fixture.capture().unwrap().receipt().clone();
    let receipt_wire = serde_json::to_value(&receipt).unwrap();
    for missing in ["signatures", "opposite"] {
        let mut changed = receipt_wire.clone();
        changed.as_object_mut().unwrap().remove(missing);
        assert!(serde_json::from_value::<ExtensionConditioningReceipt>(changed).is_err());
    }
    let mut changed = receipt_wire;
    changed["schema_version"] = serde_json::json!(1);
    assert!(serde_json::from_value::<ExtensionConditioningReceipt>(changed).is_err());
    let signature_path = fixture.directory.path().join("inputs/continuity.bin");
    let mut corrupted = encode_context_signatures(&[]).unwrap();
    corrupted[0] ^= 1;
    fs::write(&signature_path, &corrupted).unwrap();
    assert!(
        fixture.capture().is_err(),
        "changed bytes fail the retained SHA-256 declaration"
    );
    fixture.context.continuity = ExtensionContinuityEvidence::new(
        fixture.context.continuity.binding().clone(),
        vec![],
        vec![None, None],
        declaration("inputs/continuity.bin", &corrupted),
    )
    .unwrap();
    fixture.replace_manifest(&serde_json::to_vec(&fixture.context).unwrap());
    assert!(
        fixture.capture().is_err(),
        "matching hash cannot authorize an invalid signature header"
    );
    fs::remove_file(&signature_path).unwrap();
    assert!(
        fixture.capture().is_err(),
        "missing signature artifact cannot be omitted"
    );
    symlink(
        fixture.directory.path().join("inputs/frame-0.png"),
        &signature_path,
    )
    .unwrap();
    assert!(
        fixture.capture().is_err(),
        "signature snapshots reject symbolic links"
    );
}

fn signature(color: impl Fn(u32, u32) -> [u8; 3]) -> PictureSignature {
    let rgba: Vec<u8> = (0..18)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let [r, g, b] = color(x, y);
            [r, g, b, 255]
        })
        .collect();
    PictureSignature::from_rgba(&rgba, 32, 18, 128).unwrap()
}

fn warm(x: u32, _: u32) -> [u8; 3] {
    [200, 120 + ((x * 3 % 192) as u8) / 4, 40]
}

fn cool(_: u32, y: u32) -> [u8; 3] {
    [10, 20, 90 + (y * 4) as u8]
}

fn original_identity(ordinal: usize) -> GenerationPictureIdentity {
    GenerationPictureIdentity::Original {
        qualification: deadpan_core::SourceQualificationId::new("a".repeat(64)).unwrap(),
        frame: deadpan_core::SourceFrameId(u64::try_from(ordinal).unwrap()),
    }
}

fn measured_evidence(
    all: &[PictureSignature],
    requested: std::ops::RangeInclusive<usize>,
) -> (ExtensionContinuityEvidence, Vec<u8>) {
    let value = context(
        ExtensionDirection::FromLeft,
        FrameRate::new(30, 1).unwrap(),
        false,
    );
    let mut binding = value.continuity.binding().clone();
    let GenerationInputs::Extension {
        samples,
        support,
        terminal,
        ..
    } = &mut binding.inputs
    else {
        unreachable!()
    };
    for (index, sample) in samples.iter_mut().enumerate() {
        sample.picture = original_identity(if index == 0 {
            *requested.start()
        } else {
            *requested.end()
        });
    }
    support[0].first = original_identity(*requested.start());
    support[0].last = original_identity(*requested.end());
    terminal.picture = original_identity(*requested.end());
    let window = deadpan_analysis::context_shot_window(requested, all.len()).unwrap();
    let bytes = encode_context_signatures(&all[window.clone()]).unwrap();
    let evidence = ExtensionContinuityEvidence::new(
        binding,
        window.map(original_identity).collect(),
        vec![Some(all.len()), Some(all.len())],
        declaration("inputs/continuity.bin", &bytes),
    )
    .unwrap();
    (evidence, bytes)
}

fn qualify_retained(
    evidence: &ExtensionContinuityEvidence,
    bytes: &[u8],
) -> Result<Vec<ExtensionContextMeasurement>, QualificationError> {
    evidence.qualify_signatures(
        bytes,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(30),
    )
}

#[test]
fn retained_signature_bytes_recompute_abrupt_and_gradual_transitions() {
    let still = vec![signature(warm); 32];
    let (evidence, bytes) = measured_evidence(&still, 0..=24);
    let measurements = qualify_retained(&evidence, &bytes).unwrap();
    assert_eq!(measurements.len(), 2);
    assert!(
        measurements
            .iter()
            .all(|item| item.qualification.transition.is_none())
    );
    assert_eq!(measurements[0].qualification.requested, 0..=24);
    let mut abrupt = still;
    abrupt[11] = signature(cool);
    let (evidence, bytes) = measured_evidence(&abrupt, 0..=24);
    assert!(
        qualify_retained(&evidence, &bytes)
            .unwrap_err()
            .to_string()
            .contains("detected picture transition")
    );
    let fade: Vec<_> = (0..160)
        .map(|_| signature(warm))
        .chain((1..=24).map(|step| {
            signature(|x, y| {
                warm(x, y).map(|channel| ((u32::from(channel) * (25 - step) + 12) / 25) as u8)
            })
        }))
        .chain((0..160).map(|_| signature(|_, _| [0; 3])))
        .collect();
    let (evidence, bytes) = measured_evidence(&fade, 172..=172);
    assert!(
        qualify_retained(&evidence, &bytes)
            .unwrap_err()
            .to_string()
            .contains("detected picture transition")
    );
}

#[test]
fn retained_signatures_recompute_structural_seams_and_reject_missing_rows() {
    let (evidence, _) = measured_evidence(&[signature(warm)], 0..=0);
    let generated = GenerationPictureIdentity::Generated {
        sampled_object: GeneratedObjectRef::new(
            GeneratedContentId::new("b".repeat(64)).unwrap(),
            1,
        )
        .unwrap(),
        frame: deadpan_core::SourceFrameId(0),
        content_aspect: None,
    };
    let mut binding = evidence.binding().clone();
    let GenerationInputs::Extension {
        samples, terminal, ..
    } = &mut binding.inputs
    else {
        unreachable!()
    };
    samples.last_mut().unwrap().picture = generated.clone();
    terminal.picture = generated.clone();
    for (last, rejected) in [(signature(warm), false), (signature(cool), true)] {
        let bytes = encode_context_signatures(&[signature(warm), last]).unwrap();
        let retained = ExtensionContinuityEvidence::new(
            binding.clone(),
            vec![original_identity(0), generated.clone()],
            vec![Some(1), Some(1)],
            declaration("inputs/continuity.bin", &bytes),
        )
        .unwrap();
        let result = qualify_retained(&retained, &bytes);
        assert_eq!(result.is_err(), rejected);
        if rejected {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("abrupt picture seam")
            );
        }
    }
    let bytes = encode_context_signatures(&[signature(warm)]).unwrap();
    let missing = ExtensionContinuityEvidence::new(
        binding,
        vec![original_identity(0)],
        vec![Some(1), Some(1)],
        declaration("inputs/continuity.bin", &bytes),
    )
    .unwrap();
    assert!(
        qualify_retained(&missing, &bytes)
            .unwrap_err()
            .to_string()
            .contains("missing a required physical picture")
    );
    assert!(matches!(
        evidence.qualify_signatures(
            &bytes,
            &AtomicBool::new(true),
            Instant::now() + Duration::from_secs(1)
        ),
        Err(QualificationError::Cancelled)
    ));
    assert!(matches!(
        evidence.qualify_signatures(&bytes, &AtomicBool::new(false), Instant::now()),
        Err(QualificationError::Deadline)
    ));
}
