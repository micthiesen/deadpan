//! Incremental, cursor-priority tracking ranges (specification §11.4).
//!
//! Vision's tracker is causal: it follows a subject from one picture to the
//! next, seeded by a region. A tracking range therefore splits into one
//! *segment* per manual keyframe (the initial selection and every later
//! correction), each tracked forward from its own keyframe, and each segment
//! into bounded *ranges* of whole analysed strides. The first range of a
//! segment is seeded by its keyframe; a later range is seeded at the previous
//! range's last analysed picture with the tracker's last reported region there
//! ([`continuation_region`]), so it continues the same subject.
//!
//! [`RangePlan::next`] picks, among ranges whose seed is available, the one
//! nearest the visible cursor; ties prefer the range at or after the cursor.
//! Within a segment that reaches the cursor's neighbourhood forward from its
//! keyframe; the neighbourhood before the cursor in an earlier segment is
//! reached forward from that segment's earlier keyframe, never by reverse
//! decoding. Completed ranges may come from a private cache, so a cancelled
//! run resumes from them.
//!
//! [`RangePlan::assemble`] applies the unchanged `deadpan-track-1` policy to
//! the concatenated observations of the completed prefix. Over the same
//! observations it equals one run of [`TrackedPath::track`] (and
//! [`TrackedPath::correct_and_retrack`] for later keyframes): ranges only
//! change when Vision is re-seeded, never how its reports are judged.

use serde::{Deserialize, Serialize};

use super::{
    Keyframe, MAX_TRACK_PICTURES, NormalizedRect, RawObservation, TrackError, TrackPolicy,
    TrackStop, TrackedPath,
};

/// Pictures per range in the app: about ten seconds at 24 fps, so the cost of
/// launching and verifying a worker per range stays small beside tracking.
pub const TRACK_RANGE_PICTURES: usize = 240;
/// Ranges one plan may hold.
pub const MAX_TRACK_RANGES: usize = 4_096;

/// One bounded range, in ordinals of the tracked pictures `[0, pictures)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedRange {
    /// The keyframe whose segment this range belongs to.
    pub segment: usize,
    /// The first picture the range owns.
    pub first: usize,
    /// Exclusive end of the owned pictures.
    pub end: usize,
    /// The picture the worker run is seeded on: `first` for a segment's first
    /// range, otherwise the previous range's last analysed picture, whose
    /// observation belongs to that previous range.
    pub seed: usize,
}

impl PlannedRange {
    /// Whether the range continues an earlier one rather than starting at a
    /// keyframe.
    pub fn continues(&self) -> bool {
        self.seed != self.first
    }

    /// How far the range lies from `cursor`, in pictures.
    fn distance(&self, cursor: usize) -> usize {
        if cursor < self.first {
            self.first - cursor
        } else if cursor >= self.end {
            cursor + 1 - self.end
        } else {
            0
        }
    }
}

/// The ranges of one tracking request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangePlan {
    pictures: usize,
    stride: usize,
    keyframes: Vec<usize>,
    ranges: Vec<PlannedRange>,
}

impl RangePlan {
    /// Split `pictures` tracked pictures into ranges of at most `width`
    /// pictures (rounded up to whole strides). `keyframes` are the ordinals of
    /// the manual keyframes, beginning with 0.
    pub fn new(
        pictures: usize,
        keyframes: &[usize],
        stride: u32,
        width: usize,
    ) -> Result<Self, TrackError> {
        if pictures == 0 || pictures > MAX_TRACK_PICTURES {
            return Err(TrackError::Invalid(
                "tracking ranges need 1..=36000 pictures",
            ));
        }
        if stride == 0 || stride > super::MAX_TRACK_STRIDE || width == 0 {
            return Err(TrackError::Invalid(
                "tracking range width or stride is invalid",
            ));
        }
        if keyframes.first() != Some(&0)
            || keyframes.windows(2).any(|pair| pair[0] >= pair[1])
            || keyframes.last().is_some_and(|&last| last >= pictures)
            || keyframes.len() > super::MAX_TRACK_KEYFRAMES
        {
            return Err(TrackError::Invalid(
                "keyframes must be ordered inside the tracked pictures and begin them",
            ));
        }
        let stride = stride as usize;
        let width = width.div_ceil(stride) * stride;
        let mut ranges = Vec::new();
        for (segment, &start) in keyframes.iter().enumerate() {
            let segment_end = keyframes.get(segment + 1).copied().unwrap_or(pictures);
            let mut first = start;
            while first < segment_end {
                if ranges.len() == MAX_TRACK_RANGES {
                    return Err(TrackError::Limit("tracking range"));
                }
                let end = (first + width).min(segment_end);
                ranges.push(PlannedRange {
                    segment,
                    first,
                    end,
                    seed: if first == start {
                        first
                    } else {
                        first - stride
                    },
                });
                first = end;
            }
        }
        Ok(Self {
            pictures,
            stride,
            keyframes: keyframes.to_vec(),
            ranges,
        })
    }

    pub fn ranges(&self) -> &[PlannedRange] {
        &self.ranges
    }

    pub fn pictures(&self) -> usize {
        self.pictures
    }

    pub fn stride(&self) -> u32 {
        self.stride as u32
    }

    /// The analysed pictures a range owns, as ordinals.
    pub fn analysed(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        let range = self.ranges[index];
        let start = self.keyframes[range.segment];
        (range.first..range.end)
            .filter(move |ordinal| (ordinal - start).is_multiple_of(self.stride))
    }

    /// Whether range `index` can run: it starts a segment or its predecessor
    /// in the segment is complete.
    pub fn ready(&self, index: usize, completed: &[bool]) -> bool {
        let range = self.ranges[index];
        !range.continues() || (index > 0 && completed.get(index - 1) == Some(&true))
    }

    /// The incomplete ready range nearest `cursor` (an ordinal, clamped into
    /// the tracked pictures). Ties prefer the range at or after the cursor,
    /// then the earlier one. `None` when every range is complete.
    pub fn next(&self, completed: &[bool], cursor: usize) -> Option<usize> {
        let cursor = cursor.min(self.pictures - 1);
        (0..self.ranges.len())
            .filter(|&index| completed.get(index) != Some(&true) && self.ready(index, completed))
            .min_by_key(|&index| {
                let range = self.ranges[index];
                (range.distance(cursor), range.end <= cursor, range.first)
            })
    }

    /// Exclusive end ordinal of the complete prefix of ranges.
    pub fn covered(&self, completed: &[bool]) -> usize {
        self.ranges
            .iter()
            .zip(completed.iter().chain(std::iter::repeat(&false)))
            .take_while(|(_, done)| **done)
            .last()
            .map_or(0, |(range, _)| range.end)
    }

    /// Apply `policy` to the completed prefix. `pictures` lists every tracked
    /// picture PTS, `keyframes` the manual keyframes at the plan's ordinals,
    /// and `observations[i]` range `i`'s own observations (for a continuation,
    /// without its seed picture's). A complete plan ends at `end_pts` with
    /// `stop`; a partial one ends before the first incomplete range with
    /// [`TrackStop::RangeEnd`]. `None` before the first range completes.
    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        &self,
        policy: TrackPolicy,
        display_aspect: f64,
        pictures: &[i64],
        end_pts: i64,
        stop: TrackStop,
        keyframes: &[Keyframe],
        observations: &[Option<Vec<RawObservation>>],
    ) -> Result<Option<TrackedPath>, TrackError> {
        if pictures.len() != self.pictures
            || keyframes.len() != self.keyframes.len()
            || observations.len() != self.ranges.len()
            || keyframes
                .iter()
                .zip(&self.keyframes)
                .any(|(keyframe, &ordinal)| pictures[ordinal] != keyframe.pts)
        {
            return Err(TrackError::Invalid(
                "tracking ranges do not match their pictures or keyframes",
            ));
        }
        let completed: Vec<bool> = observations.iter().map(Option::is_some).collect();
        let covered = self.covered(&completed);
        if covered == 0 {
            return Ok(None);
        }
        let (prefix_end, prefix_stop) = if covered == self.pictures {
            (end_pts, stop)
        } else {
            (pictures[covered], TrackStop::RangeEnd)
        };
        let mut path: Option<TrackedPath> = None;
        for (segment, keyframe) in keyframes.iter().enumerate() {
            let start = self.keyframes[segment];
            if start >= covered {
                break;
            }
            let end = self
                .keyframes
                .get(segment + 1)
                .copied()
                .unwrap_or(self.pictures)
                .min(covered);
            let mut segment_observations = Vec::new();
            for (range, owned) in self.ranges.iter().zip(observations) {
                if range.segment == segment && range.end <= covered {
                    segment_observations
                        .extend_from_slice(owned.as_deref().expect("covered ranges completed"));
                }
            }
            let segment_pictures = &pictures[start..end];
            path = Some(match path {
                None => TrackedPath::track(
                    policy,
                    display_aspect,
                    &pictures[..covered],
                    prefix_end,
                    prefix_stop,
                    *keyframe,
                    &segment_observations,
                )?,
                Some(mut path) => {
                    path.correct_and_retrack(*keyframe, segment_pictures, &segment_observations)?;
                    path
                }
            });
        }
        Ok(path)
    }
}

/// The region a continuation range is seeded with: the tracker's last
/// reported region in the previous range, or that range's own seed when the
/// tracker reported none there. Vision feeds each result back as its next
/// input and keeps the previous input after a failed picture.
pub fn continuation_region(
    previous_seed: NormalizedRect,
    previous: &[RawObservation],
) -> NormalizedRect {
    previous
        .iter()
        .rev()
        .find_map(|observation| observation.region)
        .unwrap_or(previous_seed)
}

#[cfg(test)]
mod tests;
