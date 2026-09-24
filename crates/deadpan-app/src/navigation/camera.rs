//! Pure draft state and bounded key meanings for Camera mode.
//!
//! The application supplies already-projected source-axis movement vectors and
//! owns target lookup, picture work, focus, and persistence. This module never
//! reads media or mutates a project.

use std::fmt;

use deadpan_core::{ExactRatio, FRAMING_NUMERIC_SCALE, FramingError, FramingPose};
use eframe::egui::{Key, Modifiers};

// The full authored scale range spans 4,096x. More than 171 five-percent
// steps necessarily leave that range in either direction.
const MAX_SCALE_STEPS: u32 = 171;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraPhase {
    Adjust,
    TargetPicker,
    Closed,
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
    Pan { direction: Direction, coarse: bool },
    Scale(ScaleDirection),
    RefreshTargets,
    Reset,
    Tab { reverse: bool },
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
    CycleField { reverse: bool },
    AdjustField { direction: Direction, count: u32 },
    Commit { pose: FramingPose, reset: bool },
    Unchanged,
    Cancel,
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
        })
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
        match key {
            CameraKey::Ignore => CameraEffect::None,
            CameraKey::ClearCount | CameraKey::Other => {
                self.clear_count();
                CameraEffect::None
            }
            CameraKey::Digit(digit) => self.digit(digit),
            CameraKey::RefreshTargets => self.refresh_targets(),
            CameraKey::Reset => self.reset(),
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
}
