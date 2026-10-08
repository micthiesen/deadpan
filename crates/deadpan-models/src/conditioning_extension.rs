//! Immutable, native-spaced conditioning for one-sided generation.
//!
//! This contract checks internal clocks, captured identity and geometry, retains
//! every input, and repeats the bounded continuity heuristic from signatures.
//! It does not decode PNGs, prove host source measurements, or admit model output.

use deadpan_core::{AttentionTarget, ExtensionDirection, TargetId};
use deadpan_jobs::{ExtensionGenerationPlan, Sha256};

use super::*;
use crate::BoundaryPicture;

#[path = "conditioning_extension/binding.rs"]
mod binding;
#[path = "conditioning_extension/continuity.rs"]
mod continuity;
pub use continuity::{
    EXTENSION_CAPTURE_POLICY, ExtensionContextMeasurement, ExtensionContinuityEvidence,
};

pub const EXTENSION_CONTEXT_SCHEMA_VERSION: u32 = 2;
pub const MAXIMUM_EXTENSION_CONTEXT_FRAMES: usize = 64;
pub const MAXIMUM_EXTENSION_INPUT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExtensionOperation {
    Extension,
}

/// One chronological input picture, including its exact definition position.
/// `content: null` is required for authored black, not decoded black pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionContextPicture {
    pub picture: BoundaryPicture,
    pub frame: WorkspaceArtifact,
    #[serde(deserialize_with = "required_option")]
    pub content: Option<RasterRect>,
}

/// An existing opposite seam is retained for validation, never conditioned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionOppositeSeam {
    Absent,
    PresentUnconditioned {
        picture: Box<BoundaryPicture>,
        frame: WorkspaceArtifact,
        content: Option<RasterRect>,
    },
}

impl<'de> Deserialize<'de> for ExtensionOppositeSeam {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Absent {},
            PresentUnconditioned {
                picture: Box<BoundaryPicture>,
                frame: WorkspaceArtifact,
                #[serde(deserialize_with = "required_option")]
                content: Option<RasterRect>,
            },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Absent {} => Self::Absent,
            Wire::PresentUnconditioned {
                picture,
                frame,
                content,
            } => Self::PresentUnconditioned {
                picture,
                frame,
                content,
            },
        })
    }
}

/// A selected target remains explicit even when its anchor cannot be measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "selection", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionRegionCapture {
    None,
    Selected {
        target: TargetId,
        label: String,
        target_sha256: Sha256,
        anchor: Box<CapturedRegionBoundary>,
    },
}

impl<'de> Deserialize<'de> for ExtensionRegionCapture {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "selection", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            None {},
            Selected {
                target: TargetId,
                label: String,
                target_sha256: Sha256,
                anchor: Box<CapturedRegionBoundary>,
            },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::None {} => Self::None,
            Wire::Selected {
                target,
                label,
                target_sha256,
                anchor,
            } => Self::Selected {
                target,
                label,
                target_sha256,
                anchor,
            },
        })
    }
}

impl ExtensionRegionCapture {
    pub fn new(
        target: TargetId,
        record: &AttentionTarget,
        anchor: &ExtensionContextPicture,
        presentation: RasterRect,
        native: [u32; 2],
    ) -> Result<Self, QualificationError> {
        let (boundaries, geometry) = anchor_geometry(&anchor.picture, anchor.content, presentation);
        let capture = RegionCapture::new(target, record, &boundaries, &geometry, native)
            .map_err(|error| extension_error(&error))?;
        let RegionCapture::Selected {
            target,
            label,
            target_sha256,
            left,
            ..
        } = capture
        else {
            unreachable!("a selected target always produces a selected capture");
        };
        Ok(Self::Selected {
            target,
            label,
            target_sha256,
            anchor: left,
        })
    }

    pub fn target_id(&self) -> Option<&TargetId> {
        match self {
            Self::None => None,
            Self::Selected { target, .. } => Some(target),
        }
    }

    fn validate(
        &self,
        anchor: &ExtensionContextPicture,
        presentation: RasterRect,
        native: [u32; 2],
    ) -> Result<(), QualificationError> {
        let capture = match self {
            Self::None => RegionCapture::None,
            Self::Selected {
                target,
                label,
                target_sha256,
                anchor,
            } => RegionCapture::Selected {
                target: target.clone(),
                label: label.clone(),
                target_sha256: target_sha256.clone(),
                left: anchor.clone(),
                right: anchor.clone(),
            },
        };
        let (boundaries, geometry) = anchor_geometry(&anchor.picture, anchor.content, presentation);
        capture
            .validate(&boundaries, &geometry, native)
            .map_err(|error| extension_error(&error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ExtensionContextWire")]
pub struct ExtensionContext {
    schema_version: u32,
    operation: ExtensionOperation,
    model_color_space: BridgeColor,
    plan: ExtensionGenerationPlan,
    input_color_interpretation: String,
    context: Vec<ExtensionContextPicture>,
    presentation: RasterRect,
    opposite: ExtensionOppositeSeam,
    region: ExtensionRegionCapture,
    continuity: ExtensionContinuityEvidence,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtensionContextWire {
    schema_version: u32,
    operation: ExtensionOperation,
    model_color_space: BridgeColor,
    plan: ExtensionGenerationPlan,
    input_color_interpretation: String,
    #[serde(deserialize_with = "bounded_context")]
    context: Vec<ExtensionContextPicture>,
    presentation: RasterRect,
    opposite: ExtensionOppositeSeam,
    region: ExtensionRegionCapture,
    continuity: ExtensionContinuityEvidence,
}

impl ExtensionContext {
    pub fn new(
        plan: ExtensionGenerationPlan,
        context: Vec<ExtensionContextPicture>,
        presentation: RasterRect,
        opposite: ExtensionOppositeSeam,
        input_color_interpretation: impl Into<String>,
        region: ExtensionRegionCapture,
        continuity: ExtensionContinuityEvidence,
    ) -> Result<Self, QualificationError> {
        let value = Self {
            schema_version: EXTENSION_CONTEXT_SCHEMA_VERSION,
            operation: ExtensionOperation::Extension,
            model_color_space: CANONICAL_BRIDGE_COLOR,
            plan,
            input_color_interpretation: input_color_interpretation.into(),
            context,
            presentation,
            opposite,
            region,
            continuity,
        };
        value.validate_shape()?;
        Ok(value)
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn plan(&self) -> &ExtensionGenerationPlan {
        &self.plan
    }
    pub fn context(&self) -> &[ExtensionContextPicture] {
        &self.context
    }
    pub const fn presentation(&self) -> RasterRect {
        self.presentation
    }
    pub fn opposite(&self) -> &ExtensionOppositeSeam {
        &self.opposite
    }
    pub fn region(&self) -> &ExtensionRegionCapture {
        &self.region
    }
    pub fn continuity(&self) -> &ExtensionContinuityEvidence {
        &self.continuity
    }
    pub fn input_color_interpretation(&self) -> &str {
        &self.input_color_interpretation
    }
    pub const fn model_color_space(&self) -> BridgeColor {
        self.model_color_space
    }

    pub fn anchor(&self) -> &ExtensionContextPicture {
        match self.plan.direction() {
            ExtensionDirection::FromLeft => {
                self.context.last().expect("validated nonempty context")
            }
            ExtensionDirection::FromRight => &self.context[0],
        }
    }

    pub fn validate_definition_binding(
        &self,
        project_id: &ProjectId,
        revision_id: &RevisionId,
    ) -> Result<(), QualificationError> {
        let BoundaryClock::Definition {
            project_id: project,
            revision_id: revision,
            ..
        } = self.anchor().picture.clock()
        else {
            return Err(extension_error("context requires definition clocks"));
        };
        if project != project_id || revision != revision_id {
            return Err(extension_error(
                "definition context differs from the worker origin",
            ));
        }
        // Shape validation proves every other picture belongs to this origin.
        Ok(())
    }

    fn validate_shape(&self) -> Result<(), QualificationError> {
        if self.schema_version != EXTENSION_CONTEXT_SCHEMA_VERSION
            || self.model_color_space != CANONICAL_BRIDGE_COLOR
            || self.context.is_empty()
            || self.context.len() > MAXIMUM_EXTENSION_CONTEXT_FRAMES
            || u32::try_from(self.context.len()).ok() != Some(self.plan.context_frame_count())
            || self.input_color_interpretation.trim().is_empty()
            || self.input_color_interpretation.len() > MAXIMUM_DESCRIPTION_BYTES
            || self.input_color_interpretation.contains('\0')
        {
            return Err(extension_error("invalid extension context manifest"));
        }
        let native = self.plan.native_dimensions();
        let native = [native.width(), native.height()];
        let project = self.plan.project_frame_rate();
        let rate = self.plan.native_frame_rate();
        let step = ExactRatio::new(
            i128::from(project.numerator()) * i128::from(rate.denominator()),
            i128::from(project.denominator()) * i128::from(rate.numerator()),
        )
        .map_err(|error| extension_error(&error.to_string()))?;
        let first = &self.context[0].picture;
        for (index, item) in self.context.iter().enumerate() {
            validate_picture(&item.picture, item.content, self.presentation, native)?;
            let distance = ExactRatio::integer(
                i64::try_from(index).map_err(|_| extension_error("context count overflows"))?,
            );
            let expected = step
                .checked_mul(distance)
                .map_err(|error| extension_error(&error.to_string()))?;
            if clock_difference(first, &item.picture)? != expected {
                return Err(extension_error(
                    "context is not chronological at exact native spacing",
                ));
            }
        }
        if let ExtensionOppositeSeam::PresentUnconditioned {
            picture, content, ..
        } = &self.opposite
        {
            validate_picture(picture, *content, self.presentation, native)?;
            let span = self
                .plan
                .project_frames()
                .frames()
                .checked_add(1)
                .ok_or_else(|| extension_error("opposite seam span overflows"))?;
            let (left, right) = match self.plan.direction() {
                ExtensionDirection::FromLeft => (&self.anchor().picture, picture.as_ref()),
                ExtensionDirection::FromRight => (picture.as_ref(), &self.anchor().picture),
            };
            if clock_difference(left, right)? != ExactRatio::integer(span) {
                return Err(extension_error(
                    "opposite seam does not span N+1 project frames",
                ));
            }
        }
        self.region
            .validate(self.anchor(), self.presentation, native)?;
        self.continuity.validate_shape()?;
        binding::validate(
            self.continuity.binding(),
            &self.plan,
            &self.context,
            self.presentation,
            &self.opposite,
            &self.region,
        )?;
        validate_declarations(self.declarations(), None)?;
        validate_signature_declaration(self.continuity.signatures(), self.declarations(), None)?;
        Ok(())
    }

    fn declarations(&self) -> impl Iterator<Item = &WorkspaceArtifact> {
        self.context
            .iter()
            .map(|item| &item.frame)
            .chain(match &self.opposite {
                ExtensionOppositeSeam::Absent => None,
                ExtensionOppositeSeam::PresentUnconditioned { frame, .. } => Some(frame),
            })
    }
}

impl TryFrom<ExtensionContextWire> for ExtensionContext {
    type Error = QualificationError;
    fn try_from(wire: ExtensionContextWire) -> Result<Self, Self::Error> {
        if wire.schema_version != EXTENSION_CONTEXT_SCHEMA_VERSION
            || wire.model_color_space != CANONICAL_BRIDGE_COLOR
        {
            return Err(extension_error("unsupported extension context schema"));
        }
        let ExtensionOperation::Extension = wire.operation;
        Self::new(
            wire.plan,
            wire.context,
            wire.presentation,
            wire.opposite,
            wire.input_color_interpretation,
            wire.region,
            wire.continuity,
        )
    }
}

fn anchor_geometry(
    picture: &BoundaryPicture,
    content: Option<RasterRect>,
    presentation: RasterRect,
) -> (BridgeBoundaries, ConditioningGeometry) {
    (
        BridgeBoundaries {
            left: picture.clone(),
            right: picture.clone(),
        },
        ConditioningGeometry {
            presentation,
            left_content: content,
            right_content: content,
        },
    )
}

fn validate_picture(
    picture: &BoundaryPicture,
    content: Option<RasterRect>,
    presentation: RasterRect,
    native: [u32; 2],
) -> Result<(), QualificationError> {
    picture.validate_shape().map_err(extension_error)?;
    // Extension manifests have no historical approximate-colour inputs. The
    // bridge reader retains those for old accepted masters, but new extension
    // evidence must declare the conversion this host actually applies.
    if let Some(decoded) = picture.decoded()
        && crate::model_input_conversion(&decoded.stream)
            .map_err(|error| extension_error(&error.to_string()))?
            != decoded.model_input
    {
        return Err(extension_error(
            "extension input colour conversion is not canonical",
        ));
    }
    let (boundaries, geometry) = anchor_geometry(picture, content, presentation);
    geometry
        .validate(native, &boundaries)
        .map_err(extension_error)
}

fn clock_difference(
    left: &BoundaryPicture,
    right: &BoundaryPicture,
) -> Result<ExactRatio, QualificationError> {
    match (left.clock(), right.clock()) {
        (
            BoundaryClock::Definition {
                project_id: lp,
                revision_id: lr,
                definition: ld,
                position: l,
            },
            BoundaryClock::Definition {
                project_id: rp,
                revision_id: rr,
                definition: rd,
                position: r,
            },
        ) if lp == rp && lr == rr && ld == rd => r
            .checked_sub(*l)
            .map_err(|error| extension_error(&error.to_string())),
        _ => Err(extension_error(
            "pictures require the same definition and immutable origin",
        )),
    }
}

fn validate_declarations<'a>(
    declarations: impl Iterator<Item = &'a WorkspaceArtifact>,
    manifest: Option<&WorkspaceArtifact>,
) -> Result<(), QualificationError> {
    let mut seen: Vec<&WorkspaceArtifact> = Vec::new();
    let mut total = 0_u64;
    for declaration in declarations {
        if seen.len() > MAXIMUM_EXTENSION_CONTEXT_FRAMES {
            return Err(extension_error("too many extension input frames"));
        }
        total = total
            .checked_add(declaration.byte_length())
            .filter(|total| *total <= MAXIMUM_EXTENSION_INPUT_BYTES)
            .ok_or_else(|| extension_error("extension inputs exceed 16 MiB"))?;
        if manifest.is_some_and(|manifest| manifest.reference() == declaration.reference())
            || seen.iter().any(|previous| {
                previous.reference() == declaration.reference() && *previous != declaration
            })
        {
            return Err(extension_error(
                "input aliases the manifest or contradicts a prior declaration",
            ));
        }
        seen.push(declaration);
    }
    Ok(())
}

fn validate_signature_declaration<'a>(
    signatures: &WorkspaceArtifact,
    frames: impl Iterator<Item = &'a WorkspaceArtifact>,
    manifest: Option<&WorkspaceArtifact>,
) -> Result<(), QualificationError> {
    if !(deadpan_analysis::CONTEXT_SIGNATURE_HEADER_BYTES as u64
        ..=deadpan_analysis::MAX_CONTEXT_SIGNATURE_BYTES as u64)
        .contains(&signatures.byte_length())
        || manifest.is_some_and(|manifest| manifest.reference() == signatures.reference())
        || frames
            .into_iter()
            .any(|frame| frame.reference() == signatures.reference())
    {
        return Err(extension_error(
            "signature artifact exceeds its bound or aliases another input",
        ));
    }
    Ok(())
}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn bounded_context<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 64 chronological context pictures")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = Vec::new();
            while result.len() < MAXIMUM_EXTENSION_CONTEXT_FRAMES {
                let Some(value) = sequence.next_element()? else {
                    return Ok(result);
                };
                result.push(value);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "extension context exceeds 64 pictures",
                ));
            }
            Ok(result)
        }
    }
    deserializer.deserialize_seq(Visitor(std::marker::PhantomData))
}

fn extension_error(message: &str) -> QualificationError {
    QualificationError::Request(format!("invalid extension conditioning: {message}"))
}

/// BLAKE3 identities of every immutable input, including an unconditioned seam.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ExtensionConditioningReceiptWire")]
pub struct ExtensionConditioningReceipt {
    schema_version: u32,
    operation: ExtensionOperation,
    manifest: ConditioningArtifactReceipt,
    context: Vec<ConditioningArtifactReceipt>,
    opposite: Option<ConditioningArtifactReceipt>,
    signatures: ConditioningArtifactReceipt,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtensionConditioningReceiptWire {
    schema_version: u32,
    operation: ExtensionOperation,
    manifest: ConditioningArtifactReceipt,
    #[serde(deserialize_with = "bounded_context")]
    context: Vec<ConditioningArtifactReceipt>,
    #[serde(deserialize_with = "required_option")]
    opposite: Option<ConditioningArtifactReceipt>,
    signatures: ConditioningArtifactReceipt,
}

impl ExtensionConditioningReceipt {
    fn new(
        manifest: ConditioningArtifactReceipt,
        context: Vec<ConditioningArtifactReceipt>,
        opposite: Option<ConditioningArtifactReceipt>,
        signatures: ConditioningArtifactReceipt,
    ) -> Result<Self, QualificationError> {
        let receipt = Self {
            schema_version: 2,
            operation: ExtensionOperation::Extension,
            manifest,
            context,
            opposite,
            signatures,
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
    pub fn context(&self) -> &[ConditioningArtifactReceipt] {
        &self.context
    }
    pub fn opposite(&self) -> Option<&ConditioningArtifactReceipt> {
        self.opposite.as_ref()
    }
    pub fn signatures(&self) -> &ConditioningArtifactReceipt {
        &self.signatures
    }

    fn validate_shape(&self) -> Result<(), QualificationError> {
        if self.schema_version != 2
            || self.context.is_empty()
            || self.context.len() > MAXIMUM_EXTENSION_CONTEXT_FRAMES
            || self.manifest.declaration.byte_length() > MAXIMUM_MANIFEST_BYTES
        {
            return Err(extension_error("invalid extension conditioning receipt"));
        }
        validate_declarations(
            self.frames().map(ConditioningArtifactReceipt::declaration),
            Some(self.manifest.declaration()),
        )?;
        validate_signature_declaration(
            self.signatures.declaration(),
            self.frames().map(ConditioningArtifactReceipt::declaration),
            Some(self.manifest.declaration()),
        )?;
        let frames: Vec<_> = self.frames().collect();
        for (index, frame) in frames.iter().enumerate() {
            if frames[..index].iter().any(|previous| {
                previous.declaration.reference() == frame.declaration.reference()
                    && *previous != *frame
            }) {
                return Err(extension_error(
                    "aliased input receipts have different retained identities",
                ));
            }
        }
        Ok(())
    }

    fn frames(&self) -> impl Iterator<Item = &ConditioningArtifactReceipt> {
        self.context.iter().chain(self.opposite.iter())
    }
}

impl TryFrom<ExtensionConditioningReceiptWire> for ExtensionConditioningReceipt {
    type Error = QualificationError;
    fn try_from(wire: ExtensionConditioningReceiptWire) -> Result<Self, Self::Error> {
        if wire.schema_version != 2 {
            return Err(extension_error(
                "unsupported extension conditioning receipt schema",
            ));
        }
        let ExtensionOperation::Extension = wire.operation;
        Self::new(wire.manifest, wire.context, wire.opposite, wire.signatures)
    }
}

/// Retained snapshots, separate from the workspace files a worker can change.
pub struct RetainedExtensionConditioning {
    manifest: ConditioningObject,
    context_frames: Vec<ConditioningObject>,
    opposite: Option<ConditioningObject>,
    signatures: ConditioningObject,
    measurements: Vec<ExtensionContextMeasurement>,
    context: ExtensionContext,
    receipt: ExtensionConditioningReceipt,
}

impl RetainedExtensionConditioning {
    pub fn manifest(&self) -> &ConditioningObject {
        &self.manifest
    }
    pub fn context_frames(&self) -> &[ConditioningObject] {
        &self.context_frames
    }
    pub fn opposite(&self) -> Option<&ConditioningObject> {
        self.opposite.as_ref()
    }
    pub fn signatures(&self) -> &ConditioningObject {
        &self.signatures
    }
    pub fn measurements(&self) -> &[ExtensionContextMeasurement] {
        &self.measurements
    }
    pub fn context(&self) -> &ExtensionContext {
        &self.context
    }
    pub fn receipt(&self) -> &ExtensionConditioningReceipt {
        &self.receipt
    }

    pub fn validate_for(&self, request: &HostMessage) -> Result<(), QualificationError> {
        self.context.validate_shape()?;
        self.receipt.validate_shape()?;
        validate_request(&self.context, self.manifest.declaration(), request)?;
        if self.context_frames.len() != self.context.context.len()
            || self.receipt.context.len() != self.context_frames.len()
            || !receipt_matches(&self.receipt.manifest, &self.manifest)
            || self.context.continuity.signatures() != self.signatures.declaration()
            || !receipt_matches(&self.receipt.signatures, &self.signatures)
        {
            return Err(extension_error(
                "retained context count or manifest differs from its receipt",
            ));
        }
        for ((picture, retained), receipt) in self
            .context
            .context
            .iter()
            .zip(&self.context_frames)
            .zip(&self.receipt.context)
        {
            if &picture.frame != retained.declaration() || !receipt_matches(receipt, retained) {
                return Err(extension_error(
                    "retained chronological frame differs from its receipt",
                ));
            }
        }
        match (
            &self.context.opposite,
            &self.opposite,
            &self.receipt.opposite,
        ) {
            (ExtensionOppositeSeam::Absent, None, None) => {}
            (
                ExtensionOppositeSeam::PresentUnconditioned { frame, .. },
                Some(retained),
                Some(receipt),
            ) if frame == retained.declaration() && receipt_matches(receipt, retained) => {}
            _ => {
                return Err(extension_error(
                    "retained opposite seam differs from its receipt",
                ));
            }
        }
        Ok(())
    }

    /// Manifest, chronological inputs, optional unconditioned seam, signatures.
    pub fn into_parts(
        self,
    ) -> (
        ConditioningObject,
        Vec<ConditioningObject>,
        Option<ConditioningObject>,
        ConditioningObject,
    ) {
        (
            self.manifest,
            self.context_frames,
            self.opposite,
            self.signatures,
        )
    }
}

/// Capture a strict extension manifest and every declared input under one
/// deadline. Run off the database writer, before exposing the pinned workspace
/// to the worker. Repeat the continuity heuristic from the retained signatures;
/// PNG decoding and source-index authenticity remain the capture host's work.
pub fn capture_extension_conditioning(
    workspace: &ArtifactWorkspace,
    request: &HostMessage,
    manifest_declaration: &WorkspaceArtifact,
    input_scope: &WorkspaceRef,
    limits: ConditioningLimits,
    cancelled: &AtomicBool,
) -> Result<RetainedExtensionConditioning, QualificationError> {
    limits.validate()?;
    request
        .validate()
        .map_err(|error| extension_error(&error.to_string()))?;
    let HostMessage::GenerateExtension {
        input,
        output_workspace,
        ..
    } = request
    else {
        return Err(extension_error(
            "capture requires a version-3 extension request",
        ));
    };
    if manifest_declaration.reference() != &input.manifest
        || manifest_declaration.sha256() != &input.sha256
    {
        return Err(extension_error(
            "context manifest declaration differs from the extension request",
        ));
    }
    if scopes_overlap(input_scope, output_workspace) {
        return Err(extension_error(
            "input and output workspace scopes must be disjoint",
        ));
    }
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(limits.timeout_ms))
        .ok_or(QualificationError::Deadline)?;
    check_control(cancelled, deadline)?;
    let snapshot = workspace
        .snapshot_with_control(
            input_scope,
            manifest_declaration,
            ArtifactLimits::new(limits.maximum_manifest_bytes)?,
            || snapshot_control(cancelled, deadline),
        )
        .map_err(map_snapshot_error)?;
    let (manifest, bytes) = retain_object(snapshot, true, cancelled, deadline)?;
    let context: ExtensionContext = serde_json::from_value(crate::strict_json::parse(
        &bytes.expect("manifest collection was requested"),
    )?)?;
    check_control(cancelled, deadline)?;
    validate_request(&context, manifest_declaration, request)?;
    let retain_frame = |declaration: &WorkspaceArtifact| {
        let snapshot = workspace
            .snapshot_with_control(
                input_scope,
                declaration,
                ArtifactLimits::new(
                    limits
                        .maximum_frame_bytes
                        .min(MAXIMUM_EXTENSION_INPUT_BYTES),
                )?,
                || snapshot_control(cancelled, deadline),
            )
            .map_err(map_snapshot_error)?;
        retain_object(snapshot, false, cancelled, deadline).map(|(object, _)| object)
    };
    let context_frames = context
        .context
        .iter()
        .map(|picture| retain_frame(&picture.frame))
        .collect::<Result<Vec<_>, QualificationError>>()?;
    let opposite = match &context.opposite {
        ExtensionOppositeSeam::Absent => None,
        ExtensionOppositeSeam::PresentUnconditioned { frame, .. } => Some(retain_frame(frame)?),
    };
    let snapshot = workspace
        .snapshot_with_control(
            input_scope,
            context.continuity.signatures(),
            ArtifactLimits::new(
                limits
                    .maximum_frame_bytes
                    .min(deadpan_analysis::MAX_CONTEXT_SIGNATURE_BYTES as u64),
            )?,
            || snapshot_control(cancelled, deadline),
        )
        .map_err(map_snapshot_error)?;
    let (signatures, bytes) = retain_object(snapshot, true, cancelled, deadline)?;
    let measurements = context.continuity.qualify_signatures(
        &bytes.expect("signature collection requested"),
        cancelled,
        deadline,
    )?;
    let receipt = ExtensionConditioningReceipt::new(
        object_receipt(&manifest)?,
        context_frames
            .iter()
            .map(object_receipt)
            .collect::<Result<Vec<_>, _>>()?,
        opposite.as_ref().map(object_receipt).transpose()?,
        object_receipt(&signatures)?,
    )?;
    let retained = RetainedExtensionConditioning {
        manifest,
        context_frames,
        opposite,
        signatures,
        measurements,
        context,
        receipt,
    };
    retained.validate_for(request)?;
    check_control(cancelled, deadline)?;
    Ok(retained)
}

fn validate_request(
    context: &ExtensionContext,
    manifest: &WorkspaceArtifact,
    request: &HostMessage,
) -> Result<(), QualificationError> {
    request
        .validate()
        .map_err(|error| extension_error(&error.to_string()))?;
    let HostMessage::GenerateExtension {
        project_id,
        revision_id,
        input,
        plan,
        constraints,
        ..
    } = request
    else {
        return Err(extension_error(
            "retained conditioning requires a version-3 extension request",
        ));
    };
    context.validate_definition_binding(project_id, revision_id)?;
    if manifest.reference() != &input.manifest
        || manifest.sha256() != &input.sha256
        || context.plan() != plan.as_ref()
        || context.region().target_id() != constraints.region_target.as_ref()
    {
        return Err(extension_error(
            "context differs from extension request plan, region or manifest",
        ));
    }
    validate_declarations(context.declarations(), Some(manifest))?;
    validate_signature_declaration(
        context.continuity.signatures(),
        context.declarations(),
        Some(manifest),
    )
}

fn object_receipt(
    object: &ConditioningObject,
) -> Result<ConditioningArtifactReceipt, QualificationError> {
    ConditioningArtifactReceipt::new(object.declaration().clone(), object.object().clone())
}

fn receipt_matches(receipt: &ConditioningArtifactReceipt, object: &ConditioningObject) -> bool {
    receipt.declaration() == object.declaration() && receipt.object() == object.object()
}

#[cfg(test)]
#[path = "conditioning_extension_tests.rs"]
mod tests;
