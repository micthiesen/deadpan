//! Bounded, worker-side time/pitch preparation using one canonical DSP schedule.
//!
//! This adapter owns already prepared planar 48 kHz stereo PCM. It does not
//! decode, resample, downmix, open devices, allocate timeline time, or implement
//! a cache. Both preview and export must consume the same recipe and schedule.
//! Configuration, reads, and replay belong on a preparation worker, never an
//! audio callback. The renderer is intentionally neither `Send` nor `Sync`:
//! move the owned PCM to a worker, then construct the renderer there.

use std::sync::atomic::{AtomicBool, Ordering};

use thiserror::Error;

#[allow(unsafe_code)]
mod ffi;

/// Identity of the fixed schedule and its pinned native implementation.
///
/// A prepared-audio cache must also include the complete recipe, source content
/// identity, selection, and source interpretation. This identifier alone is not
/// a media identity or a promise of bit equality across unqualified compilers.
pub const ENGINE_ID: &str = "deadpan-canonical-stretch-1";
pub const STRETCH_REVISION: &str = "57b93f4e9206a089a45387eaa39bdc9f310d3308";
pub const LINEAR_REVISION: &str = "5668673560146a9cfe38c25315071e3fd68c8317";
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u32 = 2;
pub const QUANTUM: usize = 256;
pub const MAX_INPUT_FRAMES: u32 = 1_048_576;
pub const MAX_OUTPUT_FRAMES: u32 = MAX_INPUT_FRAMES * 8;
/// Admission ceiling, not a limiter or a normalization target.
pub const MAX_INPUT_PEAK: f32 = 16.0;

/// A validated constant-rate, constant-pitch preparation recipe.
///
/// The rate is exactly `input_frames / output_frames`. The dense 120/15 ms
/// analysis window, seed 1337, portable FFT, and zero extension/cropping are
/// fixed by [`ENGINE_ID`]. Positive lengths are required; empty timeline spans
/// must be handled without constructing a stretcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CanonicalRecipe {
    input_frames: u32,
    output_frames: u32,
    pitch_semitones: i32,
}

impl CanonicalRecipe {
    pub fn new(
        input_frames: u32,
        output_frames: u32,
        pitch_semitones: i32,
    ) -> Result<Self, DspError> {
        if input_frames == 0 || input_frames > MAX_INPUT_FRAMES {
            return Err(DspError::InputLength);
        }
        if output_frames == 0
            || u64::from(output_frames) > u64::from(input_frames) * 8
            || u64::from(input_frames) > u64::from(output_frames) * 8
        {
            return Err(DspError::Rate);
        }
        if !(-24..=24).contains(&pitch_semitones) {
            return Err(DspError::Pitch);
        }
        Ok(Self {
            input_frames,
            output_frames,
            pitch_semitones,
        })
    }

    pub fn input_frames(self) -> u32 {
        self.input_frames
    }

    pub fn output_frames(self) -> u32 {
        self.output_frames
    }

    pub fn pitch_semitones(self) -> i32 {
        self.pitch_semitones
    }

    pub fn engine_id(self) -> &'static str {
        ENGINE_ID
    }
}

/// Owned immutable planar samples, admitted without changing their levels.
#[derive(Debug)]
pub struct StereoPcm {
    left: Box<[f32]>,
    right: Box<[f32]>,
    frames: u32,
}

impl StereoPcm {
    pub fn new(left: Vec<f32>, right: Vec<f32>) -> Result<Self, DspError> {
        let frames = u32::try_from(left.len()).map_err(|_| DspError::InputLength)?;
        if frames == 0 || frames > MAX_INPUT_FRAMES || left.len() != right.len() {
            return Err(DspError::InputLength);
        }
        if left.iter().chain(&right).any(|sample| !sample.is_finite()) {
            return Err(DspError::NonFiniteInput);
        }
        if left
            .iter()
            .chain(&right)
            .any(|sample| sample.abs() > MAX_INPUT_PEAK)
        {
            return Err(DspError::InputPeak);
        }
        Ok(Self {
            left: left.into_boxed_slice(),
            right: right.into_boxed_slice(),
            frames,
        })
    }

    pub fn frames(&self) -> u32 {
        self.frames
    }
}

/// One thread-local canonical renderer and its complete immutable input.
pub struct CanonicalStretch {
    // Rust drops fields in declaration order. Destroy the native renderer
    // before releasing the input arrays it borrows for its entire lifetime.
    native: ffi::Engine,
    _source: StereoPcm,
    recipe: CanonicalRecipe,
    position: u32,
}

impl CanonicalStretch {
    pub fn new(recipe: CanonicalRecipe, source: StereoPcm) -> Result<Self, DspError> {
        if source.frames != recipe.input_frames {
            return Err(DspError::InputLength);
        }
        let native = ffi::Engine::new(&source, recipe)?;
        Ok(Self {
            native,
            _source: source,
            recipe,
            position: 0,
        })
    }

    pub fn recipe(&self) -> CanonicalRecipe {
        self.recipe
    }

    pub fn position(&self) -> u32 {
        self.position
    }

    /// Read at most one output quantum into equal-length caller-owned slices.
    ///
    /// Returns fewer frames only at EOF, leaving the unused suffix untouched.
    /// Cancellation is checked before native work. A cancellation arriving
    /// during a call takes effect before the next call; it does not invalidate
    /// completed samples. The initial read also performs the original bounded
    /// negative-context priming schedule (at most 203 discarded quanta at the
    /// slowest admitted rate). That call cannot be interrupted mid-FFI. These
    /// are cooperative work bounds, not wall-time or realtime guarantees.
    pub fn read(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        cancelled: &AtomicBool,
    ) -> Result<usize, DspError> {
        if left.len() != right.len() || left.len() > QUANTUM {
            return Err(DspError::OutputLength);
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err(DspError::Cancelled);
        }
        let count = self.native.read(left, right)?;
        let count = u32::try_from(count).map_err(|_| DspError::NativeReport)?;
        self.position = self
            .position
            .checked_add(count)
            .filter(|position| *position <= self.recipe.output_frames)
            .ok_or(DspError::NativeReport)?;
        usize::try_from(count).map_err(|_| DspError::NativeReport)
    }

    /// Discard output from the same canonical schedule up to `target`.
    ///
    /// Work grows with the distance from the current position. Cancellation is
    /// checked between reads of at most 256 frames; already completed progress
    /// remains visible in [`Self::position`], and the renderer can resume. Seek
    /// backwards by creating a fresh renderer, or use externally prepared PCM.
    /// This is not a finite local-preroll approximation or a state checkpoint.
    pub fn replay_to(&mut self, target: u32, cancelled: &AtomicBool) -> Result<(), DspError> {
        if target < self.position || target > self.recipe.output_frames {
            return Err(DspError::ReplayRange);
        }
        let mut left = [0.0; QUANTUM];
        let mut right = [0.0; QUANTUM];
        while self.position < target {
            let count = usize::try_from(target - self.position)
                .map_err(|_| DspError::ReplayRange)?
                .min(QUANTUM);
            let read = self.read(&mut left[..count], &mut right[..count], cancelled)?;
            if read != count {
                return Err(DspError::NativeReport);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DspError {
    #[error(
        "input must have matching nonempty channels of at most 1048576 frames and match the recipe"
    )]
    InputLength,
    #[error("output length must be positive and input/output rate must be within 1/8 through 8")]
    Rate,
    #[error("integer pitch must be within -24 through 24 semitones")]
    Pitch,
    #[error("input contains a nonfinite sample")]
    NonFiniteInput,
    #[error("input peak exceeds the admitted magnitude of 16; samples were not normalized")]
    InputPeak,
    #[error("output channels must have equal lengths of at most 256 frames")]
    OutputLength,
    #[error("replay target must be between the current position and EOF")]
    ReplayRange,
    #[error("audio preparation cancelled")]
    Cancelled,
    #[error("native DSP allocation failed")]
    NativeAllocation,
    #[error("native DSP rejected validated arguments")]
    NativeArgument,
    #[error("native DSP failed")]
    NativeFailure,
    #[error("native DSP produced a nonfinite sample")]
    NonFiniteOutput,
    #[error("native DSP is unusable after an earlier failure")]
    Poisoned,
    #[error("native DSP returned an invalid report")]
    NativeReport,
}
