//! Synchronous owned FIR plan. Only these calls cross the unsafe boundary.

use std::{ffi::c_void, ptr::NonNull};

use crate::{
    FINITE_FILTER_INPUT_FRAMES, FINITE_FILTER_OUTPUT_FRAMES, FINITE_FILTER_TAPS, FilterError,
};

unsafe extern "C" {
    fn dp_dsp_filter_create(
        coefficients: *const f64,
        coefficient_count: usize,
        output: *mut *mut c_void,
    ) -> i32;
    fn dp_dsp_filter_read(
        filter: *mut c_void,
        left: *const f64,
        right: *const f64,
        input_frames: u32,
        output_left: *mut f64,
        output_right: *mut f64,
        output_frames: u32,
    ) -> i32;
    fn dp_dsp_filter_destroy(filter: *mut c_void);
}

pub(crate) struct FilterEngine {
    handle: NonNull<c_void>,
}

impl FilterEngine {
    pub(crate) fn new(coefficients: &[f64; FINITE_FILTER_TAPS]) -> Result<Self, FilterError> {
        let mut handle = std::ptr::null_mut();
        // SAFETY: the fixed array holds 4097 initialized, immutable doubles
        // throughout the call. C++ copies it, catches exceptions and publishes
        // only a fully constructed plan. No Rust pointer is retained.
        let result =
            unsafe { dp_dsp_filter_create(coefficients.as_ptr(), coefficients.len(), &mut handle) };
        status(result)?;
        Ok(Self {
            handle: NonNull::new(handle).ok_or(FilterError::NativeReport)?,
        })
    }

    pub(crate) fn read(
        &mut self,
        left: &[f64; FINITE_FILTER_INPUT_FRAMES],
        right: &[f64; FINITE_FILTER_INPUT_FRAMES],
        output_left: &mut [f64; FINITE_FILTER_OUTPUT_FRAMES],
        output_right: &mut [f64; FINITE_FILTER_OUTPUT_FRAMES],
    ) -> Result<(), FilterError> {
        let input_frames = u32::try_from(left.len()).map_err(|_| FilterError::InputLength)?;
        let output_frames =
            u32::try_from(output_left.len()).map_err(|_| FilterError::OutputLength)?;
        // SAFETY: fixed array types give both input lengths as 5120 and
        // both output lengths as 1024. Rust's exclusive output borrows prevent
        // overlap with inputs or each other. C++ synchronously borrows input,
        // stages both outputs before publishing, catches all exceptions and
        // retains no array pointer. The unique handle stays on its worker.
        let result = unsafe {
            dp_dsp_filter_read(
                self.handle.as_ptr(),
                left.as_ptr(),
                right.as_ptr(),
                input_frames,
                output_left.as_mut_ptr(),
                output_right.as_mut_ptr(),
                output_frames,
            )
        };
        status(result)
    }
}

impl Drop for FilterEngine {
    fn drop(&mut self) {
        // SAFETY: one successful create owns this non-null handle, dropped
        // exactly once. Destruction retains no Rust pointer and cannot throw.
        unsafe { dp_dsp_filter_destroy(self.handle.as_ptr()) };
    }
}

fn status(value: i32) -> Result<(), FilterError> {
    match value {
        0 => Ok(()),
        1 => Err(FilterError::NativeRejected),
        2 => Err(FilterError::NativeAllocation),
        3 => Err(FilterError::NativeFailure),
        4 => Err(FilterError::NonFiniteOutput),
        5 => Err(FilterError::Poisoned),
        _ => Err(FilterError::NativeReport),
    }
}
