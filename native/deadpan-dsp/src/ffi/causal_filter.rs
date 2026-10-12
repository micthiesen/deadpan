//! Only this owned, synchronous boundary permits unsafe causal-filter calls.

use crate::{CAUSAL_FILTER_OUTPUT_FRAMES, CausalFilterError};
use std::{ffi::c_void, ptr::NonNull};

unsafe extern "C" {
    fn dp_dsp_causal_create(
        left: *const f64,
        right: *const f64,
        taps: usize,
        output: *mut *mut c_void,
    ) -> i32;
    fn dp_dsp_causal_read(
        handle: *mut c_void,
        left: *const f64,
        right: *const f64,
        input_frames: u32,
        output_left: *mut f64,
        output_right: *mut f64,
        output_frames: u32,
    ) -> i32;
    fn dp_dsp_causal_destroy(handle: *mut c_void);
}

pub(crate) struct CausalFilterEngine {
    handle: NonNull<c_void>,
}
impl CausalFilterEngine {
    pub(crate) fn new(left: &[f64], right: &[f64]) -> Result<Self, CausalFilterError> {
        if left.len() != right.len() {
            return Err(CausalFilterError::KernelLength);
        }
        let mut handle = std::ptr::null_mut();
        // SAFETY: the safe layer checks equal bounded lengths and initialized
        // finite doubles. C++ copies both kernels, catches exceptions and only
        // publishes a complete plan. No Rust array pointer survives this call.
        status(unsafe {
            dp_dsp_causal_create(left.as_ptr(), right.as_ptr(), left.len(), &mut handle)
        })?;
        Ok(Self {
            handle: NonNull::new(handle).ok_or(CausalFilterError::NativeReport)?,
        })
    }
    pub(crate) fn read(
        &mut self,
        left: &[f64],
        right: &[f64],
        output_left: &mut [f64; CAUSAL_FILTER_OUTPUT_FRAMES],
        output_right: &mut [f64; CAUSAL_FILTER_OUTPUT_FRAMES],
    ) -> Result<(), CausalFilterError> {
        if left.len() != right.len() {
            return Err(CausalFilterError::InputLength);
        }
        let input_frames = u32::try_from(left.len()).map_err(|_| CausalFilterError::InputLength)?;
        let output_frames =
            u32::try_from(output_left.len()).map_err(|_| CausalFilterError::OutputLength)?;
        // SAFETY: the safe caller admits both complete history/core arrays;
        // native code also validates their length against its immutable plan.
        // Exclusive output borrows prevent overlap with inputs and each other.
        // C++ retains no pointer, stages both outputs and catches exceptions.
        status(unsafe {
            dp_dsp_causal_read(
                self.handle.as_ptr(),
                left.as_ptr(),
                right.as_ptr(),
                input_frames,
                output_left.as_mut_ptr(),
                output_right.as_mut_ptr(),
                output_frames,
            )
        })
    }
}
impl Drop for CausalFilterEngine {
    fn drop(&mut self) {
        // SAFETY: uniquely owned successful create; destruction cannot throw.
        unsafe { dp_dsp_causal_destroy(self.handle.as_ptr()) };
    }
}
fn status(value: i32) -> Result<(), CausalFilterError> {
    match value {
        0 => Ok(()),
        1 => Err(CausalFilterError::NativeRejected),
        2 => Err(CausalFilterError::NativeAllocation),
        3 => Err(CausalFilterError::NativeFailure),
        4 => Err(CausalFilterError::NonFiniteOutput),
        5 => Err(CausalFilterError::Poisoned),
        _ => Err(CausalFilterError::NativeReport),
    }
}
