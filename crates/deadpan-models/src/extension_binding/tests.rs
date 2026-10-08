use deadpan_core::NodeId;
use deadpan_jobs::{
    AttemptId, BridgeCapability, BridgeGenerationPlan, CancellationToken, MotionAmount,
    NativeDimensions, ProtocolVersion, ProviderPackId, ProviderPackVersion, RequestId,
    RequestVersion, RuntimeId, RuntimeVersion, Sha256, VideoSpec, WorkspaceArtifact, WorkspaceRef,
};

use super::*;

const DIRECTIONS: [ExtensionDirection; 2] =
    [ExtensionDirection::FromLeft, ExtensionDirection::FromRight];

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn rate(value: u32) -> FrameRate {
    FrameRate::new(value, 1).unwrap()
}

fn selection() -> ProviderSelection {
    ProviderSelection {
        pack_id: ProviderPackId::new("extension-pack").unwrap(),
        pack_version: ProviderPackVersion::new("1").unwrap(),
        runtime_id: RuntimeId::new("extension-runtime").unwrap(),
        runtime_version: RuntimeVersion::new("1").unwrap(),
        seed: 42,
    }
}

fn capability() -> ExtensionCapability {
    ExtensionCapability::new(
        rate(24),
        9,
        FrameCountFormula::new(8, 0, 8, 24).unwrap(),
        DimensionLimits::new(
            AxisLimits::new(512, 1024, 64).unwrap(),
            AxisLimits::new(256, 512, 64).unwrap(),
        ),
        frames(30),
    )
    .unwrap()
}

fn artifact(reference: &str, digest: char) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(reference).unwrap(),
        Sha256::new(digest.to_string().repeat(64)).unwrap(),
        1024,
    )
    .unwrap()
}

fn fixture(
    direction: ExtensionDirection,
    output_frames: i64,
) -> (
    HostMessage,
    SelectedExtensionProvider,
    NativeCandidateManifest,
) {
    let provider = selection();
    let capability = capability();
    let plan = ExtensionGenerationPlan::new(
        direction,
        frames(output_frames),
        rate(30),
        &capability,
        NativeDimensions::new(512, 320).unwrap(),
    )
    .unwrap();
    let declaration = NativeCandidateManifest {
        native: artifact("outputs/native.mp4", 'a'),
        provenance: artifact("outputs/provenance.json", 'b'),
        video: VideoSpec::new(
            frames(i64::from(plan.native_frame_count())),
            plan.native_frame_rate(),
            512,
            320,
        )
        .unwrap(),
        provider: provider.clone(),
    };
    let request = HostMessage::GenerateExtension {
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
            request_version: RequestVersion::new(3).unwrap(),
        },
        input: ContextArtifact {
            manifest: WorkspaceRef::new("inputs/context.json").unwrap(),
            sha256: Sha256::new("c".repeat(64)).unwrap(),
        },
        output_workspace: WorkspaceRef::new("outputs").unwrap(),
        constraints: HoldConstraints {
            video: VideoSpec::new(frames(output_frames), rate(30), 512, 320).unwrap(),
            conditioning: match direction {
                ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
                ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
            },
            motion: MotionAmount::Still,
            instructions: None,
            region_target: None,
        },
        provider: Box::new(provider.clone()),
        plan: Box::new(plan),
    };
    (
        request,
        SelectedExtensionProvider::new(provider, capability),
        declaration,
    )
}

#[test]
fn both_directions_retain_the_exact_worker_binding_and_complete_native_count() {
    for direction in DIRECTIONS {
        for output_frames in [1, 20] {
            let (request, selected, declaration) = fixture(direction, output_frames);
            let binding = ExtensionGenerationBinding::from_request(&request).unwrap();
            binding.validate_for(&selected, &declaration).unwrap();
            assert_eq!(binding.plan.direction(), direction);
            assert_eq!(binding.plan.project_frames(), frames(output_frames));
            assert_eq!(
                declaration.video.frames().frames(),
                i64::from(
                    binding.plan.context_frame_count() + binding.plan.generated_frame_count()
                )
            );
            let mut request_wire = serde_json::to_value(&request).unwrap();
            for field in [
                "operation",
                "protocol",
                "cancellation_token",
                "output_workspace",
            ] {
                request_wire.as_object_mut().unwrap().remove(field).unwrap();
            }
            let wire = serde_json::to_value(&binding).unwrap();
            assert_eq!(wire, request_wire);
            let decoded: ExtensionGenerationBinding = serde_json::from_value(wire).unwrap();
            assert_eq!(decoded, binding);
            decoded.validate_for(&selected, &declaration).unwrap();
        }
    }
}

#[test]
fn provider_roundtrip_preserves_unaligned_count_limits_and_every_capability_fact() {
    let capability = ExtensionCapability::new(
        FrameRate::new(24000, 1001).unwrap(),
        17,
        FrameCountFormula::new(8, 0, 9, 25).unwrap(),
        DimensionLimits::new(
            AxisLimits::new(510, 1030, 64).unwrap(),
            AxisLimits::new(250, 520, 32).unwrap(),
        ),
        frames(47),
    )
    .unwrap();
    let selected = SelectedExtensionProvider::new(selection(), capability);
    let wire = serde_json::to_value(&selected).unwrap();
    assert_eq!(wire["generated_frame_counts"]["minimum"], 9);
    assert_eq!(wire["generated_frame_counts"]["maximum"], 25);
    assert_eq!(wire["maximum_output_frames"], 47);
    assert_eq!(wire["operation"], "extension");
    let decoded: SelectedExtensionProvider = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded, selected);
    assert_eq!(*decoded.capability(), capability);
    assert_eq!(decoded.capability().maximum_native_frame_count(), 41);
}

#[test]
fn wrong_operation_and_protocol_never_produce_an_extension_binding() {
    let (request, _, _) = fixture(ExtensionDirection::FromLeft, 20);
    let mut wrong = request.clone();
    if let HostMessage::GenerateExtension { protocol, .. } = &mut wrong {
        *protocol = ProtocolVersion::V2;
    }
    assert!(ExtensionGenerationBinding::from_request(&wrong).is_err());
    let HostMessage::GenerateExtension {
        identity,
        cancellation_token,
        project_id,
        revision_id,
        target,
        input,
        output_workspace,
        mut constraints,
        provider,
        ..
    } = request
    else {
        unreachable!()
    };
    let cancel = HostMessage::Cancel {
        protocol: ProtocolVersion::V3,
        identity: identity.clone(),
        cancellation_token: cancellation_token.clone(),
    };
    assert!(ExtensionGenerationBinding::from_request(&cancel).is_err());
    constraints.conditioning = ConditioningMode::Bridge;
    let plan = BridgeGenerationPlan::new(
        constraints.video.frames(),
        constraints.video.frame_rate(),
        &BridgeCapability::new(
            true,
            rate(24),
            FrameCountFormula::new(8, 1, 9, 33).unwrap(),
            capability().dimensions(),
        ),
        NativeDimensions::new(512, 320).unwrap(),
    )
    .unwrap();
    let bridge = HostMessage::GenerateBridge {
        protocol: ProtocolVersion::V2,
        identity,
        cancellation_token,
        project_id,
        revision_id,
        target,
        input,
        output_workspace,
        constraints,
        provider,
        plan: Box::new(plan),
    };
    bridge.validate().unwrap();
    assert!(ExtensionGenerationBinding::from_request(&bridge).is_err());
}

#[test]
fn constraints_must_match_direction_duration_rate_and_both_dimensions() {
    for direction in DIRECTIONS {
        let (request, selected, declaration) = fixture(direction, 20);
        let binding = ExtensionGenerationBinding::from_request(&request).unwrap();
        for video in [
            VideoSpec::new(frames(19), rate(30), 512, 320).unwrap(),
            VideoSpec::new(frames(20), rate(24), 512, 320).unwrap(),
            VideoSpec::new(frames(20), rate(30), 576, 320).unwrap(),
            VideoSpec::new(frames(20), rate(30), 512, 384).unwrap(),
        ] {
            let mut changed = binding.clone();
            changed.constraints.video = video;
            assert!(changed.validate_for(&selected, &declaration).is_err());
            assert!(
                serde_json::from_value::<ExtensionGenerationBinding>(
                    serde_json::to_value(changed).unwrap()
                )
                .is_err()
            );
        }
        for conditioning in [
            ConditioningMode::Bridge,
            match direction {
                ExtensionDirection::FromLeft => ConditioningMode::ExtendFromRight,
                ExtensionDirection::FromRight => ConditioningMode::ExtendFromLeft,
            },
        ] {
            let mut changed = binding.clone();
            changed.constraints.conditioning = conditioning;
            assert!(changed.validate().is_err());
        }
    }
}

#[test]
fn declaration_cannot_replace_provider_or_omit_context_from_native_count() {
    for direction in DIRECTIONS {
        let (request, selected, declaration) = fixture(direction, 20);
        let binding = ExtensionGenerationBinding::from_request(&request).unwrap();
        let full_count = declaration.video.frames();
        for video in [
            VideoSpec::new(
                frames(i64::from(binding.plan.generated_frame_count())),
                rate(24),
                512,
                320,
            )
            .unwrap(),
            VideoSpec::new(frames(full_count.frames() + 1), rate(24), 512, 320).unwrap(),
            VideoSpec::new(full_count, rate(30), 512, 320).unwrap(),
            VideoSpec::new(full_count, rate(24), 576, 320).unwrap(),
            VideoSpec::new(full_count, rate(24), 512, 384).unwrap(),
        ] {
            let mut changed = declaration.clone();
            changed.video = video;
            assert!(binding.validate_for(&selected, &changed).is_err());
        }
        let mut changed = declaration.clone();
        changed.provider.seed += 1;
        assert!(binding.validate_for(&selected, &changed).is_err());
        let mut changed = declaration.clone();
        changed.provenance = artifact(changed.native.reference().as_str(), 'b');
        assert!(binding.validate_for(&selected, &changed).is_err());
        let mut changed = selection();
        changed.runtime_version = RuntimeVersion::new("different").unwrap();
        assert!(
            binding
                .validate_for(
                    &SelectedExtensionProvider::new(changed, capability()),
                    &declaration
                )
                .is_err()
        );
    }
}

#[test]
fn independent_capability_must_reproduce_the_plan() {
    let (request, selected, declaration) = fixture(ExtensionDirection::FromRight, 20);
    let binding = ExtensionGenerationBinding::from_request(&request).unwrap();
    for (pointer, replacement) in [
        ("/context_frame_count", serde_json::json!(17)),
        ("/generated_frame_counts/minimum", serde_json::json!(24)),
        ("/native_frame_rate/numerator", serde_json::json!(30)),
        ("/width/minimum", serde_json::json!(576)),
        ("/height/minimum", serde_json::json!(384)),
        ("/maximum_output_frames", serde_json::json!(19)),
    ] {
        let mut wire = serde_json::to_value(&selected).unwrap();
        *wire.pointer_mut(pointer).unwrap() = replacement;
        let changed: SelectedExtensionProvider = serde_json::from_value(wire).unwrap();
        assert!(
            binding.validate_for(&changed, &declaration).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn strict_binding_wire_rejects_invalid_identity_version_plan_and_extra_controls() {
    let (request, _, _) = fixture(ExtensionDirection::FromLeft, 20);
    let binding = ExtensionGenerationBinding::from_request(&request).unwrap();
    for (pointer, replacement) in [
        ("/identity/request_id", serde_json::json!("")),
        ("/identity/attempt_id", serde_json::json!("wrong/path")),
        ("/project_id", serde_json::json!("")),
        ("/revision_id", serde_json::json!("")),
        ("/target/hold_id", serde_json::json!("")),
        ("/target/request_version", serde_json::json!(0)),
        ("/input/manifest", serde_json::json!("../outside")),
        ("/input/sha256", serde_json::json!("invalid")),
        ("/provider/pack_id", serde_json::json!("")),
        ("/plan/schema_version", serde_json::json!(2)),
        ("/plan/operation", serde_json::json!("bridge")),
    ] {
        let mut wire = serde_json::to_value(&binding).unwrap();
        *wire.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            serde_json::from_value::<ExtensionGenerationBinding>(wire).is_err(),
            "{pointer}"
        );
    }
    for field in [
        "protocol",
        "cancellation_token",
        "output_workspace",
        "opposite",
    ] {
        let mut wire = serde_json::to_value(&binding).unwrap();
        wire[field] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<ExtensionGenerationBinding>(wire).is_err(),
            "{field}"
        );
    }
    let mut wire = serde_json::to_value(&binding).unwrap();
    wire.as_object_mut().unwrap().remove("identity");
    assert!(serde_json::from_value::<ExtensionGenerationBinding>(wire).is_err());
}

#[test]
fn strict_provider_wire_rejects_unknown_missing_and_invalid_capability_fields() {
    let selected = SelectedExtensionProvider::new(selection(), capability());
    for (pointer, replacement) in [
        ("/schema_version", serde_json::json!(2)),
        ("/operation", serde_json::json!("bridge")),
        ("/context_frame_count", serde_json::json!(8)),
        ("/generated_frame_counts/step", serde_json::json!(4)),
        ("/generated_frame_counts/offset", serde_json::json!(1)),
        ("/generated_frame_counts/maximum", serde_json::json!(1)),
        ("/width/multiple", serde_json::json!(0)),
        ("/height/maximum", serde_json::json!(1)),
        ("/maximum_output_frames", serde_json::json!(0)),
    ] {
        let mut wire = serde_json::to_value(&selected).unwrap();
        *wire.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            serde_json::from_value::<SelectedExtensionProvider>(wire).is_err(),
            "{pointer}"
        );
    }
    for pointer in ["", "/generated_frame_counts", "/width", "/height"] {
        let mut wire = serde_json::to_value(&selected).unwrap();
        wire.pointer_mut(pointer).unwrap()["extra"] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<SelectedExtensionProvider>(wire).is_err(),
            "{pointer}"
        );
    }
    let mut wire = serde_json::to_value(&selected).unwrap();
    wire.as_object_mut()
        .unwrap()
        .remove("maximum_output_frames");
    assert!(serde_json::from_value::<SelectedExtensionProvider>(wire).is_err());
}
