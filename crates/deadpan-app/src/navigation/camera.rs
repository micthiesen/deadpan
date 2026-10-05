//! Pure draft state and bounded key meanings for Camera mode.
//!
//! The application supplies already-projected source-axis movement vectors and
//! owns target lookup, picture work, focus, and persistence. This module never
//! reads media or mutates a project.

use std::fmt;

use deadpan_core::{
    ExactRatio, FRAMING_NUMERIC_SCALE, FramingError, FramingPose, TARGET_UNITS, TargetRegion,
};
use eframe::egui::{Key, Modifiers};

// The full authored scale range spans 4,096x. More than 171 five-percent
// steps necessarily leave that range in either direction.
const MAX_SCALE_STEPS: u32 = 171;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraPhase {
    Adjust,
    TargetPicker,
    /// Editing a target rectangle with the keyboard: center, width, height.
    Region,
    Closed,
}

/// The focused part of a target rectangle while editing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionField {
    Center,
    Width,
    Height,
}

impl RegionField {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Center => "Center",
            Self::Width => "Width",
            Self::Height => "Height",
        }
    }

    const fn next(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Center, false) | (Self::Height, true) => Self::Width,
            (Self::Width, false) | (Self::Center, true) => Self::Height,
            (Self::Height, false) | (Self::Width, true) => Self::Center,
        }
    }
}

/// One keyboard step of a target rectangle: 1% of the upright source.
pub const REGION_STEP: u32 = TARGET_UNITS / 100;
/// The smallest rectangle side Camera creates: 1% of the upright source.
pub const MIN_REGION_SIZE: u32 = REGION_STEP;

/// A target rectangle under keyboard edit, in millionths of the upright,
/// uncropped source picture. Steps clamp at the picture edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionDraft {
    region: TargetRegion,
    field: RegionField,
}

impl RegionDraft {
    /// Start from `region`, clamped into the picture and minimum size.
    pub fn new(region: TargetRegion) -> Self {
        Self {
            region: TargetRegion {
                center: region.center.map(|value| value.min(TARGET_UNITS)),
                size: region
                    .size
                    .map(|value| value.clamp(MIN_REGION_SIZE, TARGET_UNITS)),
            },
            field: RegionField::Center,
        }
    }

    pub const fn region(&self) -> TargetRegion {
        self.region
    }

    pub const fn field(&self) -> RegionField {
        self.field
    }

    /// Move the focused part `steps` keyboard steps. Center follows the
    /// direction; a size grows with Right/Up and shrinks with Left/Down.
    fn adjust(&mut self, direction: Direction, steps: u32) -> bool {
        let delta = i64::from(REGION_STEP) * i64::from(steps);
        let shift = |value: u32, delta: i64, minimum: u32| -> u32 {
            (i64::from(value) + delta).clamp(i64::from(minimum), i64::from(TARGET_UNITS)) as u32
        };
        let before = self.region;
        match self.field {
            RegionField::Center => {
                let (axis, sign) = match direction {
                    Direction::Left => (0, -1),
                    Direction::Right => (0, 1),
                    Direction::Up => (1, -1),
                    Direction::Down => (1, 1),
                };
                self.region.center[axis] = shift(self.region.center[axis], sign * delta, 0);
            }
            RegionField::Width | RegionField::Height => {
                let axis = usize::from(self.field == RegionField::Height);
                let sign = match direction {
                    Direction::Right | Direction::Up => 1,
                    Direction::Left | Direction::Down => -1,
                };
                self.region.size[axis] =
                    shift(self.region.size[axis], sign * delta, MIN_REGION_SIZE);
            }
        }
        self.region != before
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleDirection {
    In,
    Out,
}

/// Positive right and down movement for one 1% uncropped-source step, already
/// projected into the selected input canvas's pose axes and Q32-quantized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraPanSteps {
    pub horizontal: [ExactRatio; 2],
    pub vertical: [ExactRatio; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraKey {
    Digit(u8),
    Pan {
        direction: Direction,
        coarse: bool,
    },
    Scale(ScaleDirection),
    RefreshTargets,
    Reset,
    /// Start a new target rectangle (`n`).
    NewRegion,
    /// Toggle following the selected saved target (`t`).
    Follow,
    /// Correct the selected saved target at this picture (`c`).
    Correct,
    /// Track the selected saved target in the background (`T`).
    Track,
    Tab {
        reverse: bool,
    },
    Arrow(Direction),
    Commit,
    Cancel,
    ClearCount,
    Ignore,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraError {
    CountOverflow,
    ZeroCount,
    CountNotSupported,
    PanMappingUnavailable,
    PanStepPrecision,
    NotAdjusting,
    TargetNumber,
    TargetMismatch,
    Closed,
    NotEditingRegion,
    Framing(FramingError),
}

impl fmt::Display for CameraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CountOverflow => {
                f.write_str("Camera count exceeds 4294967295; no framing changed.")
            }
            Self::ZeroCount => f.write_str("Camera count must be positive; no framing changed."),
            Self::CountNotSupported => {
                f.write_str("Counts apply only to Camera movement and scale.")
            }
            Self::PanMappingUnavailable => {
                f.write_str("Camera movement is unavailable for this picture mapping.")
            }
            Self::PanStepPrecision => {
                f.write_str("Camera movement needs a Q32 source-step mapping.")
            }
            Self::NotAdjusting => f.write_str("Camera pose can only be edited in Adjust mode."),
            Self::TargetNumber => f.write_str("Camera targets are numbered 1 through 9."),
            Self::TargetMismatch => {
                f.write_str("The selected target changed before it could be applied.")
            }
            Self::Closed => f.write_str("Camera mode has already closed."),
            Self::NotEditingRegion => f.write_str("No target rectangle is being edited."),
            Self::Framing(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CameraError {}

impl From<FramingError> for CameraError {
    fn from(value: FramingError) -> Self {
        Self::Framing(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraEffect {
    None,
    RefreshTargets,
    SelectTarget(u8),
    PoseChanged(FramingPose),
    Reset(FramingPose),
    TargetsClosed,
    CycleField {
        reverse: bool,
    },
    AdjustField {
        direction: Direction,
        count: u32,
    },
    Commit {
        pose: FramingPose,
        reset: bool,
    },
    Unchanged,
    Cancel,
    /// The app supplies the starting rectangle and calls `open_region`.
    RequestRegion {
        correction: bool,
    },
    ToggleFollow,
    Track,
    RegionChanged(TargetRegion),
    /// Enter on a rectangle. The rectangle stays open until the app closes it.
    RegionCommit(TargetRegion),
    RegionClosed,
    Rejected(CameraError),
}

/// Camera owns no persisted state. `entry_pose` is retained for Escape and
/// no-op detection; `pose` is the temporary pose at the current cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraDraft {
    entry_pose: FramingPose,
    pose: FramingPose,
    phase: CameraPhase,
    count: Option<u32>,
    count_overflow: bool,
    pending_target: Option<u8>,
    reset_requested: bool,
    region: Option<RegionDraft>,
}

impl CameraDraft {
    pub fn new(entry_pose: FramingPose) -> Result<Self, CameraError> {
        entry_pose.validate()?;
        Ok(Self {
            entry_pose,
            pose: entry_pose,
            phase: CameraPhase::Adjust,
            count: None,
            count_overflow: false,
            pending_target: None,
            reset_requested: false,
            region: None,
        })
    }

    /// This draft's adjustments against a new entry pose, for a Camera that
    /// continues on the revision its own target edit created. An untouched
    /// pose follows the new entry; an adjusted pose and a reset are kept.
    pub fn rebased(&self, entry_pose: FramingPose) -> Self {
        let adjusted = self.reset_requested || self.pose != self.entry_pose;
        Self {
            entry_pose,
            pose: if adjusted { self.pose } else { entry_pose },
            phase: CameraPhase::Adjust,
            count: None,
            count_overflow: false,
            pending_target: None,
            reset_requested: self.reset_requested,
            region: None,
        }
    }

    pub const fn region(&self) -> Option<&RegionDraft> {
        self.region.as_ref()
    }

    /// Begin editing `region` from Adjust.
    pub fn open_region(&mut self, region: TargetRegion) -> CameraEffect {
        if self.phase != CameraPhase::Adjust {
            return CameraEffect::Rejected(CameraError::NotAdjusting);
        }
        let draft = RegionDraft::new(region);
        self.region = Some(draft);
        self.phase = CameraPhase::Region;
        self.clear_count();
        CameraEffect::RegionChanged(draft.region())
    }

    /// Leave rectangle editing, after a save or Escape.
    pub fn close_region(&mut self) -> CameraEffect {
        if self.phase != CameraPhase::Region {
            return CameraEffect::Rejected(CameraError::NotEditingRegion);
        }
        self.region = None;
        self.phase = CameraPhase::Adjust;
        self.clear_count();
        CameraEffect::RegionClosed
    }

    fn region_input(&mut self, key: CameraKey) -> CameraEffect {
        let steps = |draft: &mut Self, coarse: bool| -> Result<u32, CameraError> {
            let count = draft.take_count()?;
            count
                .checked_mul(if coarse { 5 } else { 1 })
                .ok_or(CameraError::CountOverflow)
        };
        let direction = match key {
            CameraKey::Pan { direction, coarse } => Some((direction, coarse)),
            CameraKey::Arrow(direction) => Some((direction, false)),
            _ => None,
        };
        if let Some((direction, coarse)) = direction {
            let steps = match steps(self, coarse) {
                Ok(steps) => steps,
                Err(error) => return CameraEffect::Rejected(error),
            };
            let region = self.region.as_mut().expect("region phase has a region");
            return if region.adjust(direction, steps) {
                CameraEffect::RegionChanged(region.region())
            } else {
                CameraEffect::None
            };
        }
        match key {
            CameraKey::Digit(digit) => self.digit(digit),
            CameraKey::Tab { reverse } => {
                self.clear_count();
                let region = self.region.as_mut().expect("region phase has a region");
                region.field = region.field.next(reverse);
                CameraEffect::RegionChanged(region.region())
            }
            CameraKey::Commit => {
                if let Some(error) = self.require_no_count() {
                    return CameraEffect::Rejected(error);
                }
                CameraEffect::RegionCommit(self.region.expect("region phase has a region").region())
            }
            CameraKey::Cancel => self.close_region(),
            CameraKey::Ignore => CameraEffect::None,
            _ => {
                self.clear_count();
                CameraEffect::None
            }
        }
    }

    pub const fn pose(&self) -> FramingPose {
        self.pose
    }

    pub const fn phase(&self) -> CameraPhase {
        self.phase
    }

    pub const fn pending_count(&self) -> Option<u32> {
        self.count
    }

    pub const fn count_overflowed(&self) -> bool {
        self.count_overflow
    }

    pub const fn reset_requested(&self) -> bool {
        self.reset_requested
    }

    /// Replace the draft pose from validated numeric controls. The numeric
    /// field owns quantization of edited values so untouched exact components
    /// remain unchanged. This does not clear an earlier reset request.
    pub fn set_pose(&mut self, pose: FramingPose) -> CameraEffect {
        if self.phase == CameraPhase::Closed {
            return CameraEffect::Rejected(CameraError::Closed);
        }
        if self.phase != CameraPhase::Adjust {
            return CameraEffect::Rejected(CameraError::NotAdjusting);
        }
        self.clear_count();
        self.set_candidate_pose(pose.validate().map(|()| pose))
    }

    pub fn clear_count(&mut self) {
        self.count = None;
        self.count_overflow = false;
    }

    /// Apply one already-routed Camera input. Pan is rejected when the app
    /// cannot supply its exact source-to-input-canvas movement projection.
    pub fn input(&mut self, key: CameraKey, pan_steps: Option<CameraPanSteps>) -> CameraEffect {
        if self.phase == CameraPhase::Closed {
            return CameraEffect::Rejected(CameraError::Closed);
        }
        if self.phase == CameraPhase::Region {
            return self.region_input(key);
        }
        match key {
            CameraKey::Ignore => CameraEffect::None,
            CameraKey::ClearCount | CameraKey::Other => {
                self.clear_count();
                CameraEffect::None
            }
            CameraKey::Digit(digit) => self.digit(digit),
            CameraKey::RefreshTargets => self.refresh_targets(),
            CameraKey::Reset => self.reset(),
            CameraKey::NewRegion | CameraKey::Correct | CameraKey::Follow | CameraKey::Track => {
                if self.phase != CameraPhase::Adjust {
                    self.clear_count();
                    return CameraEffect::Rejected(CameraError::NotAdjusting);
                }
                if let Some(error) = self.require_no_count() {
                    return CameraEffect::Rejected(error);
                }
                match key {
                    CameraKey::NewRegion => CameraEffect::RequestRegion { correction: false },
                    CameraKey::Correct => CameraEffect::RequestRegion { correction: true },
                    CameraKey::Follow => CameraEffect::ToggleFollow,
                    _ => CameraEffect::Track,
                }
            }
            CameraKey::Pan { direction, coarse } => self.pan(direction, coarse, pan_steps),
            CameraKey::Scale(direction) => self.scale(direction),
            CameraKey::Tab { reverse } => {
                self.clear_count();
                CameraEffect::CycleField { reverse }
            }
            CameraKey::Arrow(direction) => match self.take_count() {
                Ok(count) if self.phase == CameraPhase::Adjust => {
                    CameraEffect::AdjustField { direction, count }
                }
                Ok(_) => CameraEffect::None,
                Err(error) => CameraEffect::Rejected(error),
            },
            CameraKey::Commit => self.commit(),
            CameraKey::Cancel => self.cancel(),
        }
    }

    /// Resolve the pending numbered target against the app's current target
    /// snapshot. A failed/stale lookup must leave the draft in its picker.
    pub fn choose_target(&mut self, number: u8, pose: FramingPose) -> CameraEffect {
        if self.phase != CameraPhase::TargetPicker || self.pending_target != Some(number) {
            return CameraEffect::Rejected(CameraError::TargetMismatch);
        }
        if !(1..=9).contains(&number) {
            self.pending_target = None;
            return CameraEffect::Rejected(CameraError::TargetNumber);
        }
        let pose = match pose.quantized() {
            Ok(pose) => pose,
            Err(error) => return CameraEffect::Rejected(error.into()),
        };
        self.pose = pose;
        self.pending_target = None;
        self.phase = CameraPhase::Adjust;
        CameraEffect::PoseChanged(pose)
    }

    /// Abandon an asynchronous target lookup while retaining the picker.
    pub fn target_unavailable(&mut self, number: u8) -> CameraEffect {
        if self.phase != CameraPhase::TargetPicker || self.pending_target != Some(number) {
            return CameraEffect::Rejected(CameraError::TargetMismatch);
        }
        self.pending_target = None;
        CameraEffect::Rejected(CameraError::TargetMismatch)
    }

    fn digit(&mut self, digit: u8) -> CameraEffect {
        if self.phase == CameraPhase::TargetPicker {
            if !(1..=9).contains(&digit) {
                return CameraEffect::Rejected(CameraError::TargetNumber);
            }
            self.pending_target = Some(digit);
            return CameraEffect::SelectTarget(digit);
        }
        if digit > 9 {
            return CameraEffect::None;
        }
        match self
            .count
            .unwrap_or(0)
            .checked_mul(10)
            .and_then(|value| value.checked_add(u32::from(digit)))
        {
            Some(count) => {
                self.count = Some(count);
                CameraEffect::None
            }
            None => {
                self.count_overflow = true;
                CameraEffect::None
            }
        }
    }

    fn refresh_targets(&mut self) -> CameraEffect {
        if self.phase == CameraPhase::TargetPicker {
            self.pending_target = None;
            self.phase = CameraPhase::Adjust;
            self.clear_count();
            return CameraEffect::TargetsClosed;
        }
        if let Some(error) = self.require_no_count() {
            return CameraEffect::Rejected(error);
        }
        self.phase = CameraPhase::TargetPicker;
        self.pending_target = None;
        CameraEffect::RefreshTargets
    }

    fn reset(&mut self) -> CameraEffect {
        if let Some(error) = self.require_no_count() {
            return CameraEffect::Rejected(error);
        }
        let pose = FramingPose::identity();
        self.pose = pose;
        self.phase = CameraPhase::Adjust;
        self.pending_target = None;
        self.reset_requested = true;
        CameraEffect::Reset(pose)
    }

    fn pan(
        &mut self,
        direction: Direction,
        coarse: bool,
        pan_steps: Option<CameraPanSteps>,
    ) -> CameraEffect {
        if self.phase != CameraPhase::Adjust {
            return CameraEffect::None;
        }
        let count = match self.take_count() {
            Ok(count) => count,
            Err(error) => return CameraEffect::Rejected(error),
        };
        let Some(steps) = pan_steps else {
            return CameraEffect::Rejected(CameraError::PanMappingUnavailable);
        };
        let vector = (|| -> Result<[ExactRatio; 2], deadpan_core::TimeError> {
            Ok(match direction {
                Direction::Left => [
                    ExactRatio::ZERO.checked_sub(steps.horizontal[0])?,
                    ExactRatio::ZERO.checked_sub(steps.horizontal[1])?,
                ],
                Direction::Right => steps.horizontal,
                Direction::Up => [
                    ExactRatio::ZERO.checked_sub(steps.vertical[0])?,
                    ExactRatio::ZERO.checked_sub(steps.vertical[1])?,
                ],
                Direction::Down => steps.vertical,
            })
        })();
        let vector = match vector {
            Ok(vector) => vector,
            Err(_) => return CameraEffect::Rejected(CameraError::Framing(FramingError::Overflow)),
        };
        if vector.iter().any(|value| !is_q32_grid(*value)) {
            return CameraEffect::Rejected(CameraError::PanStepPrecision);
        }
        let multiplier = i64::from(count) * if coarse { 5 } else { 1 };
        let remaining = ExactRatio::integer(multiplier - 1);
        let candidate = (|| {
            // Quantize the first step from the arbitrary exact entry pose.
            // Later steps add grid values and need no per-key loop; the final
            // pose is identical to applying and quantizing every repetition.
            let first = FramingPose::new(
                self.pose.center_x.checked_add(vector[0])?,
                self.pose.center_y.checked_add(vector[1])?,
                self.pose.scale,
            )?
            .quantized()?;
            let dx = vector[0].checked_mul(remaining)?;
            let dy = vector[1].checked_mul(remaining)?;
            FramingPose::new(
                first.center_x.checked_add(dx)?,
                first.center_y.checked_add(dy)?,
                first.scale,
            )?
            .quantized()
        })();
        self.set_candidate_pose(candidate)
    }

    fn scale(&mut self, direction: ScaleDirection) -> CameraEffect {
        if self.phase != CameraPhase::Adjust {
            return CameraEffect::None;
        }
        let count = match self.take_count() {
            Ok(count) => count,
            Err(error) => return CameraEffect::Rejected(error),
        };
        if count > MAX_SCALE_STEPS {
            return CameraEffect::Rejected(CameraError::Framing(FramingError::PoseRange));
        }
        let factor = match direction {
            ScaleDirection::In => ExactRatio::new(21, 20),
            ScaleDirection::Out => ExactRatio::new(20, 21),
        };
        let candidate = factor
            .map_err(|_| FramingError::Overflow)
            .and_then(|factor| {
                let mut pose = self.pose;
                for _ in 0..count {
                    let scale = pose.scale.checked_mul(factor)?;
                    pose = FramingPose::new(pose.center_x, pose.center_y, scale)?.quantized()?;
                }
                Ok(pose)
            });
        self.set_candidate_pose(candidate)
    }

    fn set_candidate_pose(&mut self, candidate: Result<FramingPose, FramingError>) -> CameraEffect {
        match candidate {
            Ok(pose) if pose != self.pose => {
                self.pose = pose;
                CameraEffect::PoseChanged(pose)
            }
            Ok(_) => CameraEffect::None,
            Err(error) => CameraEffect::Rejected(CameraError::Framing(error)),
        }
    }

    fn commit(&mut self) -> CameraEffect {
        if self.phase != CameraPhase::Adjust {
            return CameraEffect::Rejected(CameraError::NotAdjusting);
        }
        if let Some(error) = self.require_no_count() {
            return CameraEffect::Rejected(error);
        }
        self.phase = CameraPhase::Closed;
        self.pending_target = None;
        if self.reset_requested || self.pose != self.entry_pose {
            CameraEffect::Commit {
                pose: self.pose,
                reset: self.reset_requested,
            }
        } else {
            CameraEffect::Unchanged
        }
    }

    fn cancel(&mut self) -> CameraEffect {
        self.phase = CameraPhase::Closed;
        self.pending_target = None;
        self.region = None;
        self.clear_count();
        CameraEffect::Cancel
    }

    fn take_count(&mut self) -> Result<u32, CameraError> {
        if self.count_overflow {
            self.clear_count();
            return Err(CameraError::CountOverflow);
        }
        let count = self.count.take().unwrap_or(1);
        self.count_overflow = false;
        if count == 0 {
            Err(CameraError::ZeroCount)
        } else {
            Ok(count)
        }
    }

    fn require_no_count(&mut self) -> Option<CameraError> {
        if self.count_overflow {
            self.clear_count();
            Some(CameraError::CountOverflow)
        } else if self.count.is_some() {
            self.clear_count();
            Some(CameraError::CountNotSupported)
        } else {
            None
        }
    }
}

fn is_q32_grid(value: ExactRatio) -> bool {
    i64::try_from(FRAMING_NUMERIC_SCALE)
        .ok()
        .and_then(|scale| value.checked_mul(ExactRatio::integer(scale)).ok())
        .is_some_and(|scaled| scaled.denominator() == 1)
}

/// Route one logical egui key while Camera owns activation. Text and IME are
/// left to their fields, but clear a pending count. `None` is reserved for
/// command/control shortcuts that the app may route through its global path.
/// Repeated keys are limited to continuous movement, scaling, and field arrows.
pub fn route_camera_key(
    key: Key,
    modifiers: Modifiers,
    text: bool,
    ime: bool,
    repeat: bool,
) -> Option<CameraKey> {
    if ime || text {
        return Some(CameraKey::ClearCount);
    }
    if modifiers.ctrl || modifiers.command || modifiers.mac_cmd {
        return None;
    }
    if modifiers.alt {
        return Some(CameraKey::Other);
    }

    let shift = modifiers.shift;
    let routed = match key {
        Key::Num0 if !shift => CameraKey::Digit(0),
        Key::Num1 if !shift => CameraKey::Digit(1),
        Key::Num2 if !shift => CameraKey::Digit(2),
        Key::Num3 if !shift => CameraKey::Digit(3),
        Key::Num4 if !shift => CameraKey::Digit(4),
        Key::Num5 if !shift => CameraKey::Digit(5),
        Key::Num6 if !shift => CameraKey::Digit(6),
        Key::Num7 if !shift => CameraKey::Digit(7),
        Key::Num8 if !shift => CameraKey::Digit(8),
        Key::Num9 if !shift => CameraKey::Digit(9),
        Key::H | Key::J | Key::K | Key::L => CameraKey::Pan {
            direction: match key {
                Key::H => Direction::Left,
                Key::J => Direction::Down,
                Key::K => Direction::Up,
                Key::L => Direction::Right,
                _ => unreachable!("matched Camera pan key"),
            },
            coarse: shift,
        },
        Key::Plus => CameraKey::Scale(ScaleDirection::In),
        Key::Minus if !shift => CameraKey::Scale(ScaleDirection::Out),
        Key::F if !shift => CameraKey::RefreshTargets,
        Key::R if !shift => CameraKey::Reset,
        Key::N if !shift => CameraKey::NewRegion,
        Key::T if shift => CameraKey::Track,
        Key::T => CameraKey::Follow,
        Key::C if !shift => CameraKey::Correct,
        Key::Tab => CameraKey::Tab { reverse: shift },
        Key::ArrowLeft => CameraKey::Arrow(Direction::Left),
        Key::ArrowRight => CameraKey::Arrow(Direction::Right),
        Key::ArrowUp => CameraKey::Arrow(Direction::Up),
        Key::ArrowDown => CameraKey::Arrow(Direction::Down),
        Key::Enter if !shift => CameraKey::Commit,
        Key::Escape if !shift => CameraKey::Cancel,
        Key::Backspace if !shift => CameraKey::ClearCount,
        _ => CameraKey::Other,
    };

    if repeat
        && !matches!(
            routed,
            CameraKey::Pan { .. } | CameraKey::Scale(_) | CameraKey::Arrow(_)
        )
    {
        Some(CameraKey::Ignore)
    } else {
        Some(routed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps() -> CameraPanSteps {
        let percent = ExactRatio::new(42_949_673, 4_294_967_296).unwrap();
        CameraPanSteps {
            horizontal: [percent, ExactRatio::ZERO],
            vertical: [ExactRatio::ZERO, percent],
        }
    }

    fn new_draft() -> CameraDraft {
        CameraDraft::new(FramingPose::identity()).unwrap()
    }

    fn quantized_x(value: ExactRatio) -> ExactRatio {
        FramingPose::new(value, ExactRatio::new(1, 2).unwrap(), ExactRatio::ONE)
            .unwrap()
            .quantized()
            .unwrap()
            .center_x
    }

    fn expected_pan_coordinate(start: ExactRatio, step: ExactRatio, count: i64) -> ExactRatio {
        let shifted = start
            .checked_add(step.checked_mul(ExactRatio::integer(count)).unwrap())
            .unwrap();
        quantized_x(shifted)
    }

    #[test]
    fn camera_enters_adjust_at_current_pose_and_escape_discards_the_draft() {
        let initial = FramingPose::new(
            ExactRatio::new(7, 10).unwrap(),
            ExactRatio::new(2, 5).unwrap(),
            ExactRatio::new(3, 2).unwrap(),
        )
        .unwrap();
        let mut draft = CameraDraft::new(initial).unwrap();
        assert_eq!(draft.phase(), CameraPhase::Adjust);
        assert_eq!(draft.pose(), initial);
        assert_eq!(draft.input(CameraKey::Cancel, None), CameraEffect::Cancel);
        assert_eq!(draft.phase(), CameraPhase::Closed);
        assert_eq!(draft.entry_pose, initial);
    }

    #[test]
    fn source_axis_vectors_pan_exactly_and_coarse_steps_are_five_percent() {
        let mut draft = new_draft();
        assert_eq!(
            draft.input(
                CameraKey::Pan {
                    direction: Direction::Left,
                    coarse: false,
                },
                Some(steps()),
            ),
            CameraEffect::PoseChanged(
                FramingPose::new(
                    ExactRatio::new(49, 100).unwrap(),
                    ExactRatio::new(1, 2).unwrap(),
                    ExactRatio::ONE,
                )
                .unwrap()
                .quantized()
                .unwrap()
            )
        );
        let CameraEffect::PoseChanged(coarse) = draft.input(
            CameraKey::Pan {
                direction: Direction::Down,
                coarse: true,
            },
            Some(steps()),
        ) else {
            panic!("coarse movement must update the draft");
        };
        assert_eq!(
            coarse.center_y,
            expected_pan_coordinate(ExactRatio::new(1, 2).unwrap(), steps().vertical[1], 5,)
        );
    }

    #[test]
    fn count_multiplies_pan_and_scale() {
        let mut draft = new_draft();
        for digit in [3, 0] {
            assert_eq!(
                draft.input(CameraKey::Digit(digit), None),
                CameraEffect::None
            );
        }
        let CameraEffect::PoseChanged(panned) = draft.input(
            CameraKey::Pan {
                direction: Direction::Right,
                coarse: false,
            },
            Some(steps()),
        ) else {
            panic!("counted pan must update the draft");
        };
        assert_eq!(
            panned.center_x,
            expected_pan_coordinate(ExactRatio::new(1, 2).unwrap(), steps().horizontal[0], 30,)
        );
        let mut repeated_pan = new_draft();
        for _ in 0..30 {
            repeated_pan.input(
                CameraKey::Pan {
                    direction: Direction::Right,
                    coarse: false,
                },
                Some(steps()),
            );
        }
        assert_eq!(panned, repeated_pan.pose());
        assert_eq!(draft.pending_count(), None);

        assert_eq!(draft.input(CameraKey::Digit(3), None), CameraEffect::None);
        let CameraEffect::PoseChanged(zoomed) =
            draft.input(CameraKey::Scale(ScaleDirection::In), None)
        else {
            panic!("counted zoom must update the draft");
        };
        let mut repeated = new_draft();
        for _ in 0..3 {
            repeated.input(CameraKey::Scale(ScaleDirection::In), None);
        }
        assert_eq!(zoomed.scale, repeated.pose().scale);
    }

    #[test]
    fn counted_scale_matches_repeated_quantized_steps_and_stays_bounded() {
        let mut counted = new_draft();
        for digit in [5, 0] {
            counted.input(CameraKey::Digit(digit), None);
        }
        assert!(matches!(
            counted.input(CameraKey::Scale(ScaleDirection::In), None),
            CameraEffect::PoseChanged(_)
        ));

        let mut repeated = new_draft();
        for _ in 0..50 {
            repeated.input(CameraKey::Scale(ScaleDirection::In), None);
        }
        assert_eq!(counted.pose(), repeated.pose());

        let maximum = FramingPose::new(
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::integer(64),
        )
        .unwrap();
        let mut near_minimum = CameraDraft::new(maximum).unwrap();
        for digit in [1, 6, 0] {
            near_minimum.input(CameraKey::Digit(digit), None);
        }
        assert!(matches!(
            near_minimum.input(CameraKey::Scale(ScaleDirection::Out), None),
            CameraEffect::PoseChanged(_)
        ));
        assert!(near_minimum.pose().scale.compare_integer(1).is_lt());
        assert!(near_minimum.pose().validate().is_ok());

        let mut too_many = new_draft();
        let before = too_many.pose();
        for digit in [1, 7, 2] {
            too_many.input(CameraKey::Digit(digit), None);
        }
        assert_eq!(
            too_many.input(CameraKey::Scale(ScaleDirection::In), None),
            CameraEffect::Rejected(CameraError::Framing(FramingError::PoseRange))
        );
        assert_eq!(too_many.pose(), before, "failed batches are atomic");
    }

    #[test]
    fn target_picker_digits_choose_targets_instead_of_becoming_counts() {
        let mut draft = new_draft();
        assert_eq!(
            draft.input(CameraKey::RefreshTargets, None),
            CameraEffect::RefreshTargets
        );
        assert_eq!(draft.phase(), CameraPhase::TargetPicker);
        assert_eq!(
            draft.input(CameraKey::Digit(6), None),
            CameraEffect::SelectTarget(6)
        );
        assert_eq!(draft.pending_count(), None);
        let target = FramingPose::new(
            ExactRatio::new(2, 3).unwrap(),
            ExactRatio::new(1, 3).unwrap(),
            ExactRatio::new(3, 2).unwrap(),
        )
        .unwrap();
        assert_eq!(
            draft.choose_target(6, target),
            CameraEffect::PoseChanged(target.quantized().unwrap())
        );
        assert_eq!(draft.phase(), CameraPhase::Adjust);
        assert_eq!(draft.input(CameraKey::Digit(2), None), CameraEffect::None);
        assert_eq!(draft.pending_count(), Some(2));
    }

    #[test]
    fn invalid_or_stale_targets_leave_picker_open() {
        let mut draft = new_draft();
        draft.input(CameraKey::RefreshTargets, None);
        assert_eq!(
            draft.input(CameraKey::Commit, None),
            CameraEffect::Rejected(CameraError::NotAdjusting)
        );
        assert_eq!(
            draft.input(CameraKey::Digit(3), None),
            CameraEffect::SelectTarget(3)
        );
        assert_eq!(
            draft.choose_target(2, FramingPose::identity()),
            CameraEffect::Rejected(CameraError::TargetMismatch)
        );
        assert_eq!(draft.phase(), CameraPhase::TargetPicker);
        assert_eq!(draft.pending_count(), None);
    }

    #[test]
    fn f_toggles_target_picker_without_discarding_the_adjustment() {
        let mut draft = new_draft();
        draft.input(
            CameraKey::Pan {
                direction: Direction::Right,
                coarse: false,
            },
            Some(steps()),
        );
        let pose = draft.pose();
        assert_eq!(
            draft.input(CameraKey::RefreshTargets, None),
            CameraEffect::RefreshTargets
        );
        assert_eq!(
            draft.input(CameraKey::RefreshTargets, None),
            CameraEffect::TargetsClosed
        );
        assert_eq!(draft.phase(), CameraPhase::Adjust);
        assert_eq!(draft.pose(), pose);
    }

    #[test]
    fn reset_is_explicit_and_survives_later_pose_adjustments() {
        let initial = FramingPose::new(
            ExactRatio::new(3, 4).unwrap(),
            ExactRatio::new(1, 4).unwrap(),
            ExactRatio::new(2, 1).unwrap(),
        )
        .unwrap();
        let mut draft = CameraDraft::new(initial).unwrap();
        assert_eq!(
            draft.input(CameraKey::Reset, None),
            CameraEffect::Reset(FramingPose::identity())
        );
        assert!(draft.reset_requested());
        assert_eq!(
            draft.input(
                CameraKey::Pan {
                    direction: Direction::Right,
                    coarse: false,
                },
                Some(steps()),
            ),
            CameraEffect::PoseChanged(
                FramingPose::new(
                    ExactRatio::new(51, 100).unwrap(),
                    ExactRatio::new(1, 2).unwrap(),
                    ExactRatio::ONE,
                )
                .unwrap()
                .quantized()
                .unwrap()
            )
        );
        assert_eq!(
            draft.input(CameraKey::Commit, None),
            CameraEffect::Commit {
                pose: draft.pose(),
                reset: true,
            }
        );
    }

    #[test]
    fn bounds_and_missing_projection_reject_without_changing_the_pose() {
        let mut draft = new_draft();
        let before = draft.pose();
        assert_eq!(
            draft.input(
                CameraKey::Pan {
                    direction: Direction::Left,
                    coarse: false,
                },
                None,
            ),
            CameraEffect::Rejected(CameraError::PanMappingUnavailable)
        );
        let large = CameraPanSteps {
            horizontal: [ExactRatio::integer(100), ExactRatio::ZERO],
            vertical: [ExactRatio::ZERO, ExactRatio::ZERO],
        };
        assert_eq!(
            draft.input(
                CameraKey::Pan {
                    direction: Direction::Right,
                    coarse: false,
                },
                Some(large),
            ),
            CameraEffect::Rejected(CameraError::Framing(FramingError::PoseRange))
        );
        assert_eq!(draft.pose(), before);
        let imprecise = CameraPanSteps {
            horizontal: [ExactRatio::new(1, 100).unwrap(), ExactRatio::ZERO],
            vertical: [ExactRatio::ZERO, ExactRatio::new(1, 100).unwrap()],
        };
        assert_eq!(
            draft.input(
                CameraKey::Pan {
                    direction: Direction::Right,
                    coarse: false,
                },
                Some(imprecise),
            ),
            CameraEffect::Rejected(CameraError::PanStepPrecision)
        );
        assert_eq!(draft.pose(), before);
        assert_eq!(
            draft.input(CameraKey::Scale(ScaleDirection::Out), None),
            CameraEffect::PoseChanged(
                FramingPose::new(
                    before.center_x,
                    before.center_y,
                    ExactRatio::new(20, 21).unwrap(),
                )
                .unwrap()
                .quantized()
                .unwrap()
            )
        );
    }

    #[test]
    fn zero_overflow_unsupported_count_and_repeated_commit_are_bounded() {
        let mut draft = new_draft();
        draft.input(CameraKey::Digit(0), None);
        assert_eq!(
            draft.input(
                CameraKey::Pan {
                    direction: Direction::Right,
                    coarse: false,
                },
                Some(steps()),
            ),
            CameraEffect::Rejected(CameraError::ZeroCount)
        );
        for digit in [4, 2, 9, 4, 9, 6, 7, 2, 9, 6, 7, 2, 9, 5] {
            draft.input(CameraKey::Digit(digit), None);
        }
        assert!(draft.count_overflowed());
        assert!(matches!(
            draft.input(CameraKey::Scale(ScaleDirection::In), None),
            CameraEffect::Rejected(CameraError::CountOverflow)
        ));
        assert_eq!(draft.pose(), FramingPose::identity());
        assert_eq!(
            draft.input(CameraKey::Commit, None),
            CameraEffect::Unchanged
        );
        assert_eq!(
            draft.input(CameraKey::Commit, None),
            CameraEffect::Rejected(CameraError::Closed)
        );
    }

    #[test]
    fn unchanged_enter_is_not_a_project_edit_but_reset_is() {
        let mut draft = new_draft();
        assert_eq!(
            draft.input(CameraKey::Commit, None),
            CameraEffect::Unchanged
        );
        let mut draft = new_draft();
        draft.input(CameraKey::Reset, None);
        assert!(matches!(
            draft.input(CameraKey::Commit, None),
            CameraEffect::Commit { reset: true, .. }
        ));
    }

    #[test]
    fn numeric_pose_update_preserves_exact_components_and_reset_intent() {
        let mut draft = new_draft();
        assert_eq!(
            draft.input(CameraKey::Reset, None),
            CameraEffect::Reset(FramingPose::identity())
        );
        let pose = FramingPose::new(
            ExactRatio::new(3, 5).unwrap(),
            ExactRatio::new(2, 5).unwrap(),
            ExactRatio::new(4, 3).unwrap(),
        )
        .unwrap();
        let CameraEffect::PoseChanged(updated) = draft.set_pose(pose) else {
            panic!("valid numeric pose must update the draft");
        };
        assert_eq!(updated, pose);
        assert!(draft.reset_requested());
        let invalid = FramingPose {
            center_x: ExactRatio::integer(99),
            ..updated
        };
        assert_eq!(
            draft.set_pose(invalid),
            CameraEffect::Rejected(CameraError::Framing(FramingError::PoseRange))
        );
        assert_eq!(draft.pose(), updated);
        assert!(draft.reset_requested());
    }

    #[test]
    fn logical_router_respects_focus_modifiers_and_key_repeat() {
        assert_eq!(
            route_camera_key(Key::H, Modifiers::SHIFT, false, false, false),
            Some(CameraKey::Pan {
                direction: Direction::Left,
                coarse: true
            })
        );
        assert_eq!(
            route_camera_key(Key::Plus, Modifiers::SHIFT, false, false, false),
            Some(CameraKey::Scale(ScaleDirection::In))
        );
        assert_eq!(
            route_camera_key(Key::Minus, Modifiers::NONE, false, false, true),
            Some(CameraKey::Scale(ScaleDirection::Out))
        );
        assert_eq!(
            route_camera_key(Key::Num4, Modifiers::NONE, false, false, true),
            Some(CameraKey::Ignore)
        );
        assert_eq!(
            route_camera_key(Key::Num4, Modifiers::SHIFT, false, false, false),
            Some(CameraKey::Other)
        );
        assert_eq!(
            route_camera_key(Key::F, Modifiers::NONE, false, false, true),
            Some(CameraKey::Ignore)
        );
        assert_eq!(
            route_camera_key(Key::Enter, Modifiers::NONE, true, false, false),
            Some(CameraKey::ClearCount)
        );
        assert_eq!(
            route_camera_key(Key::K, Modifiers::NONE, false, true, false),
            Some(CameraKey::ClearCount)
        );
        assert_eq!(
            route_camera_key(Key::Z, Modifiers::COMMAND, false, false, false),
            None
        );
        assert_eq!(
            route_camera_key(Key::Z, Modifiers::ALT, false, false, false),
            Some(CameraKey::Other)
        );

        let mut draft = new_draft();
        draft.input(CameraKey::Digit(3), None);
        assert_eq!(draft.input(CameraKey::Ignore, None), CameraEffect::None);
        assert_eq!(draft.pending_count(), Some(3));
        assert_eq!(draft.input(CameraKey::Other, None), CameraEffect::None);
        assert_eq!(draft.pending_count(), None);
    }
    fn region(center: [u32; 2], size: [u32; 2]) -> TargetRegion {
        TargetRegion { center, size }
    }

    #[test]
    fn region_editing_is_keyboard_operable_with_fields_counts_and_clamps() {
        let mut draft = new_draft();
        assert_eq!(
            draft.input(CameraKey::NewRegion, None),
            CameraEffect::RequestRegion { correction: false }
        );
        assert_eq!(
            draft.phase(),
            CameraPhase::Adjust,
            "the app supplies the start"
        );
        let start = region([500_000, 500_000], [200_000, 200_000]);
        assert_eq!(draft.open_region(start), CameraEffect::RegionChanged(start));
        assert_eq!(draft.phase(), CameraPhase::Region);
        assert_eq!(draft.region().unwrap().field(), RegionField::Center);
        // Center: arrows and h/j/k/l move it; counts repeat; Shift is 5 steps.
        draft.input(CameraKey::Digit(3), None);
        assert_eq!(
            draft.input(CameraKey::Arrow(Direction::Right), None),
            CameraEffect::RegionChanged(region([530_000, 500_000], [200_000, 200_000]))
        );
        assert_eq!(
            draft.input(
                CameraKey::Pan {
                    direction: Direction::Up,
                    coarse: true
                },
                None
            ),
            CameraEffect::RegionChanged(region([530_000, 450_000], [200_000, 200_000]))
        );
        // Tab: Width grows with Right/Up, shrinks with Left/Down.
        draft.input(CameraKey::Tab { reverse: false }, None);
        assert_eq!(draft.region().unwrap().field(), RegionField::Width);
        draft.input(CameraKey::Arrow(Direction::Up), None);
        draft.input(CameraKey::Tab { reverse: false }, None);
        assert_eq!(draft.region().unwrap().field(), RegionField::Height);
        draft.input(CameraKey::Digit(5), None);
        draft.input(CameraKey::Digit(0), None);
        draft.input(CameraKey::Arrow(Direction::Down), None);
        assert_eq!(
            draft.region().unwrap().region(),
            region([530_000, 450_000], [210_000, MIN_REGION_SIZE]),
            "sizes clamp at the minimum"
        );
        draft.input(CameraKey::Tab { reverse: true }, None);
        assert_eq!(draft.region().unwrap().field(), RegionField::Width);
        draft.input(CameraKey::Tab { reverse: true }, None);
        for digit in [9, 9] {
            draft.input(CameraKey::Digit(digit), None);
        }
        draft.input(CameraKey::Arrow(Direction::Left), None);
        assert_eq!(
            draft.region().unwrap().region().center[0],
            0,
            "centers clamp"
        );
        // A count cannot leak into Enter.
        draft.input(CameraKey::Digit(2), None);
        assert_eq!(
            draft.input(CameraKey::Commit, None),
            CameraEffect::Rejected(CameraError::CountNotSupported)
        );
        let committed = draft.region().unwrap().region();
        assert_eq!(
            draft.input(CameraKey::Commit, None),
            CameraEffect::RegionCommit(committed)
        );
        assert_eq!(draft.phase(), CameraPhase::Region, "open until saved");
        // Escape leaves only the rectangle; the Camera draft stays.
        assert_eq!(
            draft.input(CameraKey::Cancel, None),
            CameraEffect::RegionClosed
        );
        assert_eq!(draft.phase(), CameraPhase::Adjust);
        assert!(draft.region().is_none());
        assert_eq!(draft.pose(), FramingPose::identity());
    }

    #[test]
    fn follow_track_and_correction_keys_need_adjust_without_a_count() {
        let mut draft = new_draft();
        assert_eq!(
            draft.input(CameraKey::Follow, None),
            CameraEffect::ToggleFollow
        );
        assert_eq!(draft.input(CameraKey::Track, None), CameraEffect::Track);
        assert_eq!(
            draft.input(CameraKey::Correct, None),
            CameraEffect::RequestRegion { correction: true }
        );
        draft.input(CameraKey::Digit(2), None);
        assert_eq!(
            draft.input(CameraKey::Follow, None),
            CameraEffect::Rejected(CameraError::CountNotSupported)
        );
        draft.input(CameraKey::RefreshTargets, None);
        assert_eq!(
            draft.input(CameraKey::Track, None),
            CameraEffect::Rejected(CameraError::NotAdjusting)
        );
        assert_eq!(
            route_camera_key(Key::T, Modifiers::NONE, false, false, false),
            Some(CameraKey::Follow)
        );
        assert_eq!(
            route_camera_key(Key::T, Modifiers::SHIFT, false, false, false),
            Some(CameraKey::Track)
        );
        assert_eq!(
            route_camera_key(Key::N, Modifiers::NONE, false, false, false),
            Some(CameraKey::NewRegion)
        );
        assert_eq!(
            route_camera_key(Key::C, Modifiers::NONE, false, false, false),
            Some(CameraKey::Correct)
        );
        assert_eq!(
            route_camera_key(Key::N, Modifiers::NONE, false, false, true),
            Some(CameraKey::Ignore),
            "held keys never start a second rectangle"
        );
    }

    #[test]
    fn rebasing_keeps_adjustments_and_adopts_an_untouched_entry() {
        let mut draft = new_draft();
        let moved = FramingPose::new(
            ExactRatio::new(3, 5).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::integer(2),
        )
        .unwrap();
        let entry = FramingPose {
            scale: ExactRatio::integer(3),
            ..FramingPose::identity()
        };
        assert_eq!(new_draft().rebased(entry).pose(), entry);
        draft.set_pose(moved);
        draft.open_region(region([1, 2], [30_000, 30_000]));
        let mut rebased = draft.rebased(entry);
        assert_eq!(rebased.pose(), moved);
        assert_eq!(rebased.phase(), CameraPhase::Adjust);
        assert!(rebased.region().is_none());
        assert_eq!(
            rebased.input(CameraKey::Commit, None),
            CameraEffect::Commit {
                pose: moved,
                reset: false
            }
        );
    }
}
