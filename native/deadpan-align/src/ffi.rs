//! The sole unsafe adapter: C++ owns each opaque session; every buffer and
//! callback context is borrowed only for a synchronous call. The native watcher
//! joins before returning, so neither the control nor tensor pointers escape.

use crate::{AlignError, Control, Emissions, Features, LABELS, MAX_OUTPUT_FRAMES};
use std::{
    ffi::{c_char, c_int, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};

#[repr(C)]
struct DpAlignSession {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn dp_align_create(
        graph: *const u8,
        graph_len: usize,
        error: *mut c_char,
        error_len: usize,
    ) -> *mut DpAlignSession;
    fn dp_align_destroy(session: *mut DpAlignSession);
    fn dp_align_run(
        session: *mut DpAlignSession,
        features: *const f32,
        frames: usize,
        output: *mut f32,
        output_capacity: usize,
        output_frames: *mut usize,
        cancelled: extern "C" fn(*const c_void) -> c_int,
        context: *const c_void,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
}

pub(super) struct NativeSession {
    pointer: NonNull<DpAlignSession>,
    // Deliberately bind the session to the constructing worker thread.
    _thread: PhantomData<Rc<()>>,
}

fn error_text(error: &[u8]) -> AlignError {
    let end = error.iter().position(|v| *v == 0).unwrap_or(error.len());
    AlignError::Native(String::from_utf8_lossy(&error[..end]).into_owned())
}

extern "C" fn cancelled(context: *const c_void) -> c_int {
    // SAFETY: dp_align_run calls only with the live Control reference below,
    // and joins its watcher thread before returning. This callback never panics.
    let control = unsafe { &*context.cast::<Control>() };
    c_int::from(control.check().is_err())
}

impl NativeSession {
    pub(super) fn new(graph: &[u8], control: &Control) -> Result<Self, AlignError> {
        control.check()?;
        let mut error = [0_u8; 512];
        // SAFETY: graph and bounded writable error remain live throughout the
        // synchronous constructor. ORT owns its initialized model afterwards.
        let pointer = unsafe {
            dp_align_create(
                graph.as_ptr(),
                graph.len(),
                error.as_mut_ptr().cast(),
                error.len(),
            )
        };
        let pointer = NonNull::new(pointer).ok_or_else(|| error_text(&error))?;
        let value = Self {
            pointer,
            _thread: PhantomData,
        };
        control.check()?;
        Ok(value)
    }

    pub(super) fn infer(
        &mut self,
        features: &Features,
        control: &Control,
    ) -> Result<Emissions, AlignError> {
        control.check()?;
        let mut output = vec![0_f32; MAX_OUTPUT_FRAMES * LABELS];
        let mut frames = 0_usize;
        let mut error = [0_u8; 512];
        // SAFETY: &mut self excludes overlapping runs/destruction. Features,
        // output, frame count, error and control remain live until native code
        // returns; callback watcher teardown occurs before that return.
        let result = unsafe {
            dp_align_run(
                self.pointer.as_ptr(),
                features.values().as_ptr(),
                features.frames(),
                output.as_mut_ptr(),
                output.len(),
                &mut frames,
                cancelled,
                (control as *const Control).cast(),
                error.as_mut_ptr().cast(),
                error.len(),
            )
        };
        // Give a cancelled/deadline result its precise Rust error even when
        // ORT reports its generic RunOptions termination exception.
        control.check()?;
        if result == 2 {
            return Err(AlignError::Cancelled);
        }
        if result != 0 {
            return Err(error_text(&error));
        }
        if frames != features.frames().div_ceil(2) || frames > MAX_OUTPUT_FRAMES {
            return Err(AlignError::Integrity("native output count"));
        }
        output.truncate(frames * LABELS);
        if output.iter().any(|v| !v.is_finite()) {
            return Err(AlignError::Integrity("nonfinite native output"));
        }
        Ok(Emissions {
            frames,
            log_probs: output,
        })
    }
}

impl Drop for NativeSession {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner and every prior synchronous run ended.
        unsafe { dp_align_destroy(self.pointer.as_ptr()) };
    }
}
