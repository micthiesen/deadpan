//! Authored clip gain after complete voice time/pitch mapping and edge fades.
//!
//! A treatment's owner supplies its evaluation clock and contribution scope.
//! These fixed owner-output coordinates are never normalized to an owner's
//! duration. Cropping or extending its allocation does not rewrite the curve.
//! Interior values use Q32 millidecibels; times and segment selection stay exact.
//! Amplitude conversion, PCM processing and aggregate voice mixing belong to DSP.

mod admission;
mod json;
mod numeric;
mod wire;

pub(crate) use admission::{
    MAX_ISOLATED_GAIN_RECORDS, invalid, node_map, validate_command, validate_document,
    validate_nodes, validate_nodes_with_limit,
};

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::ExactRatio;

pub const GAIN_NUMERIC_SCALE: u64 = 1 << 32;
pub const MIN_GAIN_MILLIDECIBELS: i32 = -96_000;
pub const MAX_GAIN_MILLIDECIBELS: i32 = 24_000;
/// Per-owner work is at most 16 curves of 64 segments and 64 mute tests.
pub const MAX_GAIN_ENVELOPES: usize = 16;
pub const MAX_GAIN_SEGMENTS: usize = 64;
pub const MAX_GAIN_MUTE_RANGES: usize = 64;
/// Document/plan integration must charge these independent aggregate limits.
pub const MAX_GAIN_LAYERS: usize = 16;
pub const MAX_GAIN_RECORDS: usize = 100_000;
pub const MAX_AUDIO_TREATMENT_STAGES: usize = 2;
/// Saturation drive is authored in exact millidecibels from 0 through 24 dB, the authored gain ceiling.
pub const MAX_SATURATION_DRIVE_MILLIDECIBELS: i32 = 24_000;
/// Standalone JSON input/output cap. Enclosing documents and commands must also
/// impose their own byte limit when they deserialize these types directly.
pub const MAX_AUDIO_TREATMENTS_JSON_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainError {
    ValueRange,
    TimeRange,
    Segments,
    StageOrder,
    Limit,
    Overflow,
}

impl fmt::Display for GainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ValueRange => "gain must be between -96000 and 24000 millidecibels",
            Self::TimeRange => "gain ranges require nonnegative increasing exact owner times",
            Self::Segments => "gain segments must increase from the range start to its exact end",
            Self::StageOrder => {
                "treatment order must list each present stage (ClipGain, Saturation) exactly once"
            }
            Self::Limit => "gain exceeds its declared collection or aggregate record limit",
            Self::Overflow => "gain arithmetic exceeds its bounded numeric representation",
        })
    }
}
impl Error for GainError {}
impl From<crate::TimeError> for GainError {
    fn from(_: crate::TimeError) -> Self {
        Self::Overflow
    }
}

/// Finite authored gain, never a mute sentinel. Zero means unity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "i32", into = "i32")]
pub struct GainDb(i32);

impl GainDb {
    pub const UNITY: Self = Self(0);

    pub fn new(millidecibels: i32) -> Result<Self, GainError> {
        if !(MIN_GAIN_MILLIDECIBELS..=MAX_GAIN_MILLIDECIBELS).contains(&millidecibels) {
            return Err(GainError::ValueRange);
        }
        Ok(Self(millidecibels))
    }

    pub const fn millidecibels(self) -> i32 {
        self.0
    }

    pub fn adjusted(self, delta_millidecibels: i32) -> Result<Self, GainError> {
        Self::new(
            self.0
                .checked_add(delta_millidecibels)
                .ok_or(GainError::Overflow)?,
        )
    }

    fn grid(self) -> i64 {
        i64::from(self.0) * GAIN_NUMERIC_SCALE as i64
    }
}
impl TryFrom<i32> for GainDb {
    type Error = GainError;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<GainDb> for i32 {
    fn from(value: GainDb) -> Self {
        value.0
    }
}

/// A closed evaluation-space contract. No implicit source clock or normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GainClock {
    OwnerOutput,
}

/// Exact owner-local project-frame coordinates, including sample-derived ratios.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "wire::Range", into = "wire::Range")]
pub struct GainRange {
    start: ExactRatio,
    end: ExactRatio,
}

impl GainRange {
    pub fn new(start: ExactRatio, end: ExactRatio) -> Result<Self, GainError> {
        if start.compare_integer(0).is_lt() || !numeric::compare(start, end).is_lt() {
            return Err(GainError::TimeRange);
        }
        Ok(Self { start, end })
    }
    pub const fn start(self) -> ExactRatio {
        self.start
    }
    pub const fn end(self) -> ExactRatio {
        self.end
    }
    pub fn contains(self, local: ExactRatio) -> bool {
        !numeric::compare(local, self.start).is_lt() && numeric::compare(local, self.end).is_lt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GainCurve {
    Step,
    Linear,
    Smoothstep,
    /// Bezier value controls in dB, with linear segment progress.
    Cubic {
        control1: GainDb,
        control2: GainDb,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "wire::Segment", into = "wire::Segment")]
pub struct GainSegment {
    end: ExactRatio,
    value: GainDb,
    curve: GainCurve,
}

impl GainSegment {
    pub fn new(end: ExactRatio, value: GainDb, curve: GainCurve) -> Result<Self, GainError> {
        if !end.compare_integer(0).is_gt() {
            return Err(GainError::TimeRange);
        }
        Ok(Self { end, value, curve })
    }
    pub const fn end(&self) -> ExactRatio {
        self.end
    }
    pub const fn value(&self) -> GainDb {
        self.value
    }
    pub const fn curve(&self) -> GainCurve {
        self.curve
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "wire::Envelope", into = "wire::Envelope")]
pub struct GainEnvelope {
    clock: GainClock,
    range: GainRange,
    initial: GainDb,
    segments: Vec<GainSegment>,
}

impl GainEnvelope {
    pub fn new(
        clock: GainClock,
        range: GainRange,
        initial: GainDb,
        segments: Vec<GainSegment>,
    ) -> Result<Self, GainError> {
        let result = Self {
            clock,
            range,
            initial,
            segments,
        };
        result.validate()?;
        Ok(result)
    }

    pub const fn clock(&self) -> GainClock {
        self.clock
    }
    pub const fn range(&self) -> GainRange {
        self.range
    }
    pub const fn initial(&self) -> GainDb {
        self.initial
    }
    pub fn segments(&self) -> &[GainSegment] {
        &self.segments
    }

    pub fn validate(&self) -> Result<(), GainError> {
        if self.segments.is_empty() || self.segments.len() > MAX_GAIN_SEGMENTS {
            return Err(GainError::Limit);
        }
        let mut previous = self.range.start;
        for segment in &self.segments {
            if !numeric::compare(previous, segment.end).is_lt()
                || numeric::compare(segment.end, self.range.end).is_gt()
            {
                return Err(GainError::Segments);
            }
            previous = segment.end;
        }
        if previous != self.range.end {
            return Err(GainError::Segments);
        }
        Ok(())
    }

    /// Initial value plus segment targets and explicit cubic controls.
    pub fn record_count(&self) -> usize {
        1 + self
            .segments
            .iter()
            .map(|segment| 1 + 2 * usize::from(matches!(segment.curve, GainCurve::Cubic { .. })))
            .sum::<usize>()
    }

    /// Half-open range; outside it the envelope contributes exactly zero dB.
    pub fn evaluate(&self, local: ExactRatio) -> Result<ExactRatio, GainError> {
        numeric::millidecibels(self.evaluate_grid(local)?)
    }

    fn evaluate_grid(&self, local: ExactRatio) -> Result<i64, GainError> {
        if !self.range.contains(local) {
            return Ok(0);
        }
        let mut start = self.range.start;
        let mut from = self.initial;
        for segment in &self.segments {
            match numeric::compare(local, segment.end) {
                std::cmp::Ordering::Equal => return Ok(segment.value.grid()),
                std::cmp::Ordering::Greater => {
                    start = segment.end;
                    from = segment.value;
                }
                std::cmp::Ordering::Less => {
                    if local == start || matches!(segment.curve, GainCurve::Step) {
                        return Ok(from.grid());
                    }
                    let t = numeric::progress(local, start, segment.end)?;
                    return numeric::interpolate(from, segment.value, segment.curve, t);
                }
            }
        }
        Err(GainError::Segments)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "wire::Clip", into = "wire::Clip")]
pub struct ClipGain {
    trim: GainDb,
    muted: bool,
    envelopes: Vec<GainEnvelope>,
    mute_ranges: Vec<GainRange>,
}

impl ClipGain {
    pub fn new(
        trim: GainDb,
        muted: bool,
        envelopes: Vec<GainEnvelope>,
        mute_ranges: Vec<GainRange>,
    ) -> Result<Self, GainError> {
        let result = Self {
            trim,
            muted,
            envelopes,
            mute_ranges,
        };
        result.validate()?;
        Ok(result)
    }
    pub const fn trim(&self) -> GainDb {
        self.trim
    }
    pub const fn muted(&self) -> bool {
        self.muted
    }
    pub fn envelopes(&self) -> &[GainEnvelope] {
        &self.envelopes
    }
    pub fn mute_ranges(&self) -> &[GainRange] {
        &self.mute_ranges
    }

    pub fn validate(&self) -> Result<(), GainError> {
        if self.envelopes.len() > MAX_GAIN_ENVELOPES
            || self.mute_ranges.len() > MAX_GAIN_MUTE_RANGES
        {
            return Err(GainError::Limit);
        }
        for envelope in &self.envelopes {
            envelope.validate()?;
        }
        Ok(())
    }

    /// Constant trim, whole-owner mute flag, curve values/controls and mute ranges.
    pub fn record_count(&self) -> usize {
        2 + self
            .envelopes
            .iter()
            .map(GainEnvelope::record_count)
            .sum::<usize>()
            + self.mute_ranges.len()
    }

    pub fn with_trim(&self, trim: GainDb) -> Self {
        Self {
            trim,
            ..self.clone()
        }
    }
    pub fn adjust_trim(&self, delta_millidecibels: i32) -> Result<Self, GainError> {
        Ok(self.with_trim(self.trim.adjusted(delta_millidecibels)?))
    }

    /// The caller establishes the owner's allocation. Whole-owner trim/mute
    /// apply throughout that allocation; only range effects use `local`.
    /// Overlapping envelopes add dB without clamping or normalization.
    pub fn evaluate(&self, local: ExactRatio) -> Result<EvaluatedGain, GainError> {
        let mut sum = self.trim.grid();
        for envelope in &self.envelopes {
            sum = sum
                .checked_add(envelope.evaluate_grid(local)?)
                .ok_or(GainError::Overflow)?;
        }
        Ok(EvaluatedGain {
            millidecibels: numeric::millidecibels(sum)?,
            muted: self.muted || self.mute_ranges.iter().any(|range| range.contains(local)),
        })
    }
}

/// The admitted treatment stages. The order is serialized, never inferred
/// from a UI widget's position (specification §10.2); a future stage needs a
/// new closed vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioTreatmentStage {
    ClipGain,
    Saturation,
}

/// Intentional nonlinear drive (specification §8.3 "Saturation"): the signal
/// is multiplied by the drive and passed through `tanh`, a memoryless soft
/// clipper whose output stays within ±1 before later stages and the master
/// limiter. Being stateless, it evaluates identically at any read boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "wire::SaturationWire", into = "wire::SaturationWire")]
pub struct Saturation {
    drive: GainDb,
}

impl Saturation {
    pub fn new(drive: GainDb) -> Result<Self, GainError> {
        if !(0..=MAX_SATURATION_DRIVE_MILLIDECIBELS).contains(&drive.millidecibels()) {
            return Err(GainError::ValueRange);
        }
        Ok(Self { drive })
    }
    pub const fn drive(self) -> GainDb {
        self.drive
    }
    /// The linear drive factor applied before the soft clipper.
    pub fn drive_factor(self) -> f64 {
        10_f64.powf(f64::from(self.drive.millidecibels()) / 20_000.0)
    }
    /// One sample through the stage.
    pub fn shape(self, sample: f64) -> f64 {
        (sample * self.drive_factor()).tanh()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "wire::Treatments", into = "wire::Treatments")]
/// A bounded recipe. For standalone JSON use `from_json`; direct serde ingress
/// assumes a byte-bounded enclosing document/command. This also bounds inherited
/// ExactRatio decimal strings before those strings are allocated and parsed.
pub struct AudioTreatments {
    order: Vec<AudioTreatmentStage>,
    clip_gain: Option<ClipGain>,
    saturation: Option<Saturation>,
}

impl AudioTreatments {
    pub fn new(
        order: Vec<AudioTreatmentStage>,
        clip_gain: Option<ClipGain>,
    ) -> Result<Self, GainError> {
        Self::with_stages(order, clip_gain, None)
    }
    /// Every stage recipe with its explicit order: each present stage is
    /// listed exactly once and no absent stage is listed.
    pub fn with_stages(
        order: Vec<AudioTreatmentStage>,
        clip_gain: Option<ClipGain>,
        saturation: Option<Saturation>,
    ) -> Result<Self, GainError> {
        let result = Self {
            order,
            clip_gain,
            saturation,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn from_clip_gain(clip_gain: ClipGain) -> Self {
        Self {
            order: vec![AudioTreatmentStage::ClipGain],
            clip_gain: Some(clip_gain),
            saturation: None,
        }
    }
    pub fn order(&self) -> &[AudioTreatmentStage] {
        &self.order
    }
    pub fn clip_gain(&self) -> Option<&ClipGain> {
        self.clip_gain.as_ref()
    }
    pub fn saturation(&self) -> Option<Saturation> {
        self.saturation
    }
    pub fn is_empty(&self) -> bool {
        self.clip_gain.is_none() && self.saturation.is_none()
    }
    /// The same recipe with `saturation` replaced or removed. A new stage is
    /// appended after the existing ones, so it follows clip gain by default
    /// (specification §10.2); removing it keeps the remaining order.
    pub fn with_saturation(&self, saturation: Option<Saturation>) -> Result<Self, GainError> {
        let mut order: Vec<_> = self
            .order
            .iter()
            .copied()
            .filter(|stage| *stage != AudioTreatmentStage::Saturation || saturation.is_some())
            .collect();
        if saturation.is_some() && !order.contains(&AudioTreatmentStage::Saturation) {
            order.push(AudioTreatmentStage::Saturation);
        }
        Self::with_stages(order, self.clip_gain.clone(), saturation)
    }
    /// The same recipe with `clip_gain` replaced, keeping the existing order
    /// and placing a new clip-gain stage first.
    pub fn with_clip_gain(&self, clip_gain: ClipGain) -> Result<Self, GainError> {
        let mut order = self.order.clone();
        if !order.contains(&AudioTreatmentStage::ClipGain) {
            order.insert(0, AudioTreatmentStage::ClipGain);
        }
        Self::with_stages(order, Some(clip_gain), self.saturation)
    }
    pub fn validate(&self) -> Result<(), GainError> {
        if self.order.len() > MAX_AUDIO_TREATMENT_STAGES {
            return Err(GainError::Limit);
        }
        let listed = |stage| self.order.iter().filter(|value| **value == stage).count();
        if listed(AudioTreatmentStage::ClipGain) != usize::from(self.clip_gain.is_some())
            || listed(AudioTreatmentStage::Saturation) != usize::from(self.saturation.is_some())
        {
            return Err(GainError::StageOrder);
        }
        if let Some(gain) = &self.clip_gain {
            gain.validate()?;
        }
        if let Some(saturation) = self.saturation {
            Saturation::new(saturation.drive)?;
        }
        Ok(())
    }
    pub fn record_count(&self) -> usize {
        self.clip_gain.as_ref().map_or(0, ClipGain::record_count)
            + usize::from(self.saturation.is_some())
    }
    /// Preserve existing effects when a physical owner gains a nonnegative
    /// whole-frame prefix: old material and every authored key move together.
    /// New effects can then use the current owner coordinates directly.
    ///
    /// Cropping retains the complete owner behind a Partition and does not call
    /// this helper. Tail growth also leaves these absolute keys unchanged.
    /// This pure transform changes no sampling clock, gain value, or wire type;
    /// callers must atomically update media placement and retained bindings.
    pub fn with_owner_prefix(&self, prefix: crate::FrameDuration) -> Result<Self, GainError> {
        self.validate()?;
        let mut result = self.clone();
        let shift = ExactRatio::integer(prefix.frames());
        if let Some(gain) = &mut result.clip_gain {
            for envelope in &mut gain.envelopes {
                envelope.range = GainRange::new(
                    envelope.range.start.checked_add(shift)?,
                    envelope.range.end.checked_add(shift)?,
                )?;
                for segment in &mut envelope.segments {
                    segment.end = segment.end.checked_add(shift)?;
                }
            }
            for range in &mut gain.mute_ranges {
                *range = GainRange::new(
                    range.start.checked_add(shift)?,
                    range.end.checked_add(shift)?,
                )?;
            }
        }
        result.validate()?;
        Ok(result)
    }

    pub fn evaluate(&self, local: ExactRatio) -> Result<EvaluatedGain, GainError> {
        self.clip_gain
            .as_ref()
            .map_or(Ok(EvaluatedGain::UNITY), |gain| gain.evaluate(local))
    }
}

/// Evaluated finite dB and exact mute remain separate. Values may exceed one
/// authored factor's bounds because overlapping factors add without limiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EvaluatedGain {
    pub millidecibels: ExactRatio,
    pub muted: bool,
}

impl EvaluatedGain {
    pub const UNITY: Self = Self {
        millidecibels: ExactRatio::ZERO,
        muted: false,
    };
    pub fn decibels(self) -> Result<ExactRatio, GainError> {
        Ok(self.millidecibels.checked_div(ExactRatio::integer(1000))?)
    }
}

/// Charge a document, subtree or patch-side before cloning its gain records.
/// Structural path depth is a separate plan/document responsibility.
pub fn validate_audio_treatments<'a>(
    treatments: impl IntoIterator<Item = &'a AudioTreatments>,
) -> Result<usize, GainError> {
    validate_audio_treatments_with_limit(treatments, MAX_GAIN_RECORDS)
}

fn validate_audio_treatments_with_limit<'a>(
    treatments: impl IntoIterator<Item = &'a AudioTreatments>,
    limit: usize,
) -> Result<usize, GainError> {
    let mut records = 0usize;
    for treatment in treatments {
        treatment.validate()?;
        records = records
            .checked_add(treatment.record_count())
            .ok_or(GainError::Limit)?;
        if records > limit {
            return Err(GainError::Limit);
        }
    }
    Ok(records)
}

/// Charge one structural provider-to-root path independently of the document's
/// aggregate record inventory. Explicit unity recipes still own a layer.
pub fn validate_audio_treatment_layers<'a>(
    treatments: impl IntoIterator<Item = &'a AudioTreatments>,
) -> Result<usize, GainError> {
    let mut layers = 0usize;
    for treatment in treatments {
        treatment.validate()?;
        layers += usize::from(!treatment.is_empty());
        if layers > MAX_GAIN_LAYERS {
            return Err(GainError::Limit);
        }
    }
    Ok(layers)
}
