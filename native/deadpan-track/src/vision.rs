//! The narrow Apple Vision adapter: one `VNTrackObjectRequest` driven by one
//! `VNSequenceRequestHandler` over owned BGRA `CVPixelBuffer`s.
//!
//! Every `unsafe` block here calls an Objective-C or CoreVideo API whose
//! generated binding is `unsafe` only because the framework cannot prove its
//! contract to Rust; each block states the contract it relies on. Nothing
//! retains a borrowed pointer past its call: pixel bytes are copied into a
//! buffer CoreVideo owns, and every Objective-C object is reference-counted
//! through `Retained`/`CFRetained`.

use std::ptr::NonNull;

use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA,
    kCVReturnSuccess,
};
use objc2_foundation::NSArray;
use objc2_vision::{
    VNDetectedObjectObservation, VNRequest, VNRequestTrackingLevel, VNSequenceRequestHandler,
    VNTrackObjectRequest,
};

pub const ENGINE: &str = "Apple Vision VNTrackObjectRequest";
pub const TRACKING_LEVEL: &str = "accurate";

/// A box in Vision's convention: normalized to the coded picture, origin at
/// the lower-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisionRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One packed RGBA8 picture borrowed from the decoder.
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    pub row_stride_bytes: usize,
    pub rgba: &'a [u8],
}

pub struct Tracker {
    handler: Retained<VNSequenceRequestHandler>,
    request: Retained<VNTrackObjectRequest>,
}

impl Tracker {
    /// Seed the tracker with the selected box on the first picture.
    pub fn new(seed: VisionRect) -> Self {
        let rect = CGRect::new(
            CGPoint::new(seed.x, seed.y),
            CGSize::new(seed.width, seed.height),
        );
        // SAFETY: plain class constructors and an initializer on a freshly
        // allocated request; the seed observation is a valid retained object
        // for the duration of the call, and the tracking level is one of
        // Vision's two declared values.
        unsafe {
            let observation = VNDetectedObjectObservation::observationWithBoundingBox(rect);
            let request = VNTrackObjectRequest::initWithDetectedObjectObservation(
                VNTrackObjectRequest::alloc(),
                &observation,
            );
            request.setTrackingLevel(VNRequestTrackingLevel::Accurate);
            Self {
                handler: VNSequenceRequestHandler::new(),
                request,
            }
        }
    }

    /// The request revision Vision actually uses.
    pub fn revision(&self) -> u64 {
        // SAFETY: a property read on a live request.
        unsafe { self.request.revision() as u64 }
    }

    /// Track into `picture`. `Ok(None)` means Vision produced no observation;
    /// `Err` carries Vision's own error description. Mark the sequence's
    /// final picture `last` so Vision releases its tracker after it.
    pub fn track(
        &mut self,
        picture: &Picture<'_>,
        last: bool,
    ) -> Result<Option<(VisionRect, f32)>, String> {
        if last {
            // SAFETY: a property write on a live request before it is performed.
            unsafe { self.request.setLastFrame(true) };
        }
        let buffer = pixel_buffer(picture)?;
        autoreleasepool(|_| {
            let request: &VNRequest = &self.request;
            let requests = NSArray::from_slice(&[request]);
            // SAFETY: the handler, the request array and the pixel buffer are
            // live retained objects for the whole synchronous call; Vision
            // performs the request before returning.
            let performed = unsafe {
                self.handler
                    .performRequests_onCVPixelBuffer_error(&requests, &buffer)
            };
            if let Err(error) = performed {
                return Err(error.localizedDescription().to_string());
            }
            // SAFETY: `results` may be read after the request was performed.
            let Some(results) = (unsafe { self.request.results() }) else {
                return Ok(None);
            };
            let Some(first) = results.firstObject() else {
                return Ok(None);
            };
            let Ok(observation) = first.downcast::<VNDetectedObjectObservation>() else {
                return Err("Vision returned an unexpected observation kind".into());
            };
            // SAFETY: property reads on a live observation; feeding it back
            // as the next input is Vision's documented tracking loop.
            let (rect, confidence) = unsafe {
                let rect = observation.boundingBox();
                let confidence = observation.confidence();
                self.request.setInputObservation(&observation);
                (rect, confidence)
            };
            Ok(Some((
                VisionRect {
                    x: rect.origin.x,
                    y: rect.origin.y,
                    width: rect.size.width,
                    height: rect.size.height,
                },
                confidence,
            )))
        })
    }
}

/// Copy a picture into a new BGRA pixel buffer owned by CoreVideo.
fn pixel_buffer(picture: &Picture<'_>) -> Result<CFRetained<CVPixelBuffer>, String> {
    let (width, height) = (picture.width as usize, picture.height as usize);
    let row_bytes = width.checked_mul(4).ok_or("picture width overflows")?;
    if width == 0
        || height == 0
        || picture.row_stride_bytes < row_bytes
        || picture.rgba.len() < picture.row_stride_bytes * (height - 1) + row_bytes
    {
        return Err("decoded picture is smaller than its declared size".into());
    }
    let mut created: *mut CVPixelBuffer = std::ptr::null_mut();
    // SAFETY: `created` is a valid out-pointer for the call; no attributes
    // dictionary is passed and the format is CoreVideo's packed BGRA.
    let status = unsafe {
        CVPixelBufferCreate(
            None,
            width,
            height,
            kCVPixelFormatType_32BGRA,
            None,
            NonNull::from(&mut created),
        )
    };
    let created = NonNull::new(created).filter(|_| status == kCVReturnSuccess);
    let Some(created) = created else {
        return Err(format!("CVPixelBufferCreate failed with {status}"));
    };
    // SAFETY: a successful create returns a +1 reference that we now own.
    let buffer = unsafe { CFRetained::from_raw(created) };
    if CVPixelBufferGetWidth(&buffer) != width || CVPixelBufferGetHeight(&buffer) != height {
        return Err("CoreVideo allocated a different picture size".into());
    }
    // SAFETY: locking a live buffer we own for writing.
    let locked = unsafe { CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags(0)) };
    if locked != kCVReturnSuccess {
        return Err(format!("CVPixelBufferLockBaseAddress failed with {locked}"));
    }
    let base = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>();
    let stride = CVPixelBufferGetBytesPerRow(&buffer);
    let result = if base.is_null() || stride < row_bytes {
        Err("CoreVideo returned an unusable pixel buffer".to_owned())
    } else {
        // SAFETY: while locked, `base` addresses `stride * height` writable
        // bytes of this single-plane buffer, which nothing else references.
        let destination = unsafe { std::slice::from_raw_parts_mut(base, stride * height) };
        for (row, target) in destination.chunks_exact_mut(stride).enumerate() {
            let source = &picture.rgba
                [row * picture.row_stride_bytes..row * picture.row_stride_bytes + row_bytes];
            for (from, to) in source
                .chunks_exact(4)
                .zip(target[..row_bytes].chunks_exact_mut(4))
            {
                to.copy_from_slice(&[from[2], from[1], from[0], 255]);
            }
        }
        Ok(())
    };
    // SAFETY: unlocking the lock taken above, with the same flags.
    let unlocked = unsafe { CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0)) };
    result?;
    if unlocked != kCVReturnSuccess {
        return Err(format!(
            "CVPixelBufferUnlockBaseAddress failed with {unlocked}"
        ));
    }
    Ok(buffer)
}
