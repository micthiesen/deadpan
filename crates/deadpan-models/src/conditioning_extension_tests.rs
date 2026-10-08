use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;

use deadpan_core::{FrameDuration, FrameRate, NodeId};
use deadpan_jobs::{
    AttemptId, AxisLimits, CancellationToken, ConditioningMode, ContextArtifact, DimensionLimits,
    ExtensionCapability, FrameCountFormula, HoldConstraints, HoldTarget, MessageIdentity,
    MotionAmount, NativeDimensions, ProtocolVersion, ProviderPackId, ProviderPackVersion,
    ProviderSelection, RequestId, RequestVersion, RuntimeId, RuntimeVersion, VideoSpec,
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
    ExtensionContext::new(
        plan(direction, rate),
        frames,
        RasterRect::new(0, 0, 4, 2).unwrap(),
        opposite,
        "opaque test inputs, not qualified PNGs",
        CANONICAL_BRIDGE_COLOR,
        ExtensionRegionCapture::None,
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
                assert_eq!(wire["schema_version"], 1);
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
    ] {
        let mut changed = wire.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ExtensionContext>(changed).is_err(),
            "missing {field}"
        );
    }
    for (field, value) in [
        ("schema_version", serde_json::json!(2)),
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
    fixture.context.region = ExtensionRegionCapture::Selected {
        target: TargetId::new("face").unwrap(),
        label: "Face".into(),
        target_sha256: Sha256::new("b".repeat(64)).unwrap(),
        anchor: Box::new(CapturedRegionBoundary::Unavailable {
            reason: RegionCaptureUnavailable::NotOriginal,
        }),
    };
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
    value.validate_shape().unwrap();
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
            let (mut manifest, frames, opposite) = retained.into_parts();
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
    let duplicate = bytes.replacen("{", "{\"schema_version\":1,", 1);
    fixture.replace_manifest(duplicate.as_bytes());
    assert!(fixture.capture().is_err());
}
