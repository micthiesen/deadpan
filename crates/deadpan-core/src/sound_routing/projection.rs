//! One bounded old-to-final projection shared by route sampling, support
//! retention and cut envelopes. Artificial contiguous seams are removed before
//! their sample origins or edge policies can acquire meaning.

use super::{RootSoundOperation, invalid};
use crate::{
    DocumentError, ExactFrameRange, ExactRatio, FrameDuration, FrameRange, ProjectFrame,
    SoundRippleMap, SoundRippleNode, TimeError,
};

/// One retained unity-rate interval. A true cut uses the enclosing edit's
/// `cuts.after` at its start and `cuts.before` at its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootSoundKeep {
    pub input: FrameRange,
    pub output: FrameRange,
    pub start_cut: bool,
    pub end_cut: bool,
}

/// Checked projection of a closed root operation. Gaps are the complement of
/// these at most three destination intervals. This is derived state, not an
/// arbitrary persisted route or structural/media admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSoundProjection {
    input: FrameDuration,
    output: FrameDuration,
    keeps: [Option<RootSoundKeep>; 3],
}

impl RootSoundProjection {
    pub fn input_duration(&self) -> FrameDuration {
        self.input
    }

    pub fn output_duration(&self) -> FrameDuration {
        self.output
    }

    pub fn keeps(&self) -> impl Iterator<Item = RootSoundKeep> + '_ {
        self.keeps.iter().flatten().copied()
    }

    pub fn is_identity(&self) -> bool {
        self.input == self.output
            && self.keeps[1..].iter().all(Option::is_none)
            && self.keeps[0].is_some_and(|keep| {
                keep.input == keep.output
                    && keep.input.start() == ProjectFrame(0)
                    && keep.input.end() == ProjectFrame(self.input.frames())
            })
    }

    pub(super) fn new(
        operation: RootSoundOperation,
        input_frames: i64,
    ) -> Result<Self, DocumentError> {
        let input = FrameDuration::new(input_frames)?;
        let d = i128::from(input_frames);
        // Every tuple is (old start, old end, signed translation). Use i128
        // through clipping, including cancelled extreme In/Out contributions.
        let (output, raw) = match operation {
            RootSoundOperation::Insert { at, duration } => {
                if at.0 < 0 || at.0 > input_frames || duration == FrameDuration::ZERO {
                    return Err(invalid("sound insertion is outside its previous clock"));
                }
                let at = i128::from(at.0);
                let n = i128::from(duration.frames());
                (d + n, [Some((0, at, 0)), Some((at, d, n)), None])
            }
            RootSoundOperation::Delete { range } => {
                if range.start().0 < 0
                    || range.end().0 > input_frames
                    || range.duration() == FrameDuration::ZERO
                {
                    return Err(invalid("sound deletion is outside its previous clock"));
                }
                let a = i128::from(range.start().0);
                let b = i128::from(range.end().0);
                (d - (b - a), [Some((0, a, 0)), Some((b, d, a - b)), None])
            }
            RootSoundOperation::Replace { range, duration } => {
                if range.start().0 < 0
                    || range.end().0 > input_frames
                    || range.duration() == FrameDuration::ZERO
                    || duration == FrameDuration::ZERO
                {
                    return Err(invalid("sound replacement is outside its previous clock"));
                }
                let a = i128::from(range.start().0);
                let b = i128::from(range.end().0);
                let n = i128::from(duration.frames());
                (
                    d - (b - a) + n,
                    [Some((0, a, 0)), Some((b, d, a - b + n)), None],
                )
            }
            RootSoundOperation::Trim {
                range,
                in_frames,
                out_frames,
            } => {
                if range.start().0 < 0
                    || range.end().0 > input_frames
                    || range.duration() == FrameDuration::ZERO
                {
                    return Err(invalid("sound Trim entry is outside its previous clock"));
                }
                let t = i128::from(range.start().0);
                let u = i128::from(range.end().0);
                let i = i128::from(in_frames);
                let o = i128::from(out_frames);
                let k = o - i;
                let output = d + k;
                if output < t {
                    return Err(invalid(
                        "sound Trim output cannot remove its retained prefix",
                    ));
                }
                (
                    output,
                    [
                        Some((0, t, 0)),
                        Some((t.max(t + i), u.min(u + o), -i)),
                        Some((u.max(t - k), d, k)),
                    ],
                )
            }
        };
        let output = FrameDuration::new(i64::try_from(output).map_err(|_| TimeError::Overflow)?)?;
        let mut result = Self {
            input,
            output,
            keeps: [None; 3],
        };
        let mut count = 0usize;
        for (start, end, shift) in raw.into_iter().flatten() {
            let start = start.clamp(0, d);
            let end = end.clamp(0, d);
            if start >= end {
                continue;
            }
            let destination_start = start + shift;
            let destination_end = end + shift;
            if destination_start < 0 || destination_end > i128::from(output.frames()) {
                return Err(invalid("sound Keep lies outside its final clock"));
            }
            let next = RootSoundKeep {
                input: frame_range(start, end)?,
                output: frame_range(destination_start, destination_end)?,
                start_cut: false,
                end_cut: false,
            };
            if count != 0 {
                let previous = result.keeps[count - 1]
                    .as_mut()
                    .ok_or_else(|| invalid("sound Keep inventory is sparse"))?;
                if previous.input.end() > next.input.start()
                    || previous.output.end() > next.output.start()
                {
                    return Err(invalid("sound Keeps are not monotone and disjoint"));
                }
                if previous.input.end() == next.input.start()
                    && previous.output.end() == next.output.start()
                {
                    previous.input = FrameRange::new(previous.input.start(), next.input.end())?;
                    previous.output = FrameRange::new(previous.output.start(), next.output.end())?;
                    continue;
                }
            }
            let slot = result
                .keeps
                .get_mut(count)
                .ok_or_else(|| invalid("sound projection exceeds three Keeps"))?;
            *slot = Some(next);
            count += 1;
        }
        for keep in result.keeps.iter_mut().flatten() {
            keep.start_cut =
                keep.input.start() != ProjectFrame(0) || keep.output.start() != ProjectFrame(0);
            keep.end_cut = keep.input.end() != ProjectFrame(input.frames())
                || keep.output.end() != ProjectFrame(output.frames());
        }
        Ok(result)
    }

    pub(super) fn ripple_map(&self) -> Result<SoundRippleMap, DocumentError> {
        if self.output == FrameDuration::ZERO {
            return Err(invalid("empty root sound output must remove the event"));
        }
        let mut nodes = Vec::with_capacity(6);
        let mut cursor = 0;
        for keep in self.keeps() {
            if cursor < keep.output.start().0 {
                nodes.push(SoundRippleNode::Gap {
                    duration: ExactRatio::integer(keep.output.start().0 - cursor),
                });
            }
            nodes.push(SoundRippleNode::Keep {
                range: ExactFrameRange {
                    start: ExactRatio::integer(keep.input.start().0),
                    end: ExactRatio::integer(keep.input.end().0),
                },
            });
            cursor = keep.output.end().0;
        }
        if cursor < self.output.frames() {
            nodes.push(SoundRippleNode::Gap {
                duration: ExactRatio::integer(self.output.frames() - cursor),
            });
        }
        if nodes.len() > 1 {
            let parts = (0..nodes.len())
                .map(|n| u32::try_from(n).map_err(|_| TimeError::Overflow))
                .collect::<Result<Vec<_>, _>>()?;
            nodes.push(SoundRippleNode::Sequence { parts });
        }
        let root = nodes
            .len()
            .checked_sub(1)
            .ok_or_else(|| invalid("sound map is empty"))?;
        SoundRippleMap::new(
            ExactRatio::integer(self.input.frames()),
            u32::try_from(root).map_err(|_| TimeError::Overflow)?,
            nodes,
        )
    }
}

fn frame_range(start: i128, end: i128) -> Result<FrameRange, TimeError> {
    FrameRange::new(
        ProjectFrame(i64::try_from(start).map_err(|_| TimeError::Overflow)?),
        ProjectFrame(i64::try_from(end).map_err(|_| TimeError::Overflow)?),
    )
}
