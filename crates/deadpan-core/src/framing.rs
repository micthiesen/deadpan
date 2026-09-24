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
    pub value: FramingValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FramingValue {
    Static { pose: FramingPose },
    Envelope { envelope: FramingEnvelope },
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

    pub fn validate(&self) -> Result<(), FramingError> {
        match &self.value {
            FramingValue::Static { pose } => pose.validate(),
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

    /// Evaluate owner-output progress. Coordinates at both exact endpoints are
    /// accepted for inspection; picture traversal samples the half-open interior.
    pub fn evaluate(
        &self,
        local: ExactRatio,
        duration: FrameDuration,
    ) -> Result<FramingPose, FramingError> {
        self.validate()?;
        let frames = duration.frames();
        if frames == 0 || local.compare_integer(0).is_lt() || local.compare_integer(frames).is_gt()
        {
            return Err(FramingError::TimeRange);
        }
        let FramingValue::Envelope { envelope } = &self.value else {
            let FramingValue::Static { pose } = self.value else {
                unreachable!()
            };
            return Ok(pose);
        };
        let mut start = ExactRatio::ZERO;
        let mut from = envelope.initial;
        for segment in &envelope.segments {
            let end_frame = segment.end.checked_mul(ExactRatio::integer(frames))?;
            match numeric::compare(local, end_frame) {
                std::cmp::Ordering::Equal => return Ok(segment.pose),
                std::cmp::Ordering::Greater => {
                    start = segment.end;
                    from = segment.pose;
                }
                std::cmp::Ordering::Less => {
                    let start_frame = start.checked_mul(ExactRatio::integer(frames))?;
                    if local == start_frame || matches!(segment.curve, FramingCurve::Step) {
                        return Ok(from);
                    }
                    let progress = numeric::segment_progress(local, frames, start, segment.end)?;
                    return interpolate(from, segment.pose, segment.curve, progress);
                }
            }
        }
        Err(FramingError::TimeRange)
    }
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
        FramingCurve::Smoothstep => {
            let q = i128::from(FRAMING_NUMERIC_SCALE);
            let t = i128::from(progress);
            let shaped = ExactRatio::new(t * t * (3 * q - 2 * t), q * q)?.round_even()?;
            blend(
                from,
                to,
                u64::try_from(shaped).map_err(|_| FramingError::Overflow)?,
            )?
        }
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
    if records == 0 {
        return Ok(());
    }
    // Structural validation already proved ownership and bounded this walk.
    let mut pending = vec![(document.root(), 0usize)];
    while let Some((id, layers)) = pending.pop() {
        let layers = layers + usize::from(document.nodes()[id].framing.is_some());
        if layers > MAX_FRAMING_LAYERS {
            return Err(invalid(FramingError::Limit));
        }
        pending.extend(document.children(id).map(|child| (child, layers)));
    }
    Ok(())
}
