//! Authored canvas framing on one structural owner's output clock.
//!
//! Timeline and segment selection are exact. Interior interpolation uses a
//! declared Q32 round-even numeric grid; it never changes the sampled time.

mod numeric;
mod preflight;
pub(crate) use preflight::check as preflight;

use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{ExactRatio, FrameDuration};

pub const FRAMING_NUMERIC_SCALE: u64 = 1 << 32;
pub const MAX_FRAMING_SEGMENTS: usize = 64;
pub const MAX_FRAMING_LAYERS: usize = 16;
pub const MAX_FRAMING_RECORDS: usize = 100_000;
const MAX_PROGRESS_DENOMINATOR: i128 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramingError {
    PoseRange,
    EnvelopeRange,
    Limit,
    TimeRange,
    Overflow,
}

impl fmt::Display for FramingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PoseRange => "framing center must be in -16..=17 and scale in 1/64..=64",
            Self::EnvelopeRange => {
                "framing segments must increase from zero to one with denominators at most 1000000"
            }
            Self::Limit => "framing exceeds the declared segment, record or layer limit",
            Self::TimeRange => {
                "framing evaluation requires a nonempty owner and a coordinate within its output"
            }
            Self::Overflow => "framing arithmetic exceeds its bounded numeric representation",
        })
    }
}
impl Error for FramingError {}
impl From<crate::TimeError> for FramingError {
    fn from(_: crate::TimeError) -> Self {
        Self::Overflow
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FramingPose {
    pub center_x: ExactRatio,
    pub center_y: ExactRatio,
    pub scale: ExactRatio,
}

impl Default for FramingPose {
    fn default() -> Self {
        Self::identity()
    }
}

impl FramingPose {
    pub fn identity() -> Self {
        Self {
            center_x: ExactRatio::new(1, 2).expect("constant positive denominator"),
            center_y: ExactRatio::new(1, 2).expect("constant positive denominator"),
            scale: ExactRatio::ONE,
        }
    }

    pub fn new(
        center_x: ExactRatio,
        center_y: ExactRatio,
        scale: ExactRatio,
    ) -> Result<Self, FramingError> {
        let pose = Self {
            center_x,
            center_y,
            scale,
        };
        pose.validate()?;
        Ok(pose)
    }

    pub fn validate(&self) -> Result<(), FramingError> {
        let centers = [self.center_x, self.center_y];
        if centers
            .iter()
            .any(|v| v.compare_integer(-16).is_lt() || v.compare_integer(17).is_gt())
            || numeric::compare(self.scale, ExactRatio::new(1, 64)?).is_lt()
            || self.scale.compare_integer(64).is_gt()
        {
            return Err(FramingError::PoseRange);
        }
        Ok(())
    }

    /// Explicit numeric precision for repeated Camera adjustments and derived
    /// interpolation. Authored construction itself does not round or clamp.
    pub fn quantized(&self) -> Result<Self, FramingError> {
        self.validate()?;
        Self::from_grid(self.grid()?)
    }

    fn grid(&self) -> Result<[i64; 3], FramingError> {
        Ok([
            numeric::quantize(self.center_x)?,
            numeric::quantize(self.center_y)?,
            numeric::quantize(self.scale)?,
        ])
    }

    fn from_grid(values: [i64; 3]) -> Result<Self, FramingError> {
        let ratio = |value| ExactRatio::new(i128::from(value), i128::from(FRAMING_NUMERIC_SCALE));
        Self::new(ratio(values[0])?, ratio(values[1])?, ratio(values[2])?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Framing {
    #[serde(default, skip_serializing_if = "FramingClock::is_owner_output")]
    pub clock: FramingClock,
    pub value: FramingValue,
}

/// The normalization domain of an authored camera path. A retained domain holds
/// its endpoint poses when the physical owner grows beyond that original path.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FramingClock {
    #[default]
    OwnerOutput,
    RetainedOutput {
        /// `authored_local = current_owner_local + offset`.
        offset: ExactRatio,
        duration: ExactRatio,
    },
}

impl FramingClock {
    fn is_owner_output(&self) -> bool {
        matches!(self, Self::OwnerOutput)
    }

    fn validate(self) -> Result<(), FramingError> {
        if let Self::RetainedOutput { duration, .. } = self
            && !duration.compare_integer(0).is_gt()
        {
            return Err(FramingError::TimeRange);
        }
        Ok(())
    }

    fn resolve(
        self,
        local: ExactRatio,
        owner_duration: ExactRatio,
    ) -> Result<(ExactRatio, ExactRatio), FramingError> {
        match self {
            Self::OwnerOutput => Ok((local, owner_duration)),
            Self::RetainedOutput { offset, duration } => {
                let local = local.checked_add(offset)?;
                let local = if local.compare_integer(0).is_lt() {
                    ExactRatio::ZERO
                } else if local.compare(duration).is_gt() {
                    duration
                } else {
                    local
                };
                Ok((local, duration))
            }
        }
    }
}

impl<'de> Deserialize<'de> for FramingClock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            OwnerOutput {},
            RetainedOutput {
                offset: ExactRatio,
                duration: ExactRatio,
            },
        }
        let clock = match Wire::deserialize(deserializer)? {
            Wire::OwnerOutput {} => Self::OwnerOutput,
            Wire::RetainedOutput { offset, duration } => Self::RetainedOutput { offset, duration },
        };
        clock.validate().map_err(de::Error::custom)?;
        Ok(clock)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FramingValue {
    Static {
        pose: FramingPose,
    },
    Envelope {
        envelope: FramingEnvelope,
    },
    /// Center on an attention target at the source time of the picture
    /// shown. Where the picture is not the target's asset
    /// or lies outside its span, `fallback` applies.
    Follow {
        target: crate::TargetId,
        scale: ExactRatio,
        fallback: FramingPose,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale_transition: Option<FollowScaleTransition>,
    },
}

/// One scale change across the owner's declared clock. Both the live and
/// fallback scales keep their own base; only their terminal factor is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FollowScaleTransition {
    pub factor: ExactRatio,
    pub curve: crate::framing_zoom::ZoomCurve,
}

/// Framing before the plan resolves a target against the provider picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluatedFraming<'a> {
    pub pose: FramingPose,
    pub follow: Option<(&'a crate::TargetId, ExactRatio)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FramingEnvelope {
    pub initial: FramingPose,
    #[serde(deserialize_with = "bounded_segments")]
    pub segments: Vec<FramingSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FramingSegment {
    pub end: ExactRatio,
    pub pose: FramingPose,
    pub curve: FramingCurve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FramingCurve {
    Step,
    Linear,
    Smoothstep,
    Cubic {
        control1: FramingPose,
        control2: FramingPose,
    },
}

impl Framing {
    pub fn static_pose(pose: FramingPose) -> Result<Self, FramingError> {
        let framing = Self {
            clock: FramingClock::OwnerOutput,
            value: FramingValue::Static { pose },
        };
        framing.validate()?;
        Ok(framing)
    }

    pub fn creep(
        from: FramingPose,
        to: FramingPose,
        curve: FramingCurve,
    ) -> Result<Self, FramingError> {
        let framing = Self {
            clock: FramingClock::OwnerOutput,
            value: FramingValue::Envelope {
                envelope: FramingEnvelope {
                    initial: from,
                    segments: vec![FramingSegment {
                        end: ExactRatio::ONE,
                        pose: to,
                        curve,
                    }],
                },
            },
        };
        framing.validate()?;
        Ok(framing)
    }

    /// Retain an existing path while a physical owner gains context. The old
    /// body moves from `x` to `x + prefix`; no pose or normalized key changes.
    /// A zero prefix freezes the previous duration for tail-only growth.
    pub fn prepend_owner_frames(
        &self,
        prefix: FrameDuration,
        previous_duration: FrameDuration,
    ) -> Result<Self, FramingError> {
        self.validate()?;
        if previous_duration == FrameDuration::ZERO {
            return Err(FramingError::TimeRange);
        }
        let (offset, duration) = match self.clock {
            FramingClock::OwnerOutput => (
                ExactRatio::ZERO,
                ExactRatio::integer(previous_duration.frames()),
            ),
            FramingClock::RetainedOutput { offset, duration } => (offset, duration),
        };
        let mut framing = self.clone();
        framing.clock = FramingClock::RetainedOutput {
            offset: offset.checked_sub(ExactRatio::integer(prefix.frames()))?,
            duration,
        };
        framing.validate()?;
        Ok(framing)
    }

    pub fn validate(&self) -> Result<(), FramingError> {
        self.clock.validate()?;
        match &self.value {
            FramingValue::Static { pose } => pose.validate(),
            FramingValue::Follow {
                scale,
                fallback,
                scale_transition,
                ..
            } => {
                fallback.validate()?;
                FramingPose::new(fallback.center_x, fallback.center_y, *scale)?;
                if let Some(transition) = scale_transition {
                    if !transition.factor.compare_integer(0).is_gt() {
                        return Err(FramingError::PoseRange);
                    }
                    for base in [*scale, fallback.scale] {
                        FramingPose::new(
                            fallback.center_x,
                            fallback.center_y,
                            base.checked_mul(transition.factor)?,
                        )?;
                    }
                }
                Ok(())
            }
            FramingValue::Envelope { envelope } => {
                envelope.initial.validate()?;
                if envelope.segments.is_empty() || envelope.segments.len() > MAX_FRAMING_SEGMENTS {
                    return Err(FramingError::Limit);
                }
                let mut previous = ExactRatio::ZERO;
                for segment in &envelope.segments {
                    if segment.end.denominator() > MAX_PROGRESS_DENOMINATOR
                        || !numeric::compare(segment.end, previous).is_gt()
                        || segment.end.compare_integer(1).is_gt()
                    {
                        return Err(FramingError::EnvelopeRange);
                    }
                    segment.pose.validate()?;
                    if let FramingCurve::Cubic { control1, control2 } = segment.curve {
                        control1.validate()?;
                        control2.validate()?;
                    }
                    previous = segment.end;
                }
                if previous != ExactRatio::ONE {
                    return Err(FramingError::EnvelopeRange);
                }
                Ok(())
            }
        }
    }

    pub fn record_count(&self) -> usize {
        match &self.value {
            FramingValue::Static { .. } => 1,
            FramingValue::Follow {
                scale_transition, ..
            } => 1 + usize::from(scale_transition.is_some()),
            FramingValue::Envelope { envelope } => {
                1 + envelope
                    .segments
                    .iter()
                    .map(|segment| {
                        if matches!(segment.curve, FramingCurve::Cubic { .. }) {
                            3
                        } else {
                            1
                        }
                    })
                    .sum::<usize>()
            }
        }
    }

    /// Evaluate in the recipe clock. Both current owner endpoints are accepted
    /// for inspection; picture traversal samples the half-open interior.
    pub fn evaluate(
        &self,
        local: ExactRatio,
        duration: FrameDuration,
    ) -> Result<FramingPose, FramingError> {
        self.evaluate_exact(local, ExactRatio::integer(duration.frames()))
    }

    /// Evaluate a derived owner clock without rounding its extent. This does
    /// not change authored frame counts or introduce fractional project frames.
    /// Both exact endpoints are accepted, as with `evaluate`.
    pub fn evaluate_exact(
        &self,
        local: ExactRatio,
        duration: ExactRatio,
    ) -> Result<FramingPose, FramingError> {
        self.evaluate_with_follow_exact(local, duration)
            .map(|value| value.pose)
    }

    /// Evaluate scale and fallback in the same owner clock. Provider target
    /// lookup remains the plan's responsibility and uses picture source time.
    pub fn evaluate_with_follow_exact(
        &self,
        local: ExactRatio,
        duration: ExactRatio,
    ) -> Result<EvaluatedFraming<'_>, FramingError> {
        self.validate()?;
        if !duration.compare_integer(0).is_gt()
            || local.compare_integer(0).is_lt()
            || numeric::compare(local, duration).is_gt()
        {
            return Err(FramingError::TimeRange);
        }
        let (local, duration) = self.clock.resolve(local, duration)?;
        let envelope = match &self.value {
            FramingValue::Static { pose } => {
                return Ok(EvaluatedFraming {
                    pose: *pose,
                    follow: None,
                });
            }
            // The target is resolved against the picture by the plan; on its
            // own, a follow evaluates to its fallback.
            FramingValue::Follow {
                target,
                scale,
                fallback,
                scale_transition,
            } => {
                let evaluated = |base| match scale_transition {
                    None => Ok(base),
                    Some(transition) => follow_scale(base, *transition, local, duration),
                };
                return Ok(EvaluatedFraming {
                    pose: FramingPose {
                        scale: evaluated(fallback.scale)?,
                        ..*fallback
                    },
                    follow: Some((target, evaluated(*scale)?)),
                });
            }
            FramingValue::Envelope { envelope } => envelope,
        };
        let mut start = ExactRatio::ZERO;
        let mut from = envelope.initial;
        for segment in &envelope.segments {
            // Do not materialize `end * duration` or `local / duration` as an
            // ExactRatio. Their intermediates can exceed i128 even when all
            // inputs and the selected segment are valid.
            match numeric::compare_progress(local, duration, segment.end)? {
                std::cmp::Ordering::Equal => {
                    return Ok(EvaluatedFraming {
                        pose: segment.pose,
                        follow: None,
                    });
                }
                std::cmp::Ordering::Greater => {
                    start = segment.end;
                    from = segment.pose;
                }
                std::cmp::Ordering::Less => {
                    if numeric::compare_progress(local, duration, start)?.is_eq()
                        || matches!(segment.curve, FramingCurve::Step)
                    {
                        return Ok(EvaluatedFraming {
                            pose: from,
                            follow: None,
                        });
                    }
                    let progress = numeric::segment_progress(local, duration, start, segment.end)?;
                    return interpolate(from, segment.pose, segment.curve, progress)
                        .map(|pose| EvaluatedFraming { pose, follow: None });
                }
            }
        }
        Err(FramingError::TimeRange)
    }
}

fn follow_scale(
    base: ExactRatio,
    transition: FollowScaleTransition,
    local: ExactRatio,
    duration: ExactRatio,
) -> Result<ExactRatio, FramingError> {
    let end = base.checked_mul(transition.factor)?;
    if local == duration {
        return Ok(end);
    }
    if local == ExactRatio::ZERO || transition.curve == crate::framing_zoom::ZoomCurve::Step {
        return Ok(base);
    }
    let progress = numeric::segment_progress(local, duration, ExactRatio::ZERO, ExactRatio::ONE)?;
    let progress = if transition.curve == crate::framing_zoom::ZoomCurve::Smoothstep {
        smoothstep(progress)?
    } else {
        progress
    };
    let value = numeric::lerp(numeric::quantize(base)?, numeric::quantize(end)?, progress)?;
    Ok(ExactRatio::new(
        i128::from(value),
        i128::from(FRAMING_NUMERIC_SCALE),
    )?)
}

fn smoothstep(progress: u64) -> Result<u64, FramingError> {
    let q = i128::from(FRAMING_NUMERIC_SCALE);
    let t = i128::from(progress);
    let shaped = ExactRatio::new(t * t * (3 * q - 2 * t), q * q)?.round_even()?;
    u64::try_from(shaped).map_err(|_| FramingError::Overflow)
}

fn interpolate(
    from: FramingPose,
    to: FramingPose,
    curve: FramingCurve,
    progress: u64,
) -> Result<FramingPose, FramingError> {
    let from = from.grid()?;
    let to = to.grid()?;
    let blend = |a: [i64; 3], b: [i64; 3], t| -> Result<[i64; 3], FramingError> {
        Ok([
            numeric::lerp(a[0], b[0], t)?,
            numeric::lerp(a[1], b[1], t)?,
            numeric::lerp(a[2], b[2], t)?,
        ])
    };
    let values = match curve {
        FramingCurve::Step => from,
        FramingCurve::Linear => blend(from, to, progress)?,
        FramingCurve::Smoothstep => blend(from, to, smoothstep(progress)?)?,
        FramingCurve::Cubic { control1, control2 } => {
            let c1 = control1.grid()?;
            let c2 = control2.grid()?;
            let a = blend(from, c1, progress)?;
            let b = blend(c1, c2, progress)?;
            let c = blend(c2, to, progress)?;
            blend(blend(a, b, progress)?, blend(b, c, progress)?, progress)?
        }
    };
    FramingPose::from_grid(values)
}

fn bounded_segments<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<FramingSegment>, D::Error> {
    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = Vec<FramingSegment>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("at most 64 framing segments")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|size| size > MAX_FRAMING_SEGMENTS)
            {
                return Err(de::Error::custom(FramingError::Limit));
            }
            let mut result = Vec::new();
            while let Some(segment) = seq.next_element()? {
                if result.len() == MAX_FRAMING_SEGMENTS {
                    return Err(de::Error::custom(FramingError::Limit));
                }
                result.push(segment);
            }
            Ok(result)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

fn invalid(error: FramingError) -> crate::DocumentError {
    crate::DocumentError::new(
        if error == FramingError::Limit {
            crate::DocumentErrorCode::LimitExceeded
        } else {
            crate::DocumentErrorCode::InvalidTree
        },
        error.to_string(),
    )
}

pub(crate) fn validate_nodes<'a>(
    nodes: impl Iterator<Item = &'a crate::BeatNode>,
) -> Result<usize, crate::DocumentError> {
    let mut records = 0usize;
    for node in nodes {
        if let Some(framing) = &node.framing {
            framing.validate().map_err(invalid)?;
            records = records
                .checked_add(framing.record_count())
                .ok_or_else(|| invalid(FramingError::Limit))?;
            if records > MAX_FRAMING_RECORDS {
                return Err(invalid(FramingError::Limit));
            }
        }
    }
    Ok(records)
}

pub(crate) fn validate_document(
    document: &crate::ProjectDocument,
) -> Result<(), crate::DocumentError> {
    let records = validate_nodes(document.nodes().values())?;
    for node in document.nodes().values() {
        if let Some(Framing {
            value: FramingValue::Follow { target, .. },
            ..
        }) = &node.framing
            && !document.targets().contains_key(target)
        {
            return Err(crate::DocumentError::new(
                crate::DocumentErrorCode::InvalidTree,
                "framing follows a target the project does not have",
            ));
        }
        if let crate::NodeKind::Repeat {
            escalation: Some(escalation),
            ..
        } = &node.kind
            && let Some(target) = escalation
                .zoom
                .as_ref()
                .and_then(|zoom| zoom.target.as_ref())
            && !document.targets().contains_key(target)
        {
            return Err(crate::DocumentError::new(
                crate::DocumentErrorCode::InvalidTree,
                "an escalating zoom moves toward a target the project does not have",
            ));
        }
    }
    // A Repeat's zoom escalation adds a posed layer of its own.
    let escalated = |node: &crate::BeatNode| {
        matches!(
            &node.kind,
            crate::NodeKind::Repeat {
                escalation: Some(crate::RepeatEscalation { zoom: Some(_), .. }),
                ..
            }
        )
    };
    if records == 0 && !document.nodes().values().any(escalated) {
        return Ok(());
    }
    // Structural validation already proved ownership and bounded this walk.
    let mut pending = vec![(document.root(), 0usize)];
    while let Some((id, layers)) = pending.pop() {
        let node = &document.nodes()[id];
        let layers = layers + usize::from(node.framing.is_some()) + usize::from(escalated(node));
        if layers > MAX_FRAMING_LAYERS {
            return Err(invalid(FramingError::Limit));
        }
        pending.extend(document.children(id).map(|child| (child, layers)));
    }
    Ok(())
}

#[cfg(test)]
mod follow_transition_tests {
    use super::*;
    use crate::framing_zoom::ZoomCurve;

    fn ratio(n: i128, d: i128) -> ExactRatio {
        ExactRatio::new(n, d).unwrap()
    }
    fn framing(curve: ZoomCurve) -> Framing {
        Framing {
            clock: FramingClock::OwnerOutput,
            value: FramingValue::Follow {
                target: crate::TargetId::new("speaker").unwrap(),
                scale: ratio(2, 1),
                fallback: FramingPose::new(ratio(1, 3), ratio(2, 7), ratio(1, 2)).unwrap(),
                scale_transition: Some(FollowScaleTransition {
                    factor: ratio(2, 1),
                    curve,
                }),
            },
        }
    }

    #[test]
    fn follow_transition_curves_preserve_exact_fallback_centers_and_endpoint_scales() {
        for (curve, live, fallback) in [
            (ZoomCurve::Step, ratio(2, 1), ratio(1, 2)),
            (ZoomCurve::Linear, ratio(5, 2), ratio(5, 8)),
            (ZoomCurve::Smoothstep, ratio(37, 16), ratio(37, 64)),
        ] {
            let recipe = framing(curve);
            let value = recipe
                .evaluate_with_follow_exact(ratio(1, 1), ratio(4, 1))
                .unwrap();
            assert_eq!(value.follow.unwrap().1, live);
            assert_eq!(value.pose.scale, fallback);
            assert_eq!(value.pose.center_x, ratio(1, 3));
            assert_eq!(value.pose.center_y, ratio(2, 7));
            assert_eq!(
                recipe.evaluate_exact(ratio(1, 1), ratio(4, 1)).unwrap(),
                value.pose
            );
            assert_eq!(
                recipe
                    .evaluate_with_follow_exact(ratio(4, 1), ratio(4, 1))
                    .unwrap()
                    .follow
                    .unwrap()
                    .1,
                ratio(4, 1)
            );
        }
        let mut recipe = framing(ZoomCurve::Linear);
        if let FramingValue::Follow {
            scale, fallback, ..
        } = &mut recipe.value
        {
            *scale = ratio(3, 5);
            fallback.scale = ratio(5, 7);
        }
        for local in [ExactRatio::ZERO, ExactRatio::ONE] {
            let value = recipe
                .evaluate_with_follow_exact(local, ExactRatio::ONE)
                .unwrap();
            assert_eq!(
                value.follow.unwrap().1,
                if local == ExactRatio::ZERO {
                    ratio(3, 5)
                } else {
                    ratio(6, 5)
                }
            );
            assert_eq!(
                value.pose.scale,
                if local == ExactRatio::ZERO {
                    ratio(5, 7)
                } else {
                    ratio(10, 7)
                }
            );
        }
    }

    #[test]
    fn follow_transition_non_grid_scales_match_an_independent_integer_grid_oracle() {
        let round = |n: i128, d: i128| {
            let whole = n / d;
            let remainder = n % d;
            whole + i128::from(remainder * 2 > d || (remainder * 2 == d && whole % 2 != 0))
        };
        let q = i128::from(FRAMING_NUMERIC_SCALE);
        let progress = round(3 * q, 7);
        let expected = |n: i128, d: i128| {
            let a = round(n * q, d);
            let b = round(2 * n * q, d);
            ratio(round(a * (q - progress) + b * progress, q), q)
        };
        let mut recipe = framing(ZoomCurve::Linear);
        if let FramingValue::Follow {
            scale, fallback, ..
        } = &mut recipe.value
        {
            *scale = ratio(3, 5);
            fallback.scale = ratio(5, 7);
        }
        let value = recipe
            .evaluate_with_follow_exact(ratio(3, 1), ratio(7, 1))
            .unwrap();
        assert_eq!(value.follow.unwrap().1, expected(3, 5));
        assert_eq!(value.pose.scale, expected(5, 7));
        assert_eq!(value.pose.center_x, ratio(1, 3));
        for curve in [ZoomCurve::Step, ZoomCurve::Linear, ZoomCurve::Smoothstep] {
            let value = framing(curve);
            let raw = serde_json::to_string(&value).unwrap();
            let decoded: Framing = serde_json::from_str(&raw).unwrap();
            decoded.validate().unwrap();
            assert_eq!(decoded, value);
            assert!(decoded.evaluate_exact(ratio(1, 2), ExactRatio::ONE).is_ok());
        }
    }

    #[test]
    fn follow_transition_retained_fractional_clock_and_growth_keep_original_samples() {
        let recipe = framing(ZoomCurve::Smoothstep);
        let grown = recipe
            .prepend_owner_frames(
                FrameDuration::new(3).unwrap(),
                FrameDuration::new(8).unwrap(),
            )
            .unwrap();
        for n in 0..=16 {
            let local = ratio(n, 2);
            let before = recipe
                .evaluate_with_follow_exact(local, ratio(8, 1))
                .unwrap();
            let after = grown
                .evaluate_with_follow_exact(local.checked_add(ratio(3, 1)).unwrap(), ratio(13, 1))
                .unwrap();
            assert_eq!(before.pose, after.pose);
            assert_eq!(before.follow, after.follow);
        }
        assert_eq!(
            grown
                .evaluate_exact(ExactRatio::ZERO, ratio(13, 1))
                .unwrap(),
            recipe
                .evaluate_exact(ExactRatio::ZERO, ratio(8, 1))
                .unwrap()
        );
        assert_eq!(
            grown.evaluate_exact(ratio(13, 1), ratio(13, 1)).unwrap(),
            recipe.evaluate_exact(ratio(8, 1), ratio(8, 1)).unwrap()
        );
        let mut fractional = recipe.clone();
        fractional.clock = FramingClock::RetainedOutput {
            offset: ratio(1, 3),
            duration: ratio(8, 3),
        };
        assert_eq!(
            fractional.evaluate_exact(ratio(1, 3), ratio(1, 1)).unwrap(),
            recipe.evaluate_exact(ratio(1, 1), ratio(4, 1)).unwrap()
        );
    }

    #[test]
    fn follow_transition_typed_aggregate_admission_uses_the_real_record_limit() {
        let mut node = crate::BeatNode::sequence("Follow owner", Vec::new());
        node.framing = Some(framing(ZoomCurve::Linear));
        assert_eq!(
            validate_nodes(std::iter::repeat_n(&node, MAX_FRAMING_RECORDS / 2)).unwrap(),
            MAX_FRAMING_RECORDS
        );
        assert!(validate_nodes(std::iter::repeat_n(&node, MAX_FRAMING_RECORDS / 2 + 1)).is_err());
        if let Some(Framing {
            value: FramingValue::Follow {
                scale_transition, ..
            },
            ..
        }) = &mut node.framing
        {
            *scale_transition = None;
        }
        assert_eq!(
            validate_nodes(std::iter::repeat_n(&node, MAX_FRAMING_RECORDS)).unwrap(),
            MAX_FRAMING_RECORDS
        );
        assert!(validate_nodes(std::iter::repeat_n(&node, MAX_FRAMING_RECORDS + 1)).is_err());
    }

    #[test]
    fn follow_transition_validates_both_branches_and_closed_raw_vocabulary() {
        let mut recipe = framing(ZoomCurve::Linear);
        for bad in [ratio(0, 1), ratio(-1, 1), ratio(128, 1), ratio(1, 128)] {
            if let FramingValue::Follow {
                scale_transition, ..
            } = &mut recipe.value
            {
                scale_transition.as_mut().unwrap().factor = bad;
            }
            assert!(recipe.validate().is_err());
        }
        recipe = framing(ZoomCurve::Linear);
        if let FramingValue::Follow { fallback, .. } = &mut recipe.value {
            fallback.scale = ratio(40, 1);
        }
        assert!(
            recipe.validate().is_err(),
            "fallback endpoint bounds are independent of live scale"
        );
        let factor = serde_json::to_string(&ratio(2, 1)).unwrap();
        for (transition, reason) in [
            (
                format!(r#"{{"factor":{factor},"curve":"cubic"}}"#),
                "unknown variant",
            ),
            (
                format!(r#"{{"factor":{factor},"factor":{factor},"curve":"linear"}}"#),
                "duplicate field",
            ),
            (
                format!(r#"{{"factor":{factor},"curve":"linear","segments":[]}}"#),
                "unknown field",
            ),
        ] {
            let error = serde_json::from_str::<FollowScaleTransition>(&transition).unwrap_err();
            assert!(error.to_string().contains(reason), "{error}");
        }
        let mut fixed = framing(ZoomCurve::Linear);
        if let FramingValue::Follow {
            scale_transition, ..
        } = &mut fixed.value
        {
            *scale_transition = None;
        }
        let encoded = serde_json::to_value(&fixed).unwrap();
        assert!(encoded["value"].get("scale_transition").is_none());
        assert_eq!(fixed.record_count(), 1);
        assert_eq!(framing(ZoomCurve::Linear).record_count(), 2);
        assert_eq!(serde_json::from_value::<Framing>(encoded).unwrap(), fixed);
    }
}
