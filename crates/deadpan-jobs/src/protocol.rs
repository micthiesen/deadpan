use std::fmt;
use std::io::{self, Read, Write};

use deadpan_core::{FrameDuration, FrameRate, NodeId, ProjectId, RevisionId};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

use crate::generation_plan::BridgeGenerationPlan;

pub const PROTOCOL_VERSION: u32 = 1;
pub const BRIDGE_PROTOCOL_VERSION: u32 = 2;
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_PROTOCOL_ID_BYTES: usize = 128;
pub const MAX_WORKSPACE_REF_BYTES: usize = 1_024;
pub const MAX_DIAGNOSTIC_BYTES: usize = 4_096;
pub const MAX_VIDEO_DIMENSION: u32 = 32_768;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub enum ProtocolVersion {
    V1,
    V2,
}

impl TryFrom<u32> for ProtocolVersion {
    type Error = ValueError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            PROTOCOL_VERSION => Ok(Self::V1),
            BRIDGE_PROTOCOL_VERSION => Ok(Self::V2),
            value => Err(ValueError::UnsupportedProtocol(value)),
        }
    }
}

impl From<ProtocolVersion> for u32 {
    fn from(value: ProtocolVersion) -> Self {
        match value {
            ProtocolVersion::V1 => PROTOCOL_VERSION,
            ProtocolVersion::V2 => BRIDGE_PROTOCOL_VERSION,
        }
    }
}

macro_rules! protocol_identifier {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > MAX_PROTOCOL_ID_BYTES
                    || !value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+')
                    })
                {
                    return Err(ValueError::InvalidIdentifier($label));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = ValueError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

protocol_identifier!(RequestId, "request ID");
protocol_identifier!(AttemptId, "attempt ID");
protocol_identifier!(CancellationToken, "cancellation token");
protocol_identifier!(ProviderPackId, "provider pack ID");
protocol_identifier!(ProviderPackVersion, "provider pack version");
protocol_identifier!(RuntimeId, "runtime ID");
protocol_identifier!(RuntimeVersion, "runtime version");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct RequestVersion(u64);

impl RequestVersion {
    pub fn new(value: u64) -> Result<Self, ValueError> {
        if value == 0 {
            return Err(ValueError::ZeroRequestVersion);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for RequestVersion {
    type Error = ValueError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<RequestVersion> for u64 {
    fn from(value: RequestVersion) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256(String);

impl Sha256 {
    pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ValueError::InvalidSha256);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Sha256 {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Sha256> for String {
    fn from(value: Sha256) -> Self {
        value.0
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A lexical POSIX path relative to a supervisor-owned workspace.
///
/// This validation grants no filesystem authority. The process supervisor must
/// still resolve the reference beneath its workspace and reject symlink escapes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkspaceRef(String);

impl WorkspaceRef {
    pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_WORKSPACE_REF_BYTES
            || value.starts_with('/')
            || value.contains('\0')
            || value.contains('\\')
            || value
                .split('/')
                .any(|component| component.is_empty() || matches!(component, "." | ".."))
        {
            return Err(ValueError::InvalidWorkspaceRef);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WorkspaceRef {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<WorkspaceRef> for String {
    fn from(value: WorkspaceRef) -> Self {
        value.0
    }
}

impl fmt::Display for WorkspaceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Diagnostic(String);

impl Diagnostic {
    pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_DIAGNOSTIC_BYTES || value.contains('\0') {
            return Err(ValueError::InvalidDiagnostic);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Diagnostic {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Diagnostic> for String {
    fn from(value: Diagnostic) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageIdentity {
    pub request_id: RequestId,
    pub attempt_id: AttemptId,
}

impl MessageIdentity {
    pub const fn new(request_id: RequestId, attempt_id: AttemptId) -> Self {
        Self {
            request_id,
            attempt_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldTarget {
    pub hold_id: NodeId,
    pub request_version: RequestVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextArtifact {
    pub manifest: WorkspaceRef,
    pub sha256: Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "VideoSpecWire")]
pub struct VideoSpec {
    frames: FrameDuration,
    frame_rate: FrameRate,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VideoSpecWire {
    frames: FrameDuration,
    frame_rate: FrameRate,
    width: u32,
    height: u32,
}

impl VideoSpec {
    pub fn new(
        frames: FrameDuration,
        frame_rate: FrameRate,
        width: u32,
        height: u32,
    ) -> Result<Self, ValueError> {
        if frames.frames() == 0 {
            return Err(ValueError::ZeroFrames);
        }
        if width == 0 || height == 0 || width > MAX_VIDEO_DIMENSION || height > MAX_VIDEO_DIMENSION
        {
            return Err(ValueError::InvalidDimensions {
                width,
                height,
                max: MAX_VIDEO_DIMENSION,
            });
        }
        Ok(Self {
            frames,
            frame_rate,
            width,
            height,
        })
    }

    pub const fn frames(&self) -> FrameDuration {
        self.frames
    }

    pub const fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }
}

impl TryFrom<VideoSpecWire> for VideoSpec {
    type Error = ValueError;

    fn try_from(value: VideoSpecWire) -> Result<Self, Self::Error> {
        Self::new(value.frames, value.frame_rate, value.width, value.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditioningMode {
    Bridge,
    ExtendFromLeft,
    ExtendFromRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionAmount {
    Still,
    Subtle,
    Moderate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldConstraints {
    pub video: VideoSpec,
    pub conditioning: ConditioningMode,
    pub motion: MotionAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSelection {
    pub pack_id: ProviderPackId,
    pub pack_version: ProviderPackVersion,
    pub runtime_id: RuntimeId,
    pub runtime_version: RuntimeVersion,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WorkspaceArtifactWire")]
pub struct WorkspaceArtifact {
    reference: WorkspaceRef,
    sha256: Sha256,
    byte_length: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceArtifactWire {
    reference: WorkspaceRef,
    sha256: Sha256,
    byte_length: u64,
}

impl WorkspaceArtifact {
    pub fn new(
        reference: WorkspaceRef,
        sha256: Sha256,
        byte_length: u64,
    ) -> Result<Self, ValueError> {
        if byte_length == 0 {
            return Err(ValueError::EmptyArtifact);
        }
        Ok(Self {
            reference,
            sha256,
            byte_length,
        })
    }

    pub fn reference(&self) -> &WorkspaceRef {
        &self.reference
    }

    pub fn sha256(&self) -> &Sha256 {
        &self.sha256
    }

    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }
}

impl TryFrom<WorkspaceArtifactWire> for WorkspaceArtifact {
    type Error = ValueError;

    fn try_from(value: WorkspaceArtifactWire) -> Result<Self, Self::Error> {
        Self::new(value.reference, value.sha256, value.byte_length)
    }
}

/// Worker-declared candidate metadata. It is not trusted proof of valid media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateManifest {
    pub media: WorkspaceArtifact,
    pub video: VideoSpec,
    pub provider: ProviderSelection,
}

/// Worker-declared native bridge sequence and provenance. Both artifacts remain
/// untrusted until the host snapshots, hashes, decodes, and validates them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "NativeCandidateManifestWire")]
pub struct NativeCandidateManifest {
    pub native: WorkspaceArtifact,
    pub provenance: WorkspaceArtifact,
    pub video: VideoSpec,
    pub provider: ProviderSelection,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeCandidateManifestWire {
    native: WorkspaceArtifact,
    provenance: WorkspaceArtifact,
    video: VideoSpec,
    provider: ProviderSelection,
}

impl NativeCandidateManifest {
    pub fn validate(&self) -> Result<(), ValueError> {
        if self.native.reference() == self.provenance.reference() {
            return Err(ValueError::DuplicateArtifactReference);
        }
        Ok(())
    }
}

impl TryFrom<NativeCandidateManifestWire> for NativeCandidateManifest {
    type Error = ValueError;

    fn try_from(value: NativeCandidateManifestWire) -> Result<Self, Self::Error> {
        let manifest = Self {
            native: value.native,
            provenance: value.provenance,
            video: value.video,
            provider: value.provider,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    GenerateHold {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
        cancellation_token: CancellationToken,
        project_id: ProjectId,
        revision_id: RevisionId,
        target: HoldTarget,
        input: ContextArtifact,
        output_workspace: WorkspaceRef,
        constraints: HoldConstraints,
        provider: Box<ProviderSelection>,
    },
    GenerateBridge {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
        cancellation_token: CancellationToken,
        project_id: ProjectId,
        revision_id: RevisionId,
        target: HoldTarget,
        input: ContextArtifact,
        output_workspace: WorkspaceRef,
        constraints: HoldConstraints,
        provider: Box<ProviderSelection>,
        plan: Box<BridgeGenerationPlan>,
    },
    Cancel {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
        cancellation_token: CancellationToken,
    },
}

impl HostMessage {
    pub fn identity(&self) -> &MessageIdentity {
        match self {
            Self::GenerateHold { identity, .. }
            | Self::GenerateBridge { identity, .. }
            | Self::Cancel { identity, .. } => identity,
        }
    }

    pub const fn protocol(&self) -> ProtocolVersion {
        match self {
            Self::GenerateHold { protocol, .. }
            | Self::GenerateBridge { protocol, .. }
            | Self::Cancel { protocol, .. } => *protocol,
        }
    }

    pub fn validate(&self) -> Result<(), ValueError> {
        match self {
            Self::GenerateHold { protocol, .. } if *protocol != ProtocolVersion::V1 => {
                Err(ValueError::ProtocolOperationMismatch)
            }
            Self::GenerateBridge {
                protocol,
                constraints,
                plan,
                ..
            } => {
                if *protocol != ProtocolVersion::V2 {
                    return Err(ValueError::ProtocolOperationMismatch);
                }
                let sampling = plan
                    .sampling_map()
                    .map_err(|_| ValueError::InvalidBridgePlan)?;
                let dimensions = plan.native_dimensions();
                if constraints.conditioning != ConditioningMode::Bridge
                    || constraints.video.frames() != plan.project_frames()
                    || constraints.video.frame_rate() != plan.project_frame_rate()
                    || constraints.video.width() != dimensions.width()
                    || constraints.video.height() != dimensions.height()
                    || sampling.output_frame_count() != plan.project_frames()
                {
                    return Err(ValueError::BridgePlanMismatch);
                }
                Ok(())
            }
            Self::Cancel { .. } | Self::GenerateHold { .. } => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStage {
    Preflight,
    RuntimeLoading,
    ModelLoading,
    Conditioning,
    Inference,
    Decoding,
    Encoding,
    WorkerValidation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StageProgressWire")]
pub struct StageProgress {
    completed: u64,
    total: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StageProgressWire {
    completed: u64,
    total: u64,
}

impl StageProgress {
    pub fn new(completed: u64, total: u64) -> Result<Self, ValueError> {
        if total == 0 || completed > total {
            return Err(ValueError::InvalidProgress { completed, total });
        }
        Ok(Self { completed, total })
    }

    pub const fn completed(&self) -> u64 {
        self.completed
    }

    pub const fn total(&self) -> u64 {
        self.total
    }
}

impl TryFrom<StageProgressWire> for StageProgress {
    type Error = ValueError;

    fn try_from(value: StageProgressWire) -> Result<Self, Self::Error> {
        Self::new(value.completed, value.total)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    UnsupportedRequest,
    InvalidInput,
    MissingArtifact,
    HashMismatch,
    ResourceExhausted,
    BackendFailure,
    OutputValidationFailed,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerFailure {
    pub code: FailureCode,
    pub detail: Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Stage {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
        stage: WorkerStage,
    },
    Progress {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
        stage: WorkerStage,
        progress: StageProgress,
    },
    Completed {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
        candidate: CandidateManifest,
    },
    CompletedBridge {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
        candidate: NativeCandidateManifest,
    },
    Failed {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
        failure: WorkerFailure,
    },
    Cancelled {
        protocol: ProtocolVersion,
        identity: MessageIdentity,
    },
}

impl WorkerMessage {
    pub fn identity(&self) -> &MessageIdentity {
        match self {
            Self::Stage { identity, .. }
            | Self::Progress { identity, .. }
            | Self::Completed { identity, .. }
            | Self::CompletedBridge { identity, .. }
            | Self::Failed { identity, .. }
            | Self::Cancelled { identity, .. } => identity,
        }
    }

    pub const fn protocol(&self) -> ProtocolVersion {
        match self {
            Self::Stage { protocol, .. }
            | Self::Progress { protocol, .. }
            | Self::Completed { protocol, .. }
            | Self::CompletedBridge { protocol, .. }
            | Self::Failed { protocol, .. }
            | Self::Cancelled { protocol, .. } => *protocol,
        }
    }

    pub fn validate(&self) -> Result<(), ValueError> {
        match self {
            Self::Completed { protocol, .. } if *protocol != ProtocolVersion::V1 => {
                Err(ValueError::ProtocolOperationMismatch)
            }
            Self::CompletedBridge {
                protocol,
                candidate,
                ..
            } => {
                if *protocol != ProtocolVersion::V2 {
                    return Err(ValueError::ProtocolOperationMismatch);
                }
                candidate.validate()
            }
            _ => Ok(()),
        }
    }
}

pub fn read_host_message(reader: &mut impl Read) -> Result<Option<HostMessage>, CodecError> {
    let message: Option<HostMessage> = read_frame(reader)?;
    if let Some(message) = &message {
        message.validate().map_err(CodecError::InvalidMessage)?;
    }
    Ok(message)
}

pub fn read_worker_message(reader: &mut impl Read) -> Result<Option<WorkerMessage>, CodecError> {
    let message: Option<WorkerMessage> = read_frame(reader)?;
    if let Some(message) = &message {
        message.validate().map_err(CodecError::InvalidMessage)?;
    }
    Ok(message)
}

pub fn write_host_message(
    writer: &mut impl Write,
    message: &HostMessage,
) -> Result<(), CodecError> {
    message.validate().map_err(CodecError::InvalidMessage)?;
    write_frame(writer, message)
}

pub fn write_worker_message(
    writer: &mut impl Write,
    message: &WorkerMessage,
) -> Result<(), CodecError> {
    message.validate().map_err(CodecError::InvalidMessage)?;
    write_frame(writer, message)
}

fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> Result<Option<T>, CodecError> {
    let mut header = [0_u8; 4];
    let header_read = read_up_to(reader, &mut header)?;
    if header_read == 0 {
        return Ok(None);
    }
    if header_read != header.len() {
        return Err(CodecError::TruncatedHeader { read: header_read });
    }

    let declared = u32::from_be_bytes(header);
    let length = usize::try_from(declared).expect("u32 always fits usize on supported platforms");
    if length > MAX_FRAME_BYTES {
        return Err(CodecError::OversizedPayload {
            declared,
            max: MAX_FRAME_BYTES,
        });
    }

    let mut payload = vec![0_u8; length];
    let body_read = read_up_to(reader, &mut payload)?;
    if body_read != length {
        return Err(CodecError::TruncatedBody {
            expected: declared,
            read: body_read,
        });
    }

    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(CodecError::MalformedPayload)
}

fn read_up_to(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize, CodecError> {
    let mut read = 0;
    while read < buffer.len() {
        match reader.read(&mut buffer[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(CodecError::Io(error)),
        }
    }
    Ok(read)
}

fn write_frame<T: Serialize>(writer: &mut impl Write, message: &T) -> Result<(), CodecError> {
    let mut buffer = BoundedBuffer::new(MAX_FRAME_BYTES);
    if let Err(error) = serde_json::to_writer(&mut buffer, message) {
        if buffer.overflowed {
            return Err(CodecError::OversizedSerialization {
                max: MAX_FRAME_BYTES,
            });
        }
        return Err(CodecError::Serialize(error));
    }
    let length = u32::try_from(buffer.bytes.len()).expect("frame limit fits u32");
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&buffer.bytes)?;
    Ok(())
}

struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl BoundedBuffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            overflowed: false,
        }
    }
}

impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.overflowed = true;
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "serialized worker message exceeds frame limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ValueError {
    #[error("unsupported worker protocol version {0}")]
    UnsupportedProtocol(u32),
    #[error("{0} must contain 1-128 ASCII letters, digits, '.', '+', '-', or '_'")]
    InvalidIdentifier(&'static str),
    #[error("request version must be positive")]
    ZeroRequestVersion,
    #[error("SHA-256 must contain exactly 64 lowercase hexadecimal characters")]
    InvalidSha256,
    #[error("workspace reference must be a bounded lexical relative POSIX path")]
    InvalidWorkspaceRef,
    #[error("diagnostic must contain 1-{MAX_DIAGNOSTIC_BYTES} bytes and no NUL")]
    InvalidDiagnostic,
    #[error("video frame count must be positive")]
    ZeroFrames,
    #[error("video dimensions {width}x{height} must be positive and no larger than {max}")]
    InvalidDimensions { width: u32, height: u32, max: u32 },
    #[error("artifact byte length must be positive")]
    EmptyArtifact,
    #[error("native media and provenance must use distinct workspace references")]
    DuplicateArtifactReference,
    #[error("message operation is incompatible with its protocol version")]
    ProtocolOperationMismatch,
    #[error("bridge generation plan is invalid")]
    InvalidBridgePlan,
    #[error("bridge generation plan does not match the requested Hold constraints")]
    BridgePlanMismatch,
    #[error("progress {completed}/{total} requires a positive total and completed <= total")]
    InvalidProgress { completed: u64, total: u64 },
}

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("I/O error while transferring worker frame")]
    Io(#[from] io::Error),
    #[error("worker frame ended after {read} of 4 header bytes")]
    TruncatedHeader { read: usize },
    #[error("worker frame ended after {read} of {expected} payload bytes")]
    TruncatedBody { expected: u32, read: usize },
    #[error("worker declared {declared} payload bytes; maximum is {max}")]
    OversizedPayload { declared: u32, max: usize },
    #[error("serialized worker message exceeds {max} bytes")]
    OversizedSerialization { max: usize },
    #[error("worker payload is malformed")]
    MalformedPayload(#[source] serde_json::Error),
    #[error("worker message violates the protocol contract")]
    InvalidMessage(#[source] ValueError),
    #[error("worker message could not be serialized")]
    Serialize(#[source] serde_json::Error),
}
