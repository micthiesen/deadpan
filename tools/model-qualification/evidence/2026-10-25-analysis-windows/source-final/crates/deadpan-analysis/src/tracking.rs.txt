//! Selected-target tracking paths in the Original's pictures.
//!
//! A tracker (Apple Vision in the `deadpan-track` worker) reports one raw
//! [`RawObservation`] per analysed picture: a normalized bounding box and the
//! tracker's confidence. [`TrackedPath::track`] applies the bounded policy of
//! [`TRACK_RULE`] to those observations and never trusts them blindly:
//!
//! 1. The user's selected region is a [`Keyframe`]; its picture is
//!    [`TrackState::Manual`] and every observation there is ignored.
//! 2. An observation is *accepted* when it has a region, its confidence is at
//!    least [`TrackPolicy::min_confidence`], and it is plausible against the
//!    last accepted (or manual) region: its center moves at most
//!    `max_center_step` per elapsed picture, never more than
//!    `max_center_jump`, both as fractions of the displayed picture's longer
//!    side (so distance is measured in the display aspect, not stretched by
//!    normalization), and its area changes by at most `max_area_ratio`. An
//!    implausible jump is a loss, never a new subject. Accepted observations
//!    are [`TrackState::Tracked`] with the tracker's confidence.
//! 3. Rejected observations between two accepted ones that are at most
//!    `max_interpolated_gap + 1` *pictures* apart (whatever the stride) are
//!    [`TrackState::Interpolated`]: linear in x, y, width and height by
//!    picture ordinal, with the lower of the two neighbours' confidences
//!    (a manual neighbour counts as 1), never the rejected observation's.
//! 4. When the gap grows beyond that bound, or no accepted observation closes
//!    it before the segment ends, its observations are [`TrackState::Lost`] at
//!    the last confident region, without confidence. Tracking then does not
//!    resume by itself:
//!    every later observation in the segment is Lost when it would still be
//!    rejected and [`TrackState::Held`] when the tracker reports a confident,
//!    plausible box, both at the last confident region, until a manual
//!    keyframe corrects it.
//! 5. A correction ([`TrackedPath::correct`]) inserts a keyframe and
//!    invalidates only the range from it to the next keyframe or the path's
//!    end; [`TrackedPath::retrack`] replaces that range alone.
//!
//! Tracking stops at the first shot boundary after its start by default
//! ([`tracking_end`]). Regions are normalized to the displayed picture with a
//! top-left origin: rotation is applied ([`NormalizedRect::coded_from_displayed`])
//! and sample aspect ratio scales both axes uniformly, so it does not change
//! normalized coordinates. A path is an annotation; it never edits a project.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The versioned policy implemented by [`TrackedPath::track`].
pub const TRACK_RULE: &str = "deadpan-track-1";
/// Pictures one tracking attempt may decode: 25 minutes at 24 fps.
pub const MAX_TRACK_PICTURES: usize = 36_000;
pub const MAX_TRACK_KEYFRAMES: usize = 1_024;
/// The longest analysed-picture stride.
pub const MAX_TRACK_STRIDE: u32 = 30;
/// The smallest normalized width or height a region may have.
pub const MIN_REGION_EXTENT: f64 = 1.0 / 4096.0;

/// Default lowest accepted tracker confidence.
pub const MIN_CONFIDENCE: f32 = 0.3;
/// Default longest gap, in pictures between confident neighbours, that may
/// be interpolated.
pub const MAX_INTERPOLATED_GAP: u32 = 6;
/// Default largest center movement per elapsed picture, as a fraction of the
/// displayed picture's longer side.
pub const MAX_CENTER_STEP: f64 = 0.1;
/// Default largest center movement between accepted observations however many
/// pictures elapsed.
pub const MAX_CENTER_JUMP: f64 = 0.35;
/// Default largest area growth or shrink factor between accepted observations.
pub const MAX_AREA_RATIO: f64 = 2.0;

const EDGE_TOLERANCE: f64 = 1e-9;

#[derive(Debug, Error, PartialEq)]
pub enum TrackError {
    #[error("tracking exceeds its {0} limit")]
    Limit(&'static str),
    #[error("tracking is invalid: {0}")]
    Invalid(&'static str),
}

/// A box in the displayed picture: top-left origin, every edge in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RectWire", into = "RectWire")]
pub struct NormalizedRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RectWire {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl TryFrom<RectWire> for NormalizedRect {
    type Error = TrackError;

    fn try_from(value: RectWire) -> Result<Self, Self::Error> {
        // Stored values are checked, never adjusted, so they round-trip.
        let rect = Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        };
        rect.check()?;
        Ok(rect)
    }
}

impl From<NormalizedRect> for RectWire {
    fn from(value: NormalizedRect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

impl NormalizedRect {
    /// A region wholly inside the picture. Edges up to 1e-9 outside it are
    /// clamped, so rotated boxes round-trip.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, TrackError> {
        Self {
            x,
            y,
            width,
            height,
        }
        .check()?;
        let (left, top) = (x.max(0.0), y.max(0.0));
        let width = if x < 0.0 || x + width > 1.0 {
            (x + width).min(1.0) - left
        } else {
            width
        };
        let height = if y < 0.0 || y + height > 1.0 {
            (y + height).min(1.0) - top
        } else {
            height
        };
        Ok(Self {
            x: left,
            y: top,
            width,
            height,
        })
    }

    fn check(&self) -> Result<(), TrackError> {
        let Self {
            x,
            y,
            width,
            height,
        } = *self;
        if ![x, y, width, height].iter().all(|value| value.is_finite()) {
            return Err(TrackError::Invalid("region is not finite"));
        }
        if width < MIN_REGION_EXTENT || height < MIN_REGION_EXTENT {
            return Err(TrackError::Invalid("region is smaller than its minimum"));
        }
        if x < -EDGE_TOLERANCE
            || y < -EDGE_TOLERANCE
            || x + width > 1.0 + EDGE_TOLERANCE
            || y + height > 1.0 + EDGE_TOLERANCE
        {
            return Err(TrackError::Invalid("region lies outside the picture"));
        }
        Ok(())
    }

    /// The part of an arbitrary box inside the picture, if any remains at
    /// least [`MIN_REGION_EXTENT`] in both directions.
    pub fn clipped(x: f64, y: f64, width: f64, height: f64) -> Option<Self> {
        if ![x, y, width, height].iter().all(|value| value.is_finite()) {
            return None;
        }
        let (left, top) = (x.max(0.0), y.max(0.0));
        let (right, bottom) = ((x + width).min(1.0), (y + height).min(1.0));
        Self::new(left, top, right - left, bottom - top).ok()
    }

    pub fn x(&self) -> f64 {
        self.x
    }
    pub fn y(&self) -> f64 {
        self.y
    }
    pub fn width(&self) -> f64 {
        self.width
    }
    pub fn height(&self) -> f64 {
        self.height
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Linear interpolation of every edge; `t` in `[0, 1]`.
    pub fn lerp(&self, other: &Self, t: f64) -> Self {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        // Convex combinations of valid boxes stay inside the picture.
        Self {
            x: mix(self.x, other.x),
            y: mix(self.y, other.y),
            width: mix(self.width, other.width),
            height: mix(self.height, other.height),
        }
    }

    /// The same box in coded (stored) pixel orientation, for a stream displayed
    /// after `quarter_turns` clockwise quarter turns.
    pub fn coded_from_displayed(self, quarter_turns: u8) -> Self {
        (0..quarter_turns % 4).fold(self, |rect, _| rect.counterclockwise())
    }

    /// The same box in displayed orientation; inverse of
    /// [`Self::coded_from_displayed`].
    pub fn displayed_from_coded(self, quarter_turns: u8) -> Self {
        (0..quarter_turns % 4).fold(self, |rect, _| rect.clockwise())
    }

    /// Rotate the picture a quarter turn clockwise: point (u, v) moves to
    /// (1 - v, u).
    fn clockwise(self) -> Self {
        Self {
            x: (1.0 - self.y - self.height).max(0.0),
            y: self.x,
            width: self.height,
            height: self.width,
        }
    }

    /// Rotate the picture a quarter turn counterclockwise: (u, v) to (v, 1 - u).
    fn counterclockwise(self) -> Self {
        Self {
            x: self.y,
            y: (1.0 - self.x - self.width).max(0.0),
            width: self.height,
            height: self.width,
        }
    }
}

/// A manual position at one picture, in the source's stream time base.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keyframe {
    pub pts: i64,
    pub region: NormalizedRect,
}

/// One tracker report for an analysed picture, as the worker produced it.
/// `region` is absent when the tracker failed on the picture.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawObservation {
    pub pts: i64,
    pub region: Option<NormalizedRect>,
    pub confidence: f32,
}

/// A worker's whole report: the PTS of every picture it decoded in the range,
/// in order, and one observation per analysed picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawTrack {
    pub decoded: Vec<i64>,
    pub observations: Vec<RawObservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackState {
    /// A user keyframe.
    Manual,
    /// An accepted tracker observation.
    Tracked,
    /// A bounded short gap filled between confident neighbours.
    Interpolated,
    /// A rejected observation that could not be interpolated; holds the last
    /// confident region.
    Lost,
    /// After a loss, a confident observation that is not trusted until a
    /// manual correction; holds the last confident region.
    Held,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackSample {
    pub pts: i64,
    pub region: NormalizedRect,
    /// Confidence in this sample's region: the tracker's for `tracked`, the
    /// lower neighbour's for `interpolated`; absent for `manual`, `lost` and
    /// `held` samples, which hold a position the tracker did not confirm.
    pub confidence: Option<f32>,
    pub state: TrackState,
}

/// The bounded acceptance and gap policy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackPolicy {
    pub min_confidence: f32,
    pub max_interpolated_gap: u32,
    pub max_center_step: f64,
    pub max_center_jump: f64,
    pub max_area_ratio: f64,
}

impl Default for TrackPolicy {
    fn default() -> Self {
        Self {
            min_confidence: MIN_CONFIDENCE,
            max_interpolated_gap: MAX_INTERPOLATED_GAP,
            max_center_step: MAX_CENTER_STEP,
            max_center_jump: MAX_CENTER_JUMP,
            max_area_ratio: MAX_AREA_RATIO,
        }
    }
}

impl TrackPolicy {
    pub fn validate(&self) -> Result<(), TrackError> {
        if !(self.min_confidence.is_finite() && (0.0..=1.0).contains(&self.min_confidence))
            || self.max_interpolated_gap > 1_000
            || !(self.max_center_step.is_finite() && self.max_center_step > 0.0)
            || !(self.max_center_jump.is_finite() && self.max_center_jump > 0.0)
            || !(self.max_area_ratio.is_finite() && self.max_area_ratio >= 1.0)
        {
            return Err(TrackError::Invalid("tracking policy is outside its bounds"));
        }
        Ok(())
    }

    /// Whether `next`, `elapsed` pictures after `last`, is the same subject,
    /// for a picture displayed `aspect` (width / height) wide.
    pub fn plausible(
        &self,
        last: &NormalizedRect,
        next: &NormalizedRect,
        elapsed: usize,
        aspect: f64,
    ) -> bool {
        let (ax, ay) = last.center();
        let (bx, by) = next.center();
        // In fractions of the longer displayed side.
        let distance = ((bx - ax) * aspect.min(1.0)).hypot((by - ay) * aspect.recip().min(1.0));
        let allowed = (self.max_center_step * elapsed as f64).min(self.max_center_jump);
        let ratio = next.area() / last.area();
        distance <= allowed && ratio <= self.max_area_ratio && ratio * self.max_area_ratio >= 1.0
    }

    fn accepts(&self, observation: &RawObservation) -> Option<NormalizedRect> {
        let confident =
            observation.confidence.is_finite() && observation.confidence >= self.min_confidence;
        observation.region.filter(|_| confident)
    }
}

/// Why a path ends where it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrackStop {
    /// The requested range ended.
    RangeEnd,
    /// A shot begins at this picture ordinal of the Original's index.
    ShotBoundary { picture: usize },
    /// [`MAX_TRACK_PICTURES`] were reached.
    PictureLimit,
}

/// The exclusive end ordinal of a tracking range starting at `start`: the
/// requested end, the first shot boundary after `start` when `stop_at_shots`,
/// or the picture limit, whichever comes first.
pub fn tracking_end(
    pictures: usize,
    start: usize,
    requested_end: usize,
    boundaries: &[usize],
    stop_at_shots: bool,
) -> Result<(usize, TrackStop), TrackError> {
    if start >= requested_end || requested_end > pictures {
        return Err(TrackError::Invalid(
            "tracking range is empty or beyond the pictures",
        ));
    }
    let mut end = (requested_end, TrackStop::RangeEnd);
    if stop_at_shots
        && let Some(&boundary) = boundaries
            .iter()
            .find(|&&boundary| boundary > start && boundary < end.0)
    {
        end = (boundary, TrackStop::ShotBoundary { picture: boundary });
    }
    if end.0 - start > MAX_TRACK_PICTURES {
        end = (start + MAX_TRACK_PICTURES, TrackStop::PictureLimit);
    }
    Ok(end)
}

/// The invalidated half-open PTS range of a correction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackRange {
    pub start_pts: i64,
    pub end_pts: i64,
}

/// A validated tracking path over `[start_pts, end_pts)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "PathWire", into = "PathWire")]
pub struct TrackedPath {
    policy: TrackPolicy,
    display_aspect: f64,
    start_pts: i64,
    end_pts: i64,
    stop: TrackStop,
    keyframes: Vec<Keyframe>,
    samples: Vec<TrackSample>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathWire {
    rule: String,
    policy: TrackPolicy,
    display_aspect: f64,
    start_pts: i64,
    end_pts: i64,
    stop: TrackStop,
    keyframes: Vec<Keyframe>,
    samples: Vec<TrackSample>,
}

impl TryFrom<PathWire> for TrackedPath {
    type Error = TrackError;

    fn try_from(value: PathWire) -> Result<Self, Self::Error> {
        if value.rule != TRACK_RULE {
            return Err(TrackError::Invalid("unsupported tracking rule"));
        }
        let path = Self {
            policy: value.policy,
            display_aspect: value.display_aspect,
            start_pts: value.start_pts,
            end_pts: value.end_pts,
            stop: value.stop,
            keyframes: value.keyframes,
            samples: value.samples,
        };
        path.validate()?;
        Ok(path)
    }
}

impl From<TrackedPath> for PathWire {
    fn from(value: TrackedPath) -> Self {
        Self {
            rule: TRACK_RULE.into(),
            policy: value.policy,
            display_aspect: value.display_aspect,
            start_pts: value.start_pts,
            end_pts: value.end_pts,
            stop: value.stop,
            keyframes: value.keyframes,
            samples: value.samples,
        }
    }
}

impl TrackedPath {
    /// Apply the policy to observations tracked from `seed`. `pictures` lists
    /// every indexed picture PTS in `[seed.pts, end_pts)` in order; it gives
    /// observations their ordinal distance. Observations need not cover every
    /// picture (a stride analyses a subset). `display_aspect` is the displayed
    /// picture's width over height, after rotation and sample aspect ratio.
    #[allow(clippy::too_many_arguments)]
    pub fn track(
        policy: TrackPolicy,
        display_aspect: f64,
        pictures: &[i64],
        end_pts: i64,
        stop: TrackStop,
        seed: Keyframe,
        observations: &[RawObservation],
    ) -> Result<Self, TrackError> {
        policy.validate()?;
        check_aspect(display_aspect)?;
        let samples = segment(
            &policy,
            display_aspect,
            pictures,
            end_pts,
            seed,
            observations,
        )?;
        let path = Self {
            policy,
            display_aspect,
            start_pts: seed.pts,
            end_pts,
            stop,
            keyframes: vec![seed],
            samples,
        };
        path.validate()?;
        Ok(path)
    }

    pub fn policy(&self) -> &TrackPolicy {
        &self.policy
    }
    pub fn display_aspect(&self) -> f64 {
        self.display_aspect
    }
    pub fn start_pts(&self) -> i64 {
        self.start_pts
    }
    pub fn end_pts(&self) -> i64 {
        self.end_pts
    }
    pub fn stop(&self) -> TrackStop {
        self.stop
    }
    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }
    pub fn samples(&self) -> &[TrackSample] {
        &self.samples
    }

    /// The range a keyframe at `pts` governs: to the next keyframe or the end.
    pub fn governed_range(&self, pts: i64) -> Result<TrackRange, TrackError> {
        if pts < self.start_pts || pts >= self.end_pts {
            return Err(TrackError::Invalid("keyframe lies outside the path"));
        }
        let end_pts = self
            .keyframes
            .iter()
            .map(|keyframe| keyframe.pts)
            .find(|&other| other > pts)
            .unwrap_or(self.end_pts);
        Ok(TrackRange {
            start_pts: pts,
            end_pts,
        })
    }

    /// Insert or replace a manual keyframe. Only the returned range changes:
    /// its samples hold the corrected position until [`Self::retrack`]
    /// replaces them. Samples outside it are untouched. All or nothing: on
    /// error the path is unchanged.
    pub fn correct(&mut self, keyframe: Keyframe) -> Result<TrackRange, TrackError> {
        let mut corrected = self.clone();
        let range = corrected.apply_correction(keyframe)?;
        corrected.validate()?;
        *self = corrected;
        Ok(range)
    }

    /// Insert `keyframe` and re-track its range from `observations` in one
    /// step: on any error the path is unchanged. `pictures` lists every
    /// indexed PTS of the governed range.
    pub fn correct_and_retrack(
        &mut self,
        keyframe: Keyframe,
        pictures: &[i64],
        observations: &[RawObservation],
    ) -> Result<TrackRange, TrackError> {
        let mut corrected = self.clone();
        corrected.apply_correction(keyframe)?;
        let range = corrected.retrack(keyframe.pts, pictures, observations)?;
        *self = corrected;
        Ok(range)
    }

    fn apply_correction(&mut self, keyframe: Keyframe) -> Result<TrackRange, TrackError> {
        let range = self.governed_range(keyframe.pts)?;
        let position = self
            .keyframes
            .partition_point(|existing| existing.pts < keyframe.pts);
        if self
            .keyframes
            .get(position)
            .is_some_and(|existing| existing.pts == keyframe.pts)
        {
            self.keyframes[position] = keyframe;
        } else {
            if self.keyframes.len() >= MAX_TRACK_KEYFRAMES {
                return Err(TrackError::Limit("keyframe"));
            }
            self.keyframes.insert(position, keyframe);
        }
        let first = self.samples.partition_point(|s| s.pts < range.start_pts);
        let last = self.samples.partition_point(|s| s.pts < range.end_pts);
        let mut replacement = vec![TrackSample {
            pts: keyframe.pts,
            region: keyframe.region,
            confidence: None,
            state: TrackState::Manual,
        }];
        replacement.extend(
            self.samples[first..last]
                .iter()
                .filter(|sample| sample.pts > keyframe.pts)
                .map(|sample| TrackSample {
                    pts: sample.pts,
                    region: keyframe.region,
                    confidence: None,
                    state: TrackState::Held,
                }),
        );
        self.samples.splice(first..last, replacement);
        Ok(range)
    }

    /// Replace the range governed by the keyframe at `keyframe_pts` with
    /// observations tracked from it. `pictures` lists every indexed PTS of
    /// that range. Samples outside the range are untouched.
    pub fn retrack(
        &mut self,
        keyframe_pts: i64,
        pictures: &[i64],
        observations: &[RawObservation],
    ) -> Result<TrackRange, TrackError> {
        let keyframe = *self
            .keyframes
            .iter()
            .find(|keyframe| keyframe.pts == keyframe_pts)
            .ok_or(TrackError::Invalid("re-tracking must start at a keyframe"))?;
        let range = self.governed_range(keyframe_pts)?;
        let replacement = segment(
            &self.policy,
            self.display_aspect,
            pictures,
            range.end_pts,
            keyframe,
            observations,
        )?;
        let first = self.samples.partition_point(|s| s.pts < range.start_pts);
        let last = self.samples.partition_point(|s| s.pts < range.end_pts);
        let previous = self.samples.clone();
        self.samples.splice(first..last, replacement);
        if let Err(error) = self.validate() {
            self.samples = previous;
            return Err(error);
        }
        Ok(range)
    }

    pub fn validate(&self) -> Result<(), TrackError> {
        self.policy.validate()?;
        check_aspect(self.display_aspect)?;
        if self.start_pts >= self.end_pts {
            return Err(TrackError::Invalid("tracking range is empty"));
        }
        if self.samples.len() > MAX_TRACK_PICTURES {
            return Err(TrackError::Limit("sample"));
        }
        if self.keyframes.is_empty() || self.keyframes.len() > MAX_TRACK_KEYFRAMES {
            return Err(TrackError::Limit("keyframe"));
        }
        if self.keyframes[0].pts != self.start_pts
            || self
                .keyframes
                .windows(2)
                .any(|pair| pair[0].pts >= pair[1].pts)
            || self.keyframes.last().is_some_and(|k| k.pts >= self.end_pts)
        {
            return Err(TrackError::Invalid(
                "keyframes must be ordered inside the path and begin it",
            ));
        }
        if self.samples.first().map(|sample| sample.pts) != Some(self.start_pts)
            || self
                .samples
                .windows(2)
                .any(|pair| pair[0].pts >= pair[1].pts)
            || self.samples.last().is_some_and(|s| s.pts >= self.end_pts)
        {
            return Err(TrackError::Invalid(
                "samples must be ordered inside the path and begin at its start",
            ));
        }
        let mut keyframes = self.keyframes.iter().peekable();
        for sample in &self.samples {
            if sample
                .confidence
                .is_some_and(|value| !(value.is_finite() && (0.0..=1.0).contains(&value)))
            {
                return Err(TrackError::Invalid("confidence lies outside 0..=1"));
            }
            let is_keyframe = keyframes.peek().is_some_and(|k| k.pts == sample.pts);
            match (sample.state, is_keyframe) {
                (TrackState::Manual, true) => {
                    let keyframe = keyframes.next().expect("peeked");
                    if keyframe.region != sample.region || sample.confidence.is_some() {
                        return Err(TrackError::Invalid(
                            "a manual sample differs from its keyframe",
                        ));
                    }
                }
                (TrackState::Manual, false) | (_, true) => {
                    return Err(TrackError::Invalid(
                        "manual samples and keyframes must correspond",
                    ));
                }
                _ => {}
            }
        }
        if keyframes.next().is_some() {
            return Err(TrackError::Invalid("a keyframe has no sample"));
        }
        Ok(())
    }
}

fn check_aspect(aspect: f64) -> Result<(), TrackError> {
    if aspect.is_finite() && (1.0 / 64.0..=64.0).contains(&aspect) {
        Ok(())
    } else {
        Err(TrackError::Invalid("display aspect lies outside 1/64..=64"))
    }
}

/// Apply the policy to one keyframe's segment.
fn segment(
    policy: &TrackPolicy,
    aspect: f64,
    pictures: &[i64],
    end_pts: i64,
    seed: Keyframe,
    observations: &[RawObservation],
) -> Result<Vec<TrackSample>, TrackError> {
    if pictures.len() > MAX_TRACK_PICTURES || observations.len() > MAX_TRACK_PICTURES {
        return Err(TrackError::Limit("picture"));
    }
    if pictures.first() != Some(&seed.pts)
        || pictures.windows(2).any(|pair| pair[0] >= pair[1])
        || pictures.last().is_some_and(|&pts| pts >= end_pts)
    {
        return Err(TrackError::Invalid(
            "pictures must be ordered, begin at the keyframe and end before the range",
        ));
    }
    let mut ordinals = Vec::with_capacity(observations.len());
    let mut previous = None;
    for observation in observations {
        if previous.is_some_and(|pts| pts >= observation.pts) {
            return Err(TrackError::Invalid("observations must be strictly ordered"));
        }
        previous = Some(observation.pts);
        let ordinal = pictures
            .binary_search(&observation.pts)
            .map_err(|_| TrackError::Invalid("an observation is not at a range picture"))?;
        ordinals.push(ordinal);
    }

    let mut samples = Vec::with_capacity(observations.len() + 1);
    samples.push(TrackSample {
        pts: seed.pts,
        region: seed.region,
        confidence: None,
        state: TrackState::Manual,
    });
    // The last confident region, its ordinal and its confidence.
    let mut anchor = (seed.region, 0_usize, 1.0_f32);
    // Rejected observations awaiting a confident neighbour.
    let mut pending: Vec<(usize, &RawObservation)> = Vec::new();
    let mut lost = false;
    let held = |observation: &RawObservation, region: NormalizedRect, state| TrackSample {
        pts: observation.pts,
        region,
        confidence: None,
        state,
    };
    let gap_limit = policy.max_interpolated_gap as usize;
    for (observation, &ordinal) in observations.iter().zip(&ordinals) {
        if ordinal == 0 {
            continue;
        }
        let accepted = policy
            .accepts(observation)
            .filter(|region| policy.plausible(&anchor.0, region, ordinal - anchor.1, aspect));
        if lost {
            let state = if accepted.is_some() {
                TrackState::Held
            } else {
                TrackState::Lost
            };
            samples.push(held(observation, anchor.0, state));
            continue;
        }
        if accepted.is_some() && !pending.is_empty() && ordinal - anchor.1 - 1 > gap_limit {
            // The gap this observation would close is too long in pictures.
            lost = true;
            for (_, gap) in pending.drain(..) {
                samples.push(held(gap, anchor.0, TrackState::Lost));
            }
            samples.push(held(observation, anchor.0, TrackState::Held));
            continue;
        }
        match accepted {
            Some(region) => {
                let span = (ordinal - anchor.1) as f64;
                let confidence = anchor.2.min(observation.confidence);
                for (gap_ordinal, gap) in pending.drain(..) {
                    let t = (gap_ordinal - anchor.1) as f64 / span;
                    samples.push(TrackSample {
                        pts: gap.pts,
                        region: anchor.0.lerp(&region, t),
                        confidence: Some(confidence),
                        state: TrackState::Interpolated,
                    });
                }
                samples.push(TrackSample {
                    pts: observation.pts,
                    region,
                    confidence: Some(observation.confidence),
                    state: TrackState::Tracked,
                });
                anchor = (region, ordinal, observation.confidence);
            }
            None => {
                pending.push((ordinal, observation));
                // Any closing neighbour would be further than the bound in
                // pictures, whatever the stride.
                if ordinal - anchor.1 > gap_limit {
                    lost = true;
                    for (_, gap) in pending.drain(..) {
                        samples.push(held(gap, anchor.0, TrackState::Lost));
                    }
                }
            }
        }
    }
    // A trailing gap has no confident neighbour to interpolate towards.
    for (_, gap) in pending.drain(..) {
        samples.push(held(gap, anchor.0, TrackState::Lost));
    }
    for sample in &mut samples {
        // Confidence outside 0..=1 is not a tracker report this rule admits.
        if sample.confidence.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
            return Err(TrackError::Invalid("confidence lies outside 0..=1"));
        }
    }
    Ok(samples)
}

mod target;
pub use target::*;

#[cfg(test)]
mod tests;
