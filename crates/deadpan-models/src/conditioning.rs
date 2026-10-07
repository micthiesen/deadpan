//! Immutable retention of opaque bridge-conditioning inputs.
//!
//! This boundary proves containment and exact byte identity for a context
//! manifest and its two declared frame artifacts. Modern manifests also
//! record the measured colour, frame identity and PTS of each boundary
//! picture, plus version 3's exact prepared rectangles, and check them for
//! internal consistency; this layer does not
//! decode images or confirm those records against media. The host establishes
//! that by deriving the manifest from the project's committed pictures.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{ExactRatio, GeneratedContentId, GeneratedObjectRef, ProjectId, RevisionId};
use deadpan_jobs::artifact::{
    ArtifactError, ArtifactLimits, ArtifactWorkspace, HashedArtifactSnapshot, SnapshotInterruption,
};
use deadpan_jobs::{BridgeGenerationPlan, HostMessage, WorkspaceArtifact, WorkspaceRef};
use serde::{Deserialize, Serialize};

use crate::{
    BoundaryClock, BridgeBoundaries, BridgeColor, CANONICAL_BRIDGE_COLOR, QualificationError,
};

#[path = "conditioning_geometry.rs"]
mod geometry;
pub use geometry::{ConditioningGeometry, RasterRect};
#[path = "conditioning_region.rs"]
mod region;
pub use region::{CapturedRegionBoundary, RegionCapture, RegionCaptureUnavailable};

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

/// A strict bridge context manifest. The frame artifacts remain opaque bytes
/// at this layer; later media preparation owns their actual image semantics.
///
/// Version 3 records the exact presentation and fitted content rectangles in
/// addition to version 2's explicit model colour and measured boundaries.
/// Earlier contexts remain readable without inventing missing geometry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "serde_json::Value")]
pub struct BridgeContext {
    schema_version: u32,
    model_color_space: BridgeColor,
    plan: BridgeGenerationPlan,
    left: WorkspaceArtifact,
    right: WorkspaceArtifact,
    input_color_interpretation: String,
    boundaries: Option<BridgeBoundaries>,
    geometry: Option<ConditioningGeometry>,
    region: Option<RegionCapture>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeContextV1Wire {
    schema_version: u32,
    model_color: String,
    plan: BridgeGenerationPlan,
    left: WorkspaceArtifact,
    right: WorkspaceArtifact,
    input_color_interpretation: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeContextV2Wire {
    schema_version: u32,
    model_color_space: BridgeColor,
    plan: BridgeGenerationPlan,
    left: WorkspaceArtifact,
    right: WorkspaceArtifact,
    input_color_interpretation: String,
    boundaries: BridgeBoundaries,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeContextV3Wire {
    schema_version: u32,
    model_color_space: BridgeColor,
    plan: BridgeGenerationPlan,
    left: WorkspaceArtifact,
    right: WorkspaceArtifact,
    input_color_interpretation: String,
    boundaries: BridgeBoundaries,
    geometry: ConditioningGeometry,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeContextV4Wire {
    schema_version: u32,
    model_color_space: BridgeColor,
    plan: BridgeGenerationPlan,
    left: WorkspaceArtifact,
    right: WorkspaceArtifact,
    input_color_interpretation: String,
    boundaries: BridgeBoundaries,
    geometry: ConditioningGeometry,
    region: RegionCapture,
}

impl BridgeContext {
    /// A context with explicit region absence. Definition clocks produce schema5;
    /// retained Project-clock callers keep the schema4 grammar.
    pub fn new(
        plan: BridgeGenerationPlan,
        left: WorkspaceArtifact,
        right: WorkspaceArtifact,
        input_color_interpretation: impl Into<String>,
        model_color_space: BridgeColor,
        boundaries: BridgeBoundaries,
        geometry: ConditioningGeometry,
    ) -> Result<Self, QualificationError> {
        let context = Self {
            schema_version: if matches!(boundaries.left.clock(), BoundaryClock::Definition { .. }) {
                5
            } else {
                4
            },
            model_color_space,
            plan,
            left,
            right,
            input_color_interpretation: input_color_interpretation.into(),
            boundaries: Some(boundaries),
            geometry: Some(geometry),
            region: Some(RegionCapture::None),
        };
        context.validate_shape()?;
        Ok(context)
    }

    pub fn with_region(mut self, region: RegionCapture) -> Result<Self, QualificationError> {
        if !matches!(self.schema_version, 4 | 5) {
            return Err(conditioning_error(
                "region capture requires context schema 4 or 5",
            ));
        }
        self.region = Some(region);
        self.validate_shape()?;
        Ok(self)
    }

    pub fn region(&self) -> Option<&RegionCapture> {
        self.region.as_ref()
    }

    /// Retained version-2 evidence has measured boundaries but no captured
    /// presentation crop. New conditioning must use [`Self::new`].
    pub fn legacy_v2(
        plan: BridgeGenerationPlan,
        left: WorkspaceArtifact,
        right: WorkspaceArtifact,
        input_color_interpretation: impl Into<String>,
        model_color_space: BridgeColor,
        boundaries: BridgeBoundaries,
    ) -> Result<Self, QualificationError> {
        let context = Self {
            schema_version: 2,
            model_color_space,
            plan,
            left,
            right,
            input_color_interpretation: input_color_interpretation.into(),
            boundaries: Some(boundaries),
            geometry: None,
            region: None,
        };
        context.validate_shape()?;
        Ok(context)
    }

    /// The retained version-1 grammar, for fixtures of previously captured
    /// bundles. New conditioning never writes it.
    pub fn legacy_v1(
        plan: BridgeGenerationPlan,
        left: WorkspaceArtifact,
        right: WorkspaceArtifact,
        input_color_interpretation: impl Into<String>,
    ) -> Result<Self, QualificationError> {
        let context = Self {
            schema_version: 1,
            model_color_space: CANONICAL_BRIDGE_COLOR,
            plan,
            left,
            right,
            input_color_interpretation: input_color_interpretation.into(),
            boundaries: None,
            geometry: None,
            region: None,
        };
        context.validate_shape()?;
        Ok(context)
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// The model's declared input/output colour space. Version 1 stated only
    /// `"srgb"`, which is the canonical space.
    pub const fn model_color_space(&self) -> BridgeColor {
        self.model_color_space
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

    /// The measured boundary pictures; absent from version-1 contexts.
    pub fn boundaries(&self) -> Option<&BridgeBoundaries> {
        self.boundaries.as_ref()
    }

    /// Exact model-raster geometry; absent from retained version-1/2 contexts.
    pub fn geometry(&self) -> Option<&ConditioningGeometry> {
        self.geometry.as_ref()
    }

    /// Require the declared model space to equal the canonical masters' space.
    ///
    /// Qualification derives both masters through the canonical FFV1 converter,
    /// which writes and verifies full-range sRGB BT.709 RGB. A model declared
    /// to produce anything else would have its pictures silently reinterpreted.
    pub fn check_model_output(&self) -> Result<(), QualificationError> {
        if self.model_color_space != CANONICAL_BRIDGE_COLOR {
            return Err(QualificationError::Request(format!(
                "colour interpretation mismatch: the conditioning declares the model's colour space as {}, but bridge masters are canonical {}",
                self.model_color_space.describe(),
                CANONICAL_BRIDGE_COLOR.describe()
            )));
        }
        Ok(())
    }

    /// Check definition provenance against the immutable worker origin. Legacy
    /// contexts carry no such clock identity and retain their prior validation.
    pub fn validate_definition_binding(
        &self,
        project_id: &ProjectId,
        revision_id: &RevisionId,
    ) -> Result<(), QualificationError> {
        if let Some(boundaries) = &self.boundaries {
            for side in [&boundaries.left, &boundaries.right] {
                if let BoundaryClock::Definition {
                    project_id: recorded_project,
                    revision_id: recorded_revision,
                    ..
                } = side.clock()
                    && (recorded_project != project_id || recorded_revision != revision_id)
                {
                    return Err(conditioning_error(
                        "definition boundary differs from the worker origin",
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_shape(&self) -> Result<(), QualificationError> {
        if !matches!(self.schema_version, 1..=5)
            || (self.schema_version == 1) != self.boundaries.is_none()
            || (self.schema_version >= 3) != self.geometry.is_some()
            || (self.schema_version >= 4) != self.region.is_some()
            || (self.schema_version == 1 && self.model_color_space != CANONICAL_BRIDGE_COLOR)
            || self.input_color_interpretation.trim().is_empty()
            || self.input_color_interpretation.len() > MAXIMUM_DESCRIPTION_BYTES
            || self.input_color_interpretation.contains('\0')
            || (self.left.reference() == self.right.reference() && self.left != self.right)
        {
            return Err(conditioning_error("invalid bridge context manifest"));
        }
        if let Some(boundaries) = &self.boundaries {
            for side in [&boundaries.left, &boundaries.right] {
                side.validate_shape().map_err(conditioning_error)?;
            }
            // The left picture is the frame before the Hold and the right the
            // frame after it.
            let span = self
                .plan
                .project_frames()
                .frames()
                .checked_add(1)
                .ok_or_else(|| conditioning_error("boundary span overflow"))?;
            let valid_span = match (boundaries.left.clock(), boundaries.right.clock()) {
                (
                    BoundaryClock::Project { frame: left },
                    BoundaryClock::Project { frame: right },
                ) if self.schema_version < 5 => right.checked_sub(*left) == Some(span),
                (
                    BoundaryClock::Definition {
                        project_id: lp,
                        revision_id: lr,
                        definition: ld,
                        position: left,
                    },
                    BoundaryClock::Definition {
                        project_id: rp,
                        revision_id: rr,
                        definition: rd,
                        position: right,
                    },
                ) if self.schema_version == 5 => {
                    lp == rp
                        && lr == rr
                        && ld == rd
                        && right.checked_sub(*left) == Ok(ExactRatio::integer(span))
                }
                _ => false,
            };
            if !valid_span {
                return Err(conditioning_error(
                    "boundary pictures do not enclose the planned Hold in one clock",
                ));
            }
            if let Some(geometry) = &self.geometry {
                let native = self.plan.native_dimensions();
                geometry
                    .validate([native.width(), native.height()], boundaries)
                    .map_err(conditioning_error)?;
                if let Some(region) = &self.region {
                    region
                        .validate(boundaries, geometry, [native.width(), native.height()])
                        .map_err(|error| conditioning_error(&error))?;
                }
            }
        }
        Ok(())
    }
}

impl Serialize for BridgeContext {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(region) = &self.region {
            return BridgeContextV4Wire {
                schema_version: self.schema_version,
                model_color_space: self.model_color_space,
                plan: self.plan.clone(),
                left: self.left.clone(),
                right: self.right.clone(),
                input_color_interpretation: self.input_color_interpretation.clone(),
                boundaries: self
                    .boundaries
                    .clone()
                    .ok_or_else(|| serde::ser::Error::custom("region requires boundaries"))?,
                geometry: self
                    .geometry
                    .ok_or_else(|| serde::ser::Error::custom("region requires geometry"))?,
                region: region.clone(),
            }
            .serialize(serializer);
        }
        match (&self.boundaries, &self.geometry) {
            (None, None) => BridgeContextV1Wire {
                schema_version: self.schema_version,
                model_color: "srgb".into(),
                plan: self.plan.clone(),
                left: self.left.clone(),
                right: self.right.clone(),
                input_color_interpretation: self.input_color_interpretation.clone(),
            }
            .serialize(serializer),
            (Some(boundaries), None) => BridgeContextV2Wire {
                schema_version: self.schema_version,
                model_color_space: self.model_color_space,
                plan: self.plan.clone(),
                left: self.left.clone(),
                right: self.right.clone(),
                input_color_interpretation: self.input_color_interpretation.clone(),
                boundaries: boundaries.clone(),
            }
            .serialize(serializer),
            (Some(boundaries), Some(geometry)) => BridgeContextV3Wire {
                schema_version: self.schema_version,
                model_color_space: self.model_color_space,
                plan: self.plan.clone(),
                left: self.left.clone(),
                right: self.right.clone(),
                input_color_interpretation: self.input_color_interpretation.clone(),
                boundaries: boundaries.clone(),
                geometry: *geometry,
            }
            .serialize(serializer),
            (None, Some(_)) => Err(serde::ser::Error::custom(
                "geometry requires measured boundaries",
            )),
        }
    }
}

impl TryFrom<serde_json::Value> for BridgeContext {
    type Error = QualificationError;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64);
        let context = match version {
            Some(1) => {
                let wire: BridgeContextV1Wire = serde_json::from_value(value)?;
                if wire.model_color != "srgb" {
                    return Err(conditioning_error("invalid bridge context manifest"));
                }
                Self {
                    schema_version: wire.schema_version,
                    model_color_space: CANONICAL_BRIDGE_COLOR,
                    plan: wire.plan,
                    left: wire.left,
                    right: wire.right,
                    input_color_interpretation: wire.input_color_interpretation,
                    boundaries: None,
                    geometry: None,
                    region: None,
                }
            }
            Some(2) => {
                let wire: BridgeContextV2Wire = serde_json::from_value(value)?;
                Self {
                    schema_version: wire.schema_version,
                    model_color_space: wire.model_color_space,
                    plan: wire.plan,
                    left: wire.left,
                    right: wire.right,
                    input_color_interpretation: wire.input_color_interpretation,
                    boundaries: Some(wire.boundaries),
                    geometry: None,
                    region: None,
                }
            }
            Some(3) => {
                let wire: BridgeContextV3Wire = serde_json::from_value(value)?;
                Self {
                    schema_version: wire.schema_version,
                    model_color_space: wire.model_color_space,
                    plan: wire.plan,
                    left: wire.left,
                    right: wire.right,
                    input_color_interpretation: wire.input_color_interpretation,
                    boundaries: Some(wire.boundaries),
                    geometry: Some(wire.geometry),
                    region: None,
                }
            }
            Some(4 | 5) => {
                let wire: BridgeContextV4Wire = serde_json::from_value(value)?;
                Self {
                    schema_version: wire.schema_version,
                    model_color_space: wire.model_color_space,
                    plan: wire.plan,
                    left: wire.left,
                    right: wire.right,
                    input_color_interpretation: wire.input_color_interpretation,
                    boundaries: Some(wire.boundaries),
                    geometry: Some(wire.geometry),
                    region: Some(wire.region),
                }
            }
            _ => {
                return Err(conditioning_error(
                    "unsupported bridge context manifest schema",
                ));
            }
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

    /// Read/seek only: endpoint qualification inspects the retained snapshot,
    /// never a path still writable by the generation worker.
    pub(crate) fn boundary_mut(&mut self, left: bool) -> &mut ConditioningObject {
        if left {
            &mut self.left
        } else {
            &mut self.right
        }
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
        let HostMessage::GenerateBridge {
            project_id,
            revision_id,
            input,
            plan,
            constraints,
            ..
        } = request
        else {
            return Err(conditioning_error(
                "retained conditioning requires a version-2 bridge request",
            ));
        };
        self.context
            .validate_definition_binding(project_id, revision_id)?;
        if self.manifest.declaration().reference() != &input.manifest
            || self.manifest.declaration().sha256() != &input.sha256
            || self.context.plan() != plan.as_ref()
            || self.context.region().and_then(RegionCapture::target_id)
                != constraints.region_target.as_ref()
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
        constraints,
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
        || context.region().and_then(RegionCapture::target_id) != constraints.region_target.as_ref()
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
                instructions: None,
                region_target: None,
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

    /// A current context: authored black on both sides of the 3-frame Hold.
    fn measured(
        plan: BridgeGenerationPlan,
        left: WorkspaceArtifact,
        right: WorkspaceArtifact,
    ) -> BridgeContext {
        BridgeContext::new(
            plan,
            left,
            right,
            "opaque prepared sRGB input",
            CANONICAL_BRIDGE_COLOR,
            BridgeBoundaries {
                left: crate::BoundaryPicture::AuthoredBlack {
                    clock: crate::BoundaryClock::Project { frame: 0 },
                },
                right: crate::BoundaryPicture::AuthoredBlack {
                    clock: crate::BoundaryClock::Project { frame: 4 },
                },
            },
            ConditioningGeometry {
                presentation: RasterRect::new(0, 0, 4, 2).unwrap(),
                left_content: None,
                right_content: None,
            },
        )
        .unwrap()
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
            let context = measured(plan(), left.clone(), right.clone());
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
        let mut retained = capture_bridge_conditioning(
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

        let context = retained.context().clone();
        for (is_left, expected) in [(true, &fixture.left_bytes), (false, &fixture.right_bytes)] {
            let mut inspected = Vec::new();
            let boundary = retained.boundary_mut(is_left);
            boundary.read_to_end(&mut inspected).unwrap();
            assert_eq!(&inspected, expected);
            boundary.rewind().unwrap();
        }
        assert_eq!(retained.context(), &context);
        assert_eq!(retained.receipt(), &receipt);
        retained.validate_for(&request).unwrap();

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

        let different = BridgeContext::legacy_v1(
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
            let mut context = BridgeContext::legacy_v1(
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
        let context = BridgeContext::legacy_v1(
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

        let alias_context = BridgeContext::legacy_v1(
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

    #[test]
    fn definition_clocks_are_exact_bound_and_distinct_from_project_frames() {
        let fixture = Fixture::new(false);
        let clock = |numerator| BoundaryClock::Definition {
            project_id: ProjectId::new("project").unwrap(),
            revision_id: RevisionId::new("revision").unwrap(),
            definition: NodeId::new("local").unwrap(),
            position: ExactRatio::new(numerator, 2).unwrap(),
        };
        let context = BridgeContext::new(
            plan(),
            fixture.left.clone(),
            fixture.right.clone(),
            "sRGB",
            CANONICAL_BRIDGE_COLOR,
            BridgeBoundaries {
                left: crate::BoundaryPicture::AuthoredBlack { clock: clock(29) },
                right: crate::BoundaryPicture::AuthoredBlack { clock: clock(37) },
            },
            ConditioningGeometry {
                presentation: RasterRect::new(0, 0, 4, 2).unwrap(),
                left_content: None,
                right_content: None,
            },
        )
        .unwrap();
        assert_eq!(context.schema_version(), 5);
        let wire = serde_json::to_value(&context).unwrap();
        assert!(
            wire["boundaries"]["left"]["authored_black"]
                .get("project_frame")
                .is_none()
        );
        assert_eq!(
            wire["boundaries"]["left"]["authored_black"]["clock"]["position"],
            serde_json::json!({"numerator":"29","denominator":"2"})
        );
        assert_eq!(
            serde_json::from_value::<BridgeContext>(wire.clone()).unwrap(),
            context
        );
        context
            .validate_definition_binding(
                &ProjectId::new("project").unwrap(),
                &RevisionId::new("revision").unwrap(),
            )
            .unwrap();
        assert!(
            context
                .validate_definition_binding(
                    &ProjectId::new("other").unwrap(),
                    &RevisionId::new("revision").unwrap()
                )
                .is_err()
        );
        assert!(
            context
                .validate_definition_binding(
                    &ProjectId::new("project").unwrap(),
                    &RevisionId::new("newer").unwrap()
                )
                .is_err()
        );
        for (field, value) in [
            ("project_id", serde_json::json!("other")),
            ("revision_id", serde_json::json!("other")),
            ("definition", serde_json::json!("other")),
            (
                "position",
                serde_json::json!({"numerator":"19","denominator":"1"}),
            ),
            (
                "position",
                serde_json::json!({"numerator":"-1","denominator":"2"}),
            ),
            (
                "position",
                serde_json::json!({"numerator":"37","denominator":"0"}),
            ),
            ("kind", serde_json::json!("project")),
            ("extra", serde_json::json!(true)),
        ] {
            let mut changed = wire.clone();
            changed["boundaries"]["right"]["authored_black"]["clock"][field] = value;
            assert!(
                serde_json::from_value::<BridgeContext>(changed).is_err(),
                "{field}"
            );
        }
        for schema in [2, 3, 4] {
            let mut changed = wire.clone();
            changed["schema_version"] = schema.into();
            if schema < 4 {
                changed.as_object_mut().unwrap().remove("region");
            }
            if schema < 3 {
                changed.as_object_mut().unwrap().remove("geometry");
            }
            assert!(serde_json::from_value::<BridgeContext>(changed).is_err());
        }
        let mut double = wire;
        double["boundaries"]["left"]["authored_black"]["project_frame"] = 14.into();
        assert!(serde_json::from_value::<BridgeContext>(double).is_err());
    }

    #[test]
    fn version_four_grammar_is_strict_deterministic_and_bounded() {
        let fixture = Fixture::new(false);
        let context = measured(plan(), fixture.left.clone(), fixture.right.clone());
        let bytes = serde_json::to_vec(&context).unwrap();
        assert_eq!(
            serde_json::to_vec(&context).unwrap(),
            bytes,
            "deterministic"
        );
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(wire["schema_version"], 4);
        assert_eq!(wire["region"], serde_json::json!({"selection":"none"}));
        assert!(wire.get("model_color").is_none());
        assert_eq!(
            wire["model_color_space"],
            serde_json::json!({"transfer":"srgb","primaries":"bt709","matrix":"rgb","range":"full"})
        );
        assert_eq!(
            wire["boundaries"]["right"],
            serde_json::json!({"authored_black":{"project_frame":4}})
        );
        let parsed: BridgeContext = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(parsed, context);
        assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);
        parsed.check_model_output().unwrap();

        let reject = |mutate: &dyn Fn(&mut serde_json::Value)| {
            let mut changed = wire.clone();
            mutate(&mut changed);
            assert!(
                serde_json::from_value::<BridgeContext>(changed.clone()).is_err(),
                "{changed}"
            );
        };
        reject(&|wire| wire["schema_version"] = serde_json::json!(5));
        reject(&|wire| wire["schema_version"] = serde_json::json!(2));
        reject(&|wire| wire["schema_version"] = serde_json::json!(1));
        reject(&|wire| {
            wire.as_object_mut().unwrap().remove("geometry");
        });
        reject(&|wire| wire["geometry"] = serde_json::Value::Null);
        reject(&|wire| wire["geometry"]["presentation"]["width"] = serde_json::json!(5));
        reject(&|wire| wire["geometry"]["presentation"]["width"] = serde_json::json!(0));
        reject(&|wire| wire["geometry"]["presentation"]["x"] = serde_json::json!(1));
        reject(&|wire| wire["geometry"]["left_content"] = wire["geometry"]["presentation"].clone());
        reject(&|wire| {
            wire.as_object_mut().unwrap().remove("boundaries");
        });
        reject(&|wire| wire["model_color"] = serde_json::json!("srgb"));
        reject(&|wire| wire["model_color_space"]["gamma"] = serde_json::json!(2.2));
        reject(&|wire| wire["model_color_space"]["transfer"] = serde_json::json!("gamma22"));
        // The pictures must enclose the planned Hold exactly.
        reject(&|wire| {
            wire["boundaries"]["right"]["authored_black"]["project_frame"] = serde_json::json!(5);
        });
        reject(&|wire| {
            wire["boundaries"]["left"]["authored_black"]["project_frame"] = serde_json::json!(-1);
            wire["boundaries"]["right"]["authored_black"]["project_frame"] = serde_json::json!(3);
        });
        reject(&|wire| wire["input_color_interpretation"] = serde_json::json!("x".repeat(4097)));
        // Version 1 never carries measured evidence.
        let legacy = serde_json::to_value(
            BridgeContext::legacy_v1(
                plan(),
                fixture.left.clone(),
                fixture.right.clone(),
                "opaque",
            )
            .unwrap(),
        )
        .unwrap();
        let mut with_boundaries = legacy.clone();
        with_boundaries["boundaries"] = wire["boundaries"].clone();
        assert!(serde_json::from_value::<BridgeContext>(with_boundaries).is_err());
        let legacy: BridgeContext = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.schema_version(), 1);
        assert!(legacy.boundaries().is_none());
        assert_eq!(legacy.model_color_space(), CANONICAL_BRIDGE_COLOR);
        legacy.check_model_output().unwrap();

        let legacy = BridgeContext::legacy_v2(
            plan(),
            fixture.left.clone(),
            fixture.right.clone(),
            context.input_color_interpretation(),
            context.model_color_space(),
            context.boundaries().unwrap().clone(),
        )
        .unwrap();
        let legacy_wire = serde_json::to_value(&legacy).unwrap();
        assert_eq!(legacy_wire["schema_version"], 2);
        assert!(legacy_wire.get("geometry").is_none());
        let parsed: BridgeContext = serde_json::from_value(legacy_wire.clone()).unwrap();
        assert_eq!(parsed, legacy);
        assert!(parsed.geometry().is_none());
        let mut invalid_legacy = legacy_wire;
        invalid_legacy["geometry"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<BridgeContext>(invalid_legacy).is_err());

        // A declared model space other than the canonical masters' fails.
        let mut foreign = wire;
        foreign["model_color_space"]["primaries"] = serde_json::json!("display_p3");
        let foreign: BridgeContext = serde_json::from_value(foreign).unwrap();
        let Err(QualificationError::Request(reason)) = foreign.check_model_output() else {
            panic!("display P3 model output must mismatch the sRGB masters")
        };
        assert!(
            reason.contains("colour interpretation mismatch"),
            "{reason}"
        );
    }

    /// Gate G: the conditioning context manifest is retained bytes read back
    /// from generated storage and is untrusted until it matches the request.
    /// Accepted output manifests must re-encode to the exact same value.
    #[test]
    fn adversarial_conditioning_manifests() {
        use deadpan_chaos::{Target, Verdict, fuzz, reject};
        let fixture = Fixture::new(false);
        let shared = Fixture::new(true);
        let seeds = vec![
            serde_json::to_vec(&measured(
                plan(),
                fixture.left.clone(),
                fixture.right.clone(),
            ))
            .unwrap(),
            serde_json::to_vec(&measured(plan(), shared.left.clone(), shared.right.clone()))
                .unwrap(),
            serde_json::to_vec(
                &BridgeContext::legacy_v1(
                    plan(),
                    fixture.left.clone(),
                    fixture.right.clone(),
                    "opaque",
                )
                .unwrap(),
            )
            .unwrap(),
        ];
        let report = fuzz(
            Target::json("models-conditioning-manifest").iterations(400),
            seeds,
            |input| match serde_json::from_slice::<BridgeContext>(input) {
                Ok(context) => {
                    if let Err(error) = context.check_model_output() {
                        return reject(error);
                    }
                    let encoded = serde_json::to_vec(&context)
                        .map_err(|error| format!("re-encode: {error}"))?;
                    let again: BridgeContext = serde_json::from_slice(&encoded)
                        .map_err(|error| format!("accepted manifest does not re-parse: {error}"))?;
                    if again != context {
                        return Err("manifest round trip changed the value".into());
                    }
                    Ok(Verdict::Accepted)
                }
                Err(error) => reject(error),
            },
        );
        report.assert_clean();
    }
}
