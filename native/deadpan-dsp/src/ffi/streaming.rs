//! No input or output pointer survives a streaming read.
use super::status;
use crate::{DspError, StreamRecipe};
use std::ffi::c_void;
use std::ops::Range;
use std::ptr::NonNull;

unsafe extern "C" {
    fn dp_dsp_stream_create(
        input: u32,
        output: u32,
        numerator: u64,
        denominator: u64,
        pitch: i32,
        handle: *mut *mut c_void,
    ) -> i32;
    fn dp_dsp_stream_required(
        handle: *mut c_void,
        requested: u32,
        start: *mut u32,
        frames: *mut u32,
    ) -> i32;
    fn dp_dsp_stream_read(
        handle: *mut c_void,
        input_start: u32,
        input_left: *const f32,
        input_right: *const f32,
        input_frames: u32,
        left: *mut f32,
        right: *mut f32,
        requested: u32,
        written: *mut u32,
    ) -> i32;
    fn dp_dsp_stream_destroy(handle: *mut c_void);
}

pub(crate) struct StreamEngine {
    handle: NonNull<c_void>,
}

impl StreamEngine {
    pub(crate) fn new(recipe: StreamRecipe) -> Result<Self, DspError> {
        let mut handle = std::ptr::null_mut();
        // SAFETY: validated scalar recipe, live report pointer. Native catches
        // exceptions and returns only a fully constructed uniquely owned handle.
        status(unsafe {
            dp_dsp_stream_create(
                recipe.input_frames(),
                recipe.output_frames(),
                recipe.rate().numerator(),
                recipe.rate().denominator(),
                recipe.pitch_semitones(),
                &mut handle,
            )
        })?;
        Ok(Self {
            handle: NonNull::new(handle).ok_or(DspError::NativeReport)?,
        })
    }
    pub(crate) fn required_input(&self, requested: usize) -> Result<Range<u32>, DspError> {
        let requested = u32::try_from(requested).map_err(|_| DspError::OutputLength)?;
        let (mut start, mut frames) = (0, 0);
        // SAFETY: live thread-local handle and distinct initialized reports;
        // native computes bounded scalar metadata only, without advancing DSP.
        status(unsafe {
            dp_dsp_stream_required(self.handle.as_ptr(), requested, &mut start, &mut frames)
        })?;
        Ok(start..start.checked_add(frames).ok_or(DspError::NativeReport)?)
    }
    pub(crate) fn read(
        &mut self,
        input_start: u32,
        input: [&[f32]; 2],
        output: [&mut [f32]; 2],
    ) -> Result<usize, DspError> {
        let requested = u32::try_from(output[0].len()).map_err(|_| DspError::OutputLength)?;
        let frames = u32::try_from(input[0].len()).map_err(|_| DspError::InputLength)?;
        let mut written = 0;
        let [left, right] = output;
        // SAFETY: the safe wrapper checked equal lengths and the exact bounded
        // input range. Rust borrows keep inputs live and outputs disjoint. Native
        // checks ranges before array access, stages output until success, and
        // clears every borrowed PCM pointer before returning, including errors.
        status(unsafe {
            dp_dsp_stream_read(
                self.handle.as_ptr(),
                input_start,
                input[0].as_ptr(),
                input[1].as_ptr(),
                frames,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                requested,
                &mut written,
            )
        })?;
        if written > requested {
            return Err(DspError::NativeReport);
        }
        Ok(written as usize)
    }
}
impl Drop for StreamEngine {
    fn drop(&mut self) {
        // SAFETY: this unique handle was created successfully and is destroyed
        // once, on its owning thread. Native retains no caller PCM pointers.
        unsafe { dp_dsp_stream_destroy(self.handle.as_ptr()) };
    }
}
