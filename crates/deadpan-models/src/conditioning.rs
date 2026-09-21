//! Immutable retention of opaque bridge-conditioning inputs.
//!
//! This boundary proves containment and exact byte identity for a context
//! manifest and its two declared frame artifacts. It does not decode images,
//! validate source clocks, or establish color correctness.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{GeneratedContentId, GeneratedObjectRef};
use deadpan_jobs::artifact::{
    ArtifactError, ArtifactLimits, ArtifactWorkspace, HashedArtifactSnapshot, SnapshotInterruption,
};
use deadpan_jobs::{BridgeGenerationPlan, HostMessage, WorkspaceArtifact, WorkspaceRef};
use serde::{Deserialize, Serialize};

use crate::QualificationError;

const MAXIMUM_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAXIMUM_FRAME_BYTES: u64 = 64 * 1024 * 1024;
const MAXIMUM_TIMEOUT_MS: u64 = 24 * 60 * 60 * 1000;
const MAXIMUM_DESCRIPTION_BYTES: usize = 4096;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

/// Host-selected resource bounds for retaining bridge-conditioning inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConditioningLimitsWire")]
pub struct ConditioningLimits {
    pub maximum_manifest_bytes: u64,
    pub maximum_frame_bytes: u64,
    pub timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditioningLimitsWire {
    maximum_manifest_bytes: u64,
    maximum_frame_bytes: u64,
    timeout_ms: u64,
}

impl ConditioningLimits {
    pub fn new(
        maximum_manifest_bytes: u64,
        maximum_frame_bytes: u64,
        timeout_ms: u64,
    ) -> Result<Self, QualificationError> {
        let limits = Self {
            maximum_manifest_bytes,
            maximum_frame_bytes,
            timeout_ms,
        };
        limits.validate()?;
        Ok(limits)
    }

    pub fn validate(self) -> Result<(), QualificationError> {
        if self.maximum_manifest_bytes == 0
            || self.maximum_manifest_bytes > MAXIMUM_MANIFEST_BYTES
            || self.maximum_frame_bytes == 0
            || self.maximum_frame_bytes > MAXIMUM_FRAME_BYTES
            || self.timeout_ms == 0
            || self.timeout_ms > MAXIMUM_TIMEOUT_MS
        {
            return Err(conditioning_error("conditioning limits are out of bounds"));
        }
        Ok(())
    }
}

impl TryFrom<ConditioningLimitsWire> for ConditioningLimits {
    type Error = QualificationError;

    fn try_from(wire: ConditioningLimitsWire) -> Result<Self, Self::Error> {
        Self::new(
            wire.maximum_manifest_bytes,
            wire.maximum_frame_bytes,
            wire.timeout_ms,
        )
    }
}

/// Strict version-1 bridge context. The frame artifacts remain opaque bytes at
/// this layer; later media preparation owns their actual image semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BridgeContextWire")]
pub struct BridgeContext {
    schema_version: u32,
    model_color: String,
    plan: BridgeGenerationPlan,
    left: WorkspaceArtifact,
    right: WorkspaceArtifact,
    input_color_interpretation: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeContextWire {
    schema_version: u32,
    model_color: String,
    plan: BridgeGenerationPlan,
    left: WorkspaceArtifact,
    right: WorkspaceArtifact,
    input_color_interpretation: String,
}

impl BridgeContext {
    pub fn new(
        plan: BridgeGenerationPlan,
        left: WorkspaceArtifact,
        right: WorkspaceArtifact,
        input_color_interpretation: impl Into<String>,
    ) -> Result<Self, QualificationError> {
        let context = Self {
            schema_version: 1,
            model_color: "srgb".into(),
            plan,
            left,
            right,
            input_color_interpretation: input_color_interpretation.into(),
        };
        context.validate_shape()?;
        Ok(context)
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn model_color(&self) -> &str {
        &self.model_color
    }

    pub fn plan(&self) -> &BridgeGenerationPlan {
        &self.plan
    }

    pub fn left(&self) -> &WorkspaceArtifact {
        &self.left
    }

    pub fn right(&self) -> &WorkspaceArtifact {
        &self.right
    }

    pub fn input_color_interpretation(&self) -> &str {
        &self.input_color_interpretation
    }

    fn validate_shape(&self) -> Result<(), QualificationError> {
        if self.schema_version != 1
            || self.model_color != "srgb"
            || self.input_color_interpretation.trim().is_empty()
            || self.input_color_interpretation.len() > MAXIMUM_DESCRIPTION_BYTES
            || self.input_color_interpretation.contains('\0')
            || (self.left.reference() == self.right.reference() && self.left != self.right)
        {
            return Err(conditioning_error("invalid bridge context manifest"));
        }
        Ok(())
    }
}

impl TryFrom<BridgeContextWire> for BridgeContext {
    type Error = QualificationError;

    fn try_from(wire: BridgeContextWire) -> Result<Self, Self::Error> {
        let context = Self {
            schema_version: wire.schema_version,
            model_color: wire.model_color,
            plan: wire.plan,
            left: wire.left,
            right: wire.right,
            input_color_interpretation: wire.input_color_interpretation,
        };
        context.validate_shape()?;
        Ok(context)
    }
}

/// One retained immutable conditioning object and its worker declaration.
pub struct ConditioningObject {
    snapshot: HashedArtifactSnapshot,
    object: GeneratedObjectRef,
}

impl ConditioningObject {
    pub fn declaration(&self) -> &WorkspaceArtifact {
        self.snapshot.declaration()
    }

    pub fn object(&self) -> &GeneratedObjectRef {
        &self.object
    }
}

impl Read for ConditioningObject {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.snapshot.read(buffer)
    }
}

impl Seek for ConditioningObject {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.snapshot.seek(position)
    }
}

/// Serializable identity for one opaque retained conditioning object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConditioningArtifactReceiptWire")]
pub struct ConditioningArtifactReceipt {
    declaration: WorkspaceArtifact,
    object: GeneratedObjectRef,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditioningArtifactReceiptWire {
    declaration: WorkspaceArtifact,
    object: GeneratedObjectRef,
}

impl ConditioningArtifactReceipt {
    fn new(
        declaration: WorkspaceArtifact,
        object: GeneratedObjectRef,
    ) -> Result<Self, QualificationError> {
        if declaration.byte_length() != object.byte_length() {
            return Err(conditioning_error(
                "conditioning declaration and retained object lengths differ",
            ));
        }
        Ok(Self {
            declaration,
            object,
        })
    }

    pub fn declaration(&self) -> &WorkspaceArtifact {
        &self.declaration
    }

    pub fn object(&self) -> &GeneratedObjectRef {
        &self.object
    }
}

impl TryFrom<ConditioningArtifactReceiptWire> for ConditioningArtifactReceipt {
    type Error = QualificationError;

    fn try_from(wire: ConditioningArtifactReceiptWire) -> Result<Self, Self::Error> {
        Self::new(wire.declaration, wire.object)
    }
}

/// Immutable metadata for the retained manifest and two opaque frame inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConditioningReceiptWire")]
pub struct ConditioningReceipt {
    schema_version: u32,
    manifest: ConditioningArtifactReceipt,
    left: ConditioningArtifactReceipt,
    right: ConditioningArtifactReceipt,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditioningReceiptWire {
    schema_version: u32,
    manifest: ConditioningArtifactReceipt,
    left: ConditioningArtifactReceipt,
    right: ConditioningArtifactReceipt,
}

impl ConditioningReceipt {
    fn new(
        manifest: ConditioningArtifactReceipt,
        left: ConditioningArtifactReceipt,
        right: ConditioningArtifactReceipt,
    ) -> Result<Self, QualificationError> {
        let receipt = Self {
            schema_version: 1,
            manifest,
            left,
            right,
        };
        receipt.validate_shape()?;
        Ok(receipt)
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn manifest(&self) -> &ConditioningArtifactReceipt {
        &self.manifest
    }

    pub fn left(&self) -> &ConditioningArtifactReceipt {
        &self.left
    }

    pub fn right(&self) -> &ConditioningArtifactReceipt {
        &self.right
    }

    fn validate_shape(&self) -> Result<(), QualificationError> {
        if self.schema_version != 1
            || self.manifest.declaration.reference() == self.left.declaration.reference()
            || self.manifest.declaration.reference() == self.right.declaration.reference()
            || (self.left.declaration.reference() == self.right.declaration.reference()
                && self.left != self.right)
        {
            return Err(conditioning_error("invalid conditioning receipt"));
        }
        Ok(())
    }
}

impl TryFrom<ConditioningReceiptWire> for ConditioningReceipt {
    type Error = QualificationError;

    fn try_from(wire: ConditioningReceiptWire) -> Result<Self, Self::Error> {
        if wire.schema_version != 1 {
            return Err(conditioning_error(
                "unsupported conditioning receipt schema",
            ));
        }
        Self::new(wire.manifest, wire.left, wire.right)
    }
}

/// Three immutable snapshots retained from one strict bridge context.
pub struct RetainedConditioning {
    manifest: ConditioningObject,
    left: ConditioningObject,
    right: ConditioningObject,
    context: BridgeContext,
    receipt: ConditioningReceipt,
}

impl RetainedConditioning {
    pub fn manifest(&self) -> &ConditioningObject {
        &self.manifest
    }

    pub fn left(&self) -> &ConditioningObject {
        &self.left
    }

    pub fn right(&self) -> &ConditioningObject {
        &self.right
    }

    pub fn context(&self) -> &BridgeContext {
        &self.context
    }

    pub fn receipt(&self) -> &ConditioningReceipt {
        &self.receipt
    }

    pub fn validate_for(&self, request: &HostMessage) -> Result<(), QualificationError> {
        request
            .validate()
            .map_err(|error| conditioning_error(&error.to_string()))?;
        let HostMessage::GenerateBridge { input, plan, .. } = request else {
            return Err(conditioning_error(
                "retained conditioning requires a version-2 bridge request",
            ));
        };
        if self.manifest.declaration().reference() != &input.manifest
            || self.manifest.declaration().sha256() != &input.sha256
            || self.context.plan() != plan.as_ref()
            || self.context.left() != self.left.declaration()
            || self.context.right() != self.right.declaration()
            || self.receipt.manifest.declaration() != self.manifest.declaration()
            || self.receipt.manifest.object() != self.manifest.object()
            || self.receipt.left.declaration() != self.left.declaration()
            || self.receipt.left.object() != self.left.object()
            || self.receipt.right.declaration() != self.right.declaration()
            || self.receipt.right.object() != self.right.object()
        {
            return Err(conditioning_error(
                "retained conditioning differs from the bridge request",
            ));
        }
        Ok(())
    }

    pub fn into_parts(self) -> (ConditioningObject, ConditioningObject, ConditioningObject) {
        (self.manifest, self.left, self.right)
    }
}

/// Retain a strict bridge context and its two opaque frame inputs.
///
/// The host must pin `workspace` before worker execution. This function runs
/// outside database transactions and does not claim the frame bytes are valid
/// images or that their color/source-clock descriptions are true.
pub fn capture_bridge_conditioning(
    workspace: &ArtifactWorkspace,
    request: &HostMessage,
    manifest_declaration: &WorkspaceArtifact,
    input_scope: &WorkspaceRef,
    limits: ConditioningLimits,
    cancelled: &AtomicBool,
) -> Result<RetainedConditioning, QualificationError> {
    limits.validate()?;
    request
        .validate()
        .map_err(|error| conditioning_error(&error.to_string()))?;
    let HostMessage::GenerateBridge {
        input,
        output_workspace,
        plan,
        ..
    } = request
    else {
        return Err(conditioning_error(
            "conditioning capture requires a version-2 bridge request",
        ));
    };
    if manifest_declaration.reference() != &input.manifest
        || manifest_declaration.sha256() != &input.sha256
    {
        return Err(conditioning_error(
            "context manifest declaration differs from the bridge request",
        ));
    }
    if scopes_overlap(input_scope, output_workspace) {
        return Err(conditioning_error(
            "input and output workspace scopes must be disjoint",
        ));
    }

    let deadline = Instant::now()
        .checked_add(Duration::from_millis(limits.timeout_ms))
        .ok_or(QualificationError::Deadline)?;
    check_control(cancelled, deadline)?;
    let manifest_snapshot = workspace
        .snapshot_with_control(
            input_scope,
            manifest_declaration,
            ArtifactLimits::new(limits.maximum_manifest_bytes)?,
            || snapshot_control(cancelled, deadline),
        )
        .map_err(map_snapshot_error)?;
    let (manifest, manifest_bytes) = retain_object(manifest_snapshot, true, cancelled, deadline)?;
    let manifest_bytes = manifest_bytes.expect("manifest collection was requested");
    let context: BridgeContext =
        serde_json::from_value(crate::strict_json::parse(&manifest_bytes)?)?;
    check_control(cancelled, deadline)?;
    if context.plan() != plan.as_ref()
        || context.left().reference() == manifest_declaration.reference()
        || context.right().reference() == manifest_declaration.reference()
    {
        return Err(conditioning_error(
            "context manifest differs from the bridge request or aliases itself",
        ));
    }

    let left_snapshot = workspace
        .snapshot_with_control(
            input_scope,
            context.left(),
            ArtifactLimits::new(limits.maximum_frame_bytes)?,
            || snapshot_control(cancelled, deadline),
        )
        .map_err(map_snapshot_error)?;
    let (left, _) = retain_object(left_snapshot, false, cancelled, deadline)?;
    let right_snapshot = workspace
        .snapshot_with_control(
            input_scope,
            context.right(),
            ArtifactLimits::new(limits.maximum_frame_bytes)?,
            || snapshot_control(cancelled, deadline),
        )
        .map_err(map_snapshot_error)?;
    let (right, _) = retain_object(right_snapshot, false, cancelled, deadline)?;
    let receipt = ConditioningReceipt::new(
        ConditioningArtifactReceipt::new(
            manifest.declaration().clone(),
            manifest.object().clone(),
        )?,
        ConditioningArtifactReceipt::new(left.declaration().clone(), left.object().clone())?,
        ConditioningArtifactReceipt::new(right.declaration().clone(), right.object().clone())?,
    )?;
    let retained = RetainedConditioning {
        manifest,
        left,
        right,
        context,
        receipt,
    };
    retained.validate_for(request)?;
    check_control(cancelled, deadline)?;
    Ok(retained)
}

fn retain_object(
    mut snapshot: HashedArtifactSnapshot,
    collect: bool,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(ConditioningObject, Option<Vec<u8>>), QualificationError> {
    let declaration = snapshot.declaration().clone();
    let mut bytes = collect.then(|| {
        Vec::with_capacity(
            usize::try_from(declaration.byte_length())
                .expect("collected manifest size is bounded to one MiB"),
        )
    });
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        check_control(cancelled, deadline)?;
        let read = snapshot.read(&mut buffer)?;
        check_control(cancelled, deadline)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        if let Some(bytes) = bytes.as_mut() {
            bytes.extend_from_slice(&buffer[..read]);
        }
    }
    check_control(cancelled, deadline)?;
    snapshot.seek(SeekFrom::Start(0))?;
    check_control(cancelled, deadline)?;
    let object = GeneratedObjectRef::new(
        GeneratedContentId::new(hasher.finalize().to_hex().to_string())
            .map_err(|error| conditioning_error(&error.to_string()))?,
        declaration.byte_length(),
    )
    .map_err(|error| conditioning_error(&error.to_string()))?;
    Ok((ConditioningObject { snapshot, object }, bytes))
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), QualificationError> {
    if cancelled.load(Ordering::Acquire) {
        Err(QualificationError::Cancelled)
    } else if Instant::now() >= deadline {
        Err(QualificationError::Deadline)
    } else {
        Ok(())
    }
}

fn snapshot_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), SnapshotInterruption> {
    if cancelled.load(Ordering::Acquire) {
        Err(SnapshotInterruption::Cancelled)
    } else if Instant::now() >= deadline {
        Err(SnapshotInterruption::Deadline)
    } else {
        Ok(())
    }
}

fn map_snapshot_error(error: ArtifactError) -> QualificationError {
    match error {
        ArtifactError::Interrupted(SnapshotInterruption::Cancelled) => {
            QualificationError::Cancelled
        }
        ArtifactError::Interrupted(SnapshotInterruption::Deadline) => QualificationError::Deadline,
        other => QualificationError::Artifact(other),
    }
}

fn scopes_overlap(left: &WorkspaceRef, right: &WorkspaceRef) -> bool {
    is_component_prefix(left, right) || is_component_prefix(right, left)
}

fn is_component_prefix(prefix: &WorkspaceRef, candidate: &WorkspaceRef) -> bool {
    let prefix: Vec<_> = prefix.as_str().split('/').collect();
    let candidate: Vec<_> = candidate.as_str().split('/').collect();
    prefix.len() <= candidate.len() && prefix == candidate[..prefix.len()]
}

fn conditioning_error(message: &str) -> QualificationError {
    QualificationError::Request(format!("invalid bridge conditioning: {message}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;
    use std::os::unix::fs::symlink;

    use deadpan_core::{FrameDuration, FrameRate, NodeId, ProjectId, RevisionId};
    use deadpan_jobs::{
        AttemptId, AxisLimits, BridgeCapability, CancellationToken, ConditioningMode,
        ContextArtifact, DimensionLimits, FrameCountFormula, HoldConstraints, HoldTarget,
        MessageIdentity, MotionAmount, NativeDimensions, ProtocolVersion, ProviderPackId,
        ProviderPackVersion, ProviderSelection, RequestId, RequestVersion, RuntimeId,
        RuntimeVersion, Sha256, VideoSpec,
    };
    use sha2::{Digest, Sha256 as Sha256Hasher};

    use super::*;

    fn plan() -> BridgeGenerationPlan {
        BridgeGenerationPlan::new(
            FrameDuration::new(3).unwrap(),
            FrameRate::new(30, 1).unwrap(),
            &BridgeCapability::new(
                true,
                FrameRate::new(24, 1).unwrap(),
                FrameCountFormula::new(1, 0, 2, 97).unwrap(),
                DimensionLimits::new(
                    AxisLimits::new(4, 4, 1).unwrap(),
                    AxisLimits::new(2, 2, 1).unwrap(),
                ),
            ),
            NativeDimensions::new(4, 2).unwrap(),
        )
        .unwrap()
    }

    fn different_valid_plan() -> BridgeGenerationPlan {
        BridgeGenerationPlan::new(
            FrameDuration::new(3).unwrap(),
            FrameRate::new(30, 1).unwrap(),
            &BridgeCapability::new(
                true,
                FrameRate::new(30, 1).unwrap(),
                FrameCountFormula::new(1, 0, 2, 97).unwrap(),
                DimensionLimits::new(
                    AxisLimits::new(4, 4, 1).unwrap(),
                    AxisLimits::new(2, 2, 1).unwrap(),
                ),
            ),
            NativeDimensions::new(4, 2).unwrap(),
        )
        .unwrap()
    }

    fn declaration(reference: &str, bytes: &[u8]) -> WorkspaceArtifact {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut digest = String::with_capacity(64);
        for byte in Sha256Hasher::digest(bytes) {
            digest.push(char::from(HEX[usize::from(byte >> 4)]));
            digest.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        WorkspaceArtifact::new(
            WorkspaceRef::new(reference).unwrap(),
            Sha256::new(digest).unwrap(),
            bytes.len() as u64,
        )
        .unwrap()
    }

    fn request(manifest: &WorkspaceArtifact, output: &str) -> HostMessage {
        HostMessage::GenerateBridge {
            protocol: ProtocolVersion::V2,
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
            output_workspace: WorkspaceRef::new(output).unwrap(),
            constraints: HoldConstraints {
                video: VideoSpec::new(
                    FrameDuration::new(3).unwrap(),
                    FrameRate::new(30, 1).unwrap(),
                    4,
                    2,
                )
                .unwrap(),
                conditioning: ConditioningMode::Bridge,
                motion: MotionAmount::Still,
            },
            provider: Box::new(ProviderSelection {
                pack_id: ProviderPackId::new("pack").unwrap(),
                pack_version: ProviderPackVersion::new("1").unwrap(),
                runtime_id: RuntimeId::new("runtime").unwrap(),
                runtime_version: RuntimeVersion::new("1").unwrap(),
                seed: 1,
            }),
            plan: Box::new(plan()),
        }
    }

    fn limits() -> ConditioningLimits {
        ConditioningLimits::new(1024 * 1024, 64 * 1024 * 1024, 30_000).unwrap()
    }

    struct Fixture {
        directory: tempfile::TempDir,
        manifest: WorkspaceArtifact,
        left: WorkspaceArtifact,
        right: WorkspaceArtifact,
        left_bytes: Vec<u8>,
        right_bytes: Vec<u8>,
    }

    impl Fixture {
        fn new(shared_frames: bool) -> Self {
            let directory = tempfile::tempdir().unwrap();
            fs::create_dir(directory.path().join("inputs")).unwrap();
            fs::create_dir(directory.path().join("outputs")).unwrap();
            let left_bytes = b"opaque-left-frame".to_vec();
            let right_bytes = if shared_frames {
                left_bytes.clone()
            } else {
                b"opaque-right-frame".to_vec()
            };
            fs::write(directory.path().join("inputs/left.bin"), &left_bytes).unwrap();
            if !shared_frames {
                fs::write(directory.path().join("inputs/right.bin"), &right_bytes).unwrap();
            }
            let left = declaration("inputs/left.bin", &left_bytes);
            let right = if shared_frames {
                left.clone()
            } else {
                declaration("inputs/right.bin", &right_bytes)
            };
            let context = BridgeContext::new(
                plan(),
                left.clone(),
                right.clone(),
                "opaque prepared sRGB input",
            )
            .unwrap();
            let bytes = serde_json::to_vec(&context).unwrap();
            fs::write(directory.path().join("inputs/context.json"), &bytes).unwrap();
            let manifest = declaration("inputs/context.json", &bytes);
            Self {
                directory,
                manifest,
                left,
                right,
                left_bytes,
                right_bytes,
            }
        }

        fn workspace(&self) -> ArtifactWorkspace {
            ArtifactWorkspace::open(self.directory.path()).unwrap()
        }

        fn request(&self) -> HostMessage {
            request(&self.manifest, "outputs")
        }
    }

    #[test]
    fn capture_freezes_exact_bytes_and_receipt_bindings() {
        let fixture = Fixture::new(false);
        let request = fixture.request();
        let retained = capture_bridge_conditioning(
            &fixture.workspace(),
            &request,
            &fixture.manifest,
            &WorkspaceRef::new("inputs").unwrap(),
            limits(),
            &AtomicBool::new(false),
        )
        .unwrap();
        retained.validate_for(&request).unwrap();
        assert_eq!(retained.context().left(), &fixture.left);
        assert_eq!(retained.context().right(), &fixture.right);
        assert_eq!(retained.receipt().schema_version(), 1);
        assert_eq!(
            retained.left().object().content().digest(),
            blake3::hash(&fixture.left_bytes).to_hex().as_str()
        );
        let receipt_json = serde_json::to_value(retained.receipt()).unwrap();
        let receipt: ConditioningReceipt = serde_json::from_value(receipt_json).unwrap();
        assert_eq!(&receipt, retained.receipt());

        fs::write(
            fixture.directory.path().join("inputs/left.bin"),
            b"later mutation",
        )
        .unwrap();
        let (_, mut left, mut right) = retained.into_parts();
        let mut observed = Vec::new();
        left.read_to_end(&mut observed).unwrap();
        assert_eq!(observed, fixture.left_bytes);
        observed.clear();
        right.read_to_end(&mut observed).unwrap();
        assert_eq!(observed, fixture.right_bytes);
    }

    #[test]
    fn strict_manifest_binding_plan_and_scope_fail_before_retention() {
        let fixture = Fixture::new(false);
        let workspace = fixture.workspace();
        let input_scope = WorkspaceRef::new("inputs").unwrap();
        let cancelled = AtomicBool::new(false);
        let wrong_manifest = declaration("inputs/elsewhere.json", b"elsewhere");
        assert!(matches!(
            capture_bridge_conditioning(
                &workspace,
                &fixture.request(),
                &wrong_manifest,
                &input_scope,
                limits(),
                &cancelled,
            ),
            Err(QualificationError::Request(_))
        ));

        for output in ["inputs", "inputs/output"] {
            assert!(matches!(
                capture_bridge_conditioning(
                    &workspace,
                    &request(&fixture.manifest, output),
                    &fixture.manifest,
                    &input_scope,
                    limits(),
                    &cancelled,
                ),
                Err(QualificationError::Request(_))
            ));
        }
        assert!(
            capture_bridge_conditioning(
                &workspace,
                &request(&fixture.manifest, "inputs2"),
                &fixture.manifest,
                &input_scope,
                limits(),
                &cancelled,
            )
            .is_ok()
        );
        assert!(matches!(
            capture_bridge_conditioning(
                &workspace,
                &request(&fixture.manifest, "inputs"),
                &fixture.manifest,
                &WorkspaceRef::new("inputs/context").unwrap(),
                limits(),
                &cancelled,
            ),
            Err(QualificationError::Request(_))
        ));

        let different = BridgeContext::new(
            different_valid_plan(),
            fixture.left.clone(),
            fixture.right.clone(),
            "opaque",
        )
        .unwrap();
        let malformed = serde_json::to_vec(&different).unwrap();
        fs::write(
            fixture.directory.path().join("inputs/context.json"),
            &malformed,
        )
        .unwrap();
        let malformed_declaration = declaration("inputs/context.json", &malformed);
        assert!(matches!(
            capture_bridge_conditioning(
                &workspace,
                &request(&malformed_declaration, "outputs"),
                &malformed_declaration,
                &input_scope,
                limits(),
                &cancelled,
            ),
            Err(QualificationError::Request(_))
        ));
    }

    #[test]
    fn cancellation_limits_and_duplicate_json_are_rejected() {
        for invalid in [
            serde_json::json!({"maximum_manifest_bytes":0,"maximum_frame_bytes":1,"timeout_ms":1}),
            serde_json::json!({"maximum_manifest_bytes":1,"maximum_frame_bytes":67108865_u64,"timeout_ms":1}),
            serde_json::json!({"maximum_manifest_bytes":1,"maximum_frame_bytes":1,"timeout_ms":86400001_u64}),
            serde_json::json!({"maximum_manifest_bytes":1,"maximum_frame_bytes":1,"timeout_ms":1,"extra":true}),
        ] {
            assert!(serde_json::from_value::<ConditioningLimits>(invalid).is_err());
        }
        let fixture = Fixture::new(false);
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            capture_bridge_conditioning(
                &fixture.workspace(),
                &fixture.request(),
                &fixture.manifest,
                &WorkspaceRef::new("inputs").unwrap(),
                limits(),
                &cancelled,
            ),
            Err(QualificationError::Cancelled)
        ));

        let duplicate = format!(
            r#"{{"schema_version":1,"schema_version":1,"model_color":"srgb","plan":{},"left":{},"right":{},"input_color_interpretation":"opaque"}}"#,
            serde_json::to_string(&plan()).unwrap(),
            serde_json::to_string(&fixture.left).unwrap(),
            serde_json::to_string(&fixture.right).unwrap(),
        );
        fs::write(
            fixture.directory.path().join("inputs/context.json"),
            duplicate.as_bytes(),
        )
        .unwrap();
        let duplicate_declaration = declaration("inputs/context.json", duplicate.as_bytes());
        assert!(matches!(
            capture_bridge_conditioning(
                &fixture.workspace(),
                &request(&duplicate_declaration, "outputs"),
                &duplicate_declaration,
                &WorkspaceRef::new("inputs").unwrap(),
                limits(),
                &AtomicBool::new(false),
            ),
            Err(QualificationError::Json(_))
        ));
    }

    #[test]
    fn symlink_hardlink_and_outside_frame_are_rejected() {
        for mode in ["symlink", "hardlink", "outside"] {
            let fixture = Fixture::new(false);
            let context_path = fixture.directory.path().join("inputs/context.json");
            let mut context = BridgeContext::new(
                plan(),
                fixture.left.clone(),
                fixture.right.clone(),
                "opaque",
            )
            .unwrap();
            match mode {
                "symlink" => {
                    let left_path = fixture.directory.path().join("inputs/left.bin");
                    fs::remove_file(&left_path).unwrap();
                    symlink("right.bin", left_path).unwrap();
                }
                "hardlink" => {
                    fs::hard_link(
                        fixture.directory.path().join("inputs/left.bin"),
                        fixture.directory.path().join("inputs/alias.bin"),
                    )
                    .unwrap();
                }
                "outside" => {
                    fs::write(fixture.directory.path().join("outside.bin"), b"outside").unwrap();
                    context.left = declaration("outside.bin", b"outside");
                    fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
                }
                _ => unreachable!(),
            }
            let manifest_bytes = fs::read(&context_path).unwrap();
            let manifest = declaration("inputs/context.json", &manifest_bytes);
            let Err(error) = capture_bridge_conditioning(
                &fixture.workspace(),
                &request(&manifest, "outputs"),
                &manifest,
                &WorkspaceRef::new("inputs").unwrap(),
                limits(),
                &AtomicBool::new(false),
            ) else {
                panic!("{mode} fixture must be rejected")
            };
            assert!(matches!(
                (mode, error),
                (
                    "symlink",
                    QualificationError::Artifact(ArtifactError::UnsafeComponent(_))
                ) | (
                    "hardlink",
                    QualificationError::Artifact(ArtifactError::MultipleLinks(_))
                ) | (
                    "outside",
                    QualificationError::Artifact(ArtifactError::OutsideOutputScope { .. }),
                )
            ));
        }
    }

    #[test]
    fn identical_left_and_right_declarations_deduplicate_content_identity() {
        let fixture = Fixture::new(true);
        let retained = capture_bridge_conditioning(
            &fixture.workspace(),
            &fixture.request(),
            &fixture.manifest,
            &WorkspaceRef::new("inputs").unwrap(),
            limits(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            retained.left().declaration(),
            retained.right().declaration()
        );
        assert_eq!(retained.left().object(), retained.right().object());
        assert_eq!(retained.receipt().left(), retained.receipt().right());
    }

    #[test]
    fn context_and_receipt_checked_deserialization_rejects_forgery() {
        let fixture = Fixture::new(false);
        let context = BridgeContext::new(
            plan(),
            fixture.left.clone(),
            fixture.right.clone(),
            "opaque",
        )
        .unwrap();
        let mut wire = serde_json::to_value(&context).unwrap();
        wire["model_color"] = serde_json::json!("display-p3");
        assert!(serde_json::from_value::<BridgeContext>(wire).is_err());
        let mut mismatched_shared = serde_json::to_value(&context).unwrap();
        mismatched_shared["right"] = mismatched_shared["left"].clone();
        mismatched_shared["right"]["sha256"] = serde_json::json!("f".repeat(64));
        assert!(serde_json::from_value::<BridgeContext>(mismatched_shared).is_err());

        let retained = capture_bridge_conditioning(
            &fixture.workspace(),
            &fixture.request(),
            &fixture.manifest,
            &WorkspaceRef::new("inputs").unwrap(),
            limits(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let mut receipt = serde_json::to_value(retained.receipt()).unwrap();
        receipt["schema_version"] = serde_json::json!(2);
        assert!(serde_json::from_value::<ConditioningReceipt>(receipt).is_err());
    }

    #[test]
    fn declaration_hash_length_and_manifest_alias_mismatches_fail() {
        let fixture = Fixture::new(false);
        let workspace = fixture.workspace();
        let scope = WorkspaceRef::new("inputs").unwrap();
        let wrong_hash = WorkspaceArtifact::new(
            fixture.manifest.reference().clone(),
            Sha256::new("f".repeat(64)).unwrap(),
            fixture.manifest.byte_length(),
        )
        .unwrap();
        assert!(matches!(
            capture_bridge_conditioning(
                &workspace,
                &request(&wrong_hash, "outputs"),
                &wrong_hash,
                &scope,
                limits(),
                &AtomicBool::new(false),
            ),
            Err(QualificationError::Artifact(
                ArtifactError::HashMismatch { .. }
            ))
        ));

        let wrong_length = WorkspaceArtifact::new(
            fixture.manifest.reference().clone(),
            fixture.manifest.sha256().clone(),
            fixture.manifest.byte_length() + 1,
        )
        .unwrap();
        assert!(matches!(
            capture_bridge_conditioning(
                &workspace,
                &request(&wrong_length, "outputs"),
                &wrong_length,
                &scope,
                limits(),
                &AtomicBool::new(false),
            ),
            Err(QualificationError::Artifact(
                ArtifactError::LengthMismatch { .. }
            ))
        ));

        let alias_context = BridgeContext::new(
            plan(),
            fixture.manifest.clone(),
            fixture.right.clone(),
            "opaque",
        )
        .unwrap();
        let bytes = serde_json::to_vec(&alias_context).unwrap();
        fs::write(fixture.directory.path().join("inputs/context.json"), &bytes).unwrap();
        let manifest = declaration("inputs/context.json", &bytes);
        assert!(matches!(
            capture_bridge_conditioning(
                &workspace,
                &request(&manifest, "outputs"),
                &manifest,
                &scope,
                limits(),
                &AtomicBool::new(false),
            ),
            Err(QualificationError::Request(_))
        ));
    }
}
