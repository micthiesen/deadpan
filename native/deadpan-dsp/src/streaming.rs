//! The same exact-rate schedule with call-scoped input windows. The caller
//! supplies canonical PCM; this adapter never opens media or calls Rust from C++.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{DspError, EXACT_RATE_ENGINE_ID, MAX_INPUT_PEAK, QUANTUM, StretchRate, ffi};

/// Conservative bound for Dense120_15 priming plus one output quantum at 8x.
/// Later windows contain at most 2048 source frames, independent of duration.
pub const MAX_STREAM_WINDOW_FRAMES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamRecipe {
    input_frames: u32,
    output_frames: u32,
    rate: StretchRate,
    pitch_semitones: i32,
}

impl StreamRecipe {
    /// Logical lengths do not allocate input or output PCM. Hosts must admit
    /// their total preparation work separately, including any unused input.
    pub fn new(
        input_frames: u32,
        output_frames: u32,
        rate: StretchRate,
        pitch_semitones: i32,
    ) -> Result<Self, DspError> {
        if input_frames == 0 {
            return Err(DspError::InputLength);
        }
        if output_frames == 0 {
            return Err(DspError::OutputLength);
        }
        if !(-24..=24).contains(&pitch_semitones) {
            return Err(DspError::Pitch);
        }
        Ok(Self {
            input_frames,
            output_frames,
            rate,
            pitch_semitones,
        })
    }
    pub fn input_frames(self) -> u32 {
        self.input_frames
    }
    pub fn output_frames(self) -> u32 {
        self.output_frames
    }
    pub fn rate(self) -> StretchRate {
        self.rate
    }
    pub fn pitch_semitones(self) -> i32 {
        self.pitch_semitones
    }
    pub fn engine_id(self) -> &'static str {
        EXACT_RATE_ENGINE_ID
    }
}

/// Thread-local state with no retained caller PCM. Windows must come from the
/// same immutable input throughout the renderer's lifetime. A failed/cancelled
/// supply does not advance DSP; retry it with the same canonical samples.
pub struct StreamingStretch {
    native: ffi::StreamEngine,
    recipe: StreamRecipe,
    position: u32,
}

impl StreamingStretch {
    pub fn new(recipe: StreamRecipe) -> Result<Self, DspError> {
        Ok(Self {
            native: ffi::StreamEngine::new(recipe)?,
            recipe,
            position: 0,
        })
    }
    pub fn recipe(&self) -> StreamRecipe {
        self.recipe
    }
    pub fn position(&self) -> u32 {
        self.position
    }

    /// Exact real-input range needed for the next read, without advancing DSP.
    /// Empty when buffered output suffices, at EOF, or for a zero-length read.
    pub fn required_input(&self, requested: usize) -> Result<Range<u32>, DspError> {
        if requested > QUANTUM {
            return Err(DspError::OutputLength);
        }
        let range = self.native.required_input(requested)?;
        if range.end > self.recipe.input_frames
            || range.start > range.end
            || (range.end - range.start) as usize > MAX_STREAM_WINDOW_FRAMES
        {
            return Err(DspError::NativeReport);
        }
        Ok(range)
    }

    /// Validate the exact requested window and synchronously render at most
    /// 256 frames. Rejections preserve output buffers and completed progress.
    /// Like the owned renderer, the initial call includes bounded priming;
    /// cancellation cannot interrupt an individual native call.
    pub fn read(
        &mut self,
        input_start: u32,
        input: [&[f32]; 2],
        output: [&mut [f32]; 2],
        cancelled: &AtomicBool,
    ) -> Result<usize, DspError> {
        if output[0].len() != output[1].len() || output[0].len() > QUANTUM {
            return Err(DspError::OutputLength);
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err(DspError::Cancelled);
        }
        let required = self.required_input(output[0].len())?;
        if input_start != required.start
            || input[0].len() != (required.end - required.start) as usize
            || input[0].len() != input[1].len()
        {
            return Err(DspError::InputLength);
        }
        validate_input(input)?;
        let count = self.native.read(input_start, input, output)?;
        self.position = self
            .position
            .checked_add(u32::try_from(count).map_err(|_| DspError::NativeReport)?)
            .filter(|position| *position <= self.recipe.output_frames)
            .ok_or(DspError::NativeReport)?;
        Ok(count)
    }
}

pub(crate) fn validate_input(input: [&[f32]; 2]) -> Result<(), DspError> {
    if input
        .iter()
        .flat_map(|channel| channel.iter())
        .any(|sample| !sample.is_finite())
    {
        return Err(DspError::NonFiniteInput);
    }
    if input
        .iter()
        .flat_map(|channel| channel.iter())
        .any(|sample| sample.abs() > MAX_INPUT_PEAK)
    {
        return Err(DspError::InputPeak);
    }
    Ok(())
}
