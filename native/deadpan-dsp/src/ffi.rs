//! The only unsafe boundary. Native code borrows immutable boxed input and
//! synchronously writes bounded output. No Rust callback crosses the C ABI.

use std::ffi::c_void;
use std::ptr::NonNull;

use crate::{CanonicalRecipe, DspError, Schedule, StereoPcm};

unsafe extern "C" {
    fn dp_dsp_create(
        left: *const f32,
        right: *const f32,
        input_frames: u32,
        output_frames: u32,
        pitch: i32,
        output: *mut *mut c_void,
    ) -> i32;
    fn dp_dsp_create_exact_rate(
        left: *const f32,
        right: *const f32,
        input_frames: u32,
        output_frames: u32,
        rate_numerator: u64,
        rate_denominator: u64,
        pitch: i32,
        output: *mut *mut c_void,
    ) -> i32;
    fn dp_dsp_read(
        engine: *mut c_void,
        left: *mut f32,
        right: *mut f32,
        requested: u32,
        written: *mut u32,
    ) -> i32;
    fn dp_dsp_destroy(engine: *mut c_void);
}

pub(super) struct Engine {
    handle: NonNull<c_void>,
}

impl Engine {
    pub(super) fn new(source: &StereoPcm, recipe: CanonicalRecipe) -> Result<Self, DspError> {
        let mut handle = std::ptr::null_mut();
        // SAFETY: source owns two immutable, equal-length boxed arrays matching
        // the validated recipe. CanonicalStretch retains source and destroys
        // this engine first. create catches all C++ exceptions, initializes
        // handle, and publishes no partial allocation on failure. No Rust
        // callback or other borrowed pointer is retained by native code.
        let result = unsafe {
            match recipe.schedule {
                Schedule::CountRatio => dp_dsp_create(
                    source.left.as_ptr(),
                    source.right.as_ptr(),
                    recipe.input_frames,
                    recipe.output_frames,
                    recipe.pitch_semitones,
                    &mut handle,
                ),
                Schedule::ExactRate => dp_dsp_create_exact_rate(
                    source.left.as_ptr(),
                    source.right.as_ptr(),
                    recipe.input_frames,
                    recipe.output_frames,
                    recipe.rate.numerator,
                    recipe.rate.denominator,
                    recipe.pitch_semitones,
                    &mut handle,
                ),
            }
        };
        status(result)?;
        let handle = NonNull::new(handle).ok_or(DspError::NativeReport)?;
        Ok(Self { handle })
    }

    pub(super) fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<usize, DspError> {
        let requested = u32::try_from(left.len()).map_err(|_| DspError::OutputLength)?;
        let mut written = 0;
        // SAFETY: only CanonicalStretch calls this after validating matching
        // lengths <=256. Both mutable slices are disjoint and stay alive for
        // this synchronous call. The native handle has unique ownership and
        // remains thread-local. C++ retains no output/report pointer, catches
        // exceptions, and writes no more than requested finite samples.
        let result = unsafe {
            dp_dsp_read(
                self.handle.as_ptr(),
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                requested,
                &mut written,
            )
        };
        status(result)?;
        if written > requested {
            return Err(DspError::NativeReport);
        }
        usize::try_from(written).map_err(|_| DspError::NativeReport)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: handle came from one successful create, is never copied or
        // shared, and is destroyed exactly once, before its retained source.
        // destroy is noexcept and retains no pointer after returning.
        unsafe { dp_dsp_destroy(self.handle.as_ptr()) };
    }
}

fn status(value: i32) -> Result<(), DspError> {
    match value {
        0 => Ok(()),
        1 => Err(DspError::NativeArgument),
        2 => Err(DspError::NativeAllocation),
        3 => Err(DspError::NativeFailure),
        4 => Err(DspError::NonFiniteOutput),
        5 => Err(DspError::Poisoned),
        _ => Err(DspError::NativeReport),
    }
}
