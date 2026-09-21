//! Authored presentation policy. Source analysis and qualification belong to the host.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    Anchor, AssetId, ColorPolicy, DocumentError, DocumentErrorCode, FrameDuration, FrameRate,
    NodeId, PresentationBasis, ProjectDocument, SourceQualificationId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameRateOrigin {
    Provisional,
    Explicit,
    TimedEdit,
    PrimarySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryOrigin {
    Default,
    Explicit,
    PrimarySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimarySource {
    pub asset: AssetId,
    pub qualification: SourceQualificationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasisState {
    pub rate_origin: FrameRateOrigin,
    pub geometry_origin: GeometryOrigin,
    pub primary: Option<PrimarySource>,
}

impl BasisState {
    pub const fn explicit() -> Self {
        Self {
            rate_origin: FrameRateOrigin::Explicit,
            geometry_origin: GeometryOrigin::Explicit,
            primary: None,
        }
    }

    pub const fn provisional() -> Self {
        Self {
            rate_origin: FrameRateOrigin::Provisional,
            geometry_origin: GeometryOrigin::Default,
            primary: None,
        }
    }
}

/// The host supplies measured source-derived values; the core validates intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrimarySourceImport {
    Adopt { basis: PresentationBasis },
    KeepBasis,
}

/// Basis and policy move together in every authored forward/inverse patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationState {
    pub basis: PresentationBasis,
    pub state: BasisState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationChange {
    pub before: PresentationState,
    pub after: PresentationState,
}

pub(crate) fn default_basis() -> PresentationBasis {
    PresentationBasis {
        width: 1920,
        height: 1080,
        frame_rate: FrameRate::new(30, 1).expect("constant valid frame rate"),
        color_policy: ColorPolicy::SdrRec709,
    }
}

pub(crate) fn validate_canvas(width: u32, height: u32) -> Result<(), DocumentError> {
    if width == 0
        || height == 0
        || width > 65_536
        || height > 65_536
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
    {
        return Err(invalid("canvas dimensions must be even and in 2..=65536"));
    }
    Ok(())
}

impl ProjectDocument {
    pub(crate) fn presentation_state(&self) -> PresentationState {
        PresentationState {
            basis: self.presentation_basis.clone(),
            state: self.basis_state.clone(),
        }
    }

    pub(crate) fn validate_basis_state(
        &self,
        durations: &BTreeMap<NodeId, FrameDuration>,
    ) -> Result<(), DocumentError> {
        let state = &self.basis_state;
        let default = default_basis();
        if state.geometry_origin == GeometryOrigin::Default
            && (!matches!(
                state.rate_origin,
                FrameRateOrigin::Provisional | FrameRateOrigin::TimedEdit
            ) || self.presentation_basis.width != default.width
                || self.presentation_basis.height != default.height)
        {
            return Err(invalid(
                "default geometry requires the original canvas and an automatic project origin",
            ));
        }
        if state.rate_origin == FrameRateOrigin::TimedEdit
            && self.presentation_basis.frame_rate != default.frame_rate
        {
            return Err(invalid(
                "timed-edit rate origin requires the locked default frame rate",
            ));
        }
        if let Some(primary) = &state.primary {
            let asset = self
                .assets
                .get(&primary.asset)
                .ok_or_else(|| invalid("primary source asset is missing"))?;
            if asset.video.is_none()
                || asset.source_qualification.as_ref() != Some(&primary.qualification)
            {
                return Err(invalid(
                    "primary source must match an existing qualified picture asset",
                ));
            }
        } else if state.rate_origin == FrameRateOrigin::PrimarySource
            || state.geometry_origin == GeometryOrigin::PrimarySource
        {
            return Err(invalid(
                "primary-source presentation origin requires a primary source",
            ));
        }
        if state.rate_origin == FrameRateOrigin::Provisional
            && (state != &BasisState::provisional()
                || self.presentation_basis != default
                || durations
                    .values()
                    .any(|duration| *duration != FrameDuration::ZERO)
                || self
                    .marks
                    .values()
                    .any(|mark| !matches!(mark.boundary.coordinate, Anchor::Source { .. })))
        {
            return Err(invalid(
                "provisional basis requires the default empty untimed presentation",
            ));
        }
        Ok(())
    }

    /// Evaluate actual authored changes after reduction, without treating labels,
    /// media registration, source-clock marks, or no-ops as timeline authorship.
    pub(crate) fn lock_timed_basis(&mut self, before: &Self) -> Result<(), DocumentError> {
        if self.basis_state.rate_origin != FrameRateOrigin::Provisional {
            return Ok(());
        }
        let old = before.structural_durations()?;
        let new = self.structural_durations()?;
        let changed_structure = before.nodes.keys().chain(self.nodes.keys()).any(|id| {
            before.nodes.get(id).map(|node| &node.kind) != self.nodes.get(id).map(|node| &node.kind)
                && (old
                    .get(id)
                    .is_some_and(|duration| *duration != FrameDuration::ZERO)
                    || new
                        .get(id)
                        .is_some_and(|duration| *duration != FrameDuration::ZERO))
        });
        let changed_marks = self.marks.iter().any(|(id, mark)| {
            before.marks.get(id) != Some(mark)
                && !matches!(mark.boundary.coordinate, Anchor::Source { .. })
        });
        if changed_structure || changed_marks {
            self.basis_state.rate_origin = FrameRateOrigin::TimedEdit;
        }
        Ok(())
    }
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidPresentation, message)
}
