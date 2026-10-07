//! The narrow Apple Vision adapter: one `VNTrackObjectRequest` driven by one
//! `VNSequenceRequestHandler`, or one `VNDetectFaceRectanglesRequest` on one
//! `VNImageRequestHandler`, over owned BGRA `CVPixelBuffer`s.
//!
//! Every `unsafe` block here calls an Objective-C or CoreVideo API whose
//! generated binding is `unsafe` only because the framework cannot prove its
//! contract to Rust; each block states the contract it relies on. Nothing
//! retains a borrowed pointer past its call: pixel bytes are copied into a
//! buffer CoreVideo owns, and every Objective-C object is reference-counted
//! through `Retained`/`CFRetained`.

use std::ptr::NonNull;

use deadpan_analysis::NormalizedRect;
use deadpan_analysis::generated_geometry::{
    FaceLandmarks, FaceObservationSet, FaceSetUnavailableReason, LandmarkAvailability,
    LandmarkRegion, LandmarkUnavailableReason, MAX_FACES_PER_PICTURE, MAX_LANDMARK_POINTS_PER_FACE,
    MAX_LANDMARK_POINTS_PER_REGION,
};

use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA,
    kCVReturnSuccess,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{
    VNDetectFaceLandmarksRequest, VNDetectFaceRectanglesRequest, VNDetectedObjectObservation,
    VNFaceLandmarkRegion2D, VNImageOption, VNImageRequestHandler, VNRequest,
    VNRequestFaceLandmarksConstellation, VNRequestTrackingLevel, VNSequenceRequestHandler,
    VNTrackObjectRequest,
};

pub const ENGINE: &str = "Apple Vision VNTrackObjectRequest";
pub const FACE_ENGINE: &str = "Apple Vision VNDetectFaceRectanglesRequest";
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

/// Faces Vision found in one picture, with the request revision it used.
pub struct Faces {
    /// Bounding boxes in Vision's convention and confidences, in Vision's order.
    pub faces: Vec<(VisionRect, f32)>,
    pub revision: u64,
}

/// Detect face rectangles in one picture with a fresh
/// `VNDetectFaceRectanglesRequest` on a fresh `VNImageRequestHandler`.
/// `limit` bounds how many observations are read; more is an error.
pub fn detect_faces(picture: &Picture<'_>, limit: usize) -> Result<Faces, String> {
    let buffer = pixel_buffer(picture)?;
    autoreleasepool(|_| {
        // SAFETY: plain constructors. The handler retains the pixel buffer,
        // which nothing modifies afterwards, and the empty options dictionary
        // has the declared key and value types.
        let (handler, request) = unsafe {
            let options = NSDictionary::<VNImageOption, AnyObject>::new();
            let handler = VNImageRequestHandler::initWithCVPixelBuffer_options(
                VNImageRequestHandler::alloc(),
                &buffer,
                &options,
            );
            (handler, VNDetectFaceRectanglesRequest::new())
        };
        let base: &VNRequest = &request;
        let requests = NSArray::from_slice(&[base]);
        // The handler performs the request synchronously before returning.
        handler
            .performRequests_error(&requests)
            .map_err(|error| error.localizedDescription().to_string())?;
        // SAFETY: property reads on a performed request.
        let (revision, results) = unsafe { (request.revision() as u64, request.results()) };
        let Some(results) = results else {
            return Ok(Faces {
                faces: Vec::new(),
                revision,
            });
        };
        if results.count() > limit {
            return Err(format!(
                "Vision found {} faces; at most {limit} are accepted",
                results.count()
            ));
        }
        let faces = results
            .iter()
            .map(|face| {
                // SAFETY: property reads on a live observation.
                let (rect, confidence) = unsafe { (face.boundingBox(), face.confidence()) };
                (
                    VisionRect {
                        x: rect.origin.x,
                        y: rect.origin.y,
                        width: rect.size.width,
                        height: rect.size.height,
                    },
                    confidence,
                )
            })
            .collect();
        Ok(Faces { faces, revision })
    })
}

/// Detect faces and copy only the five feature groups used by the host's
/// geometry/mouth guard. All Objective-C buffers stay inside this function.
/// Unsupported detector configurations and API errors fail the inspection;
/// absent or unusable landmark geometry remains explicit unavailable evidence.
pub fn detect_landmarks(picture: &Picture<'_>, rotation: u8) -> Result<FaceObservationSet, String> {
    if rotation > 3 {
        return Err("landmark picture rotation is invalid".into());
    }
    let buffer = pixel_buffer(picture)?;
    autoreleasepool(|_| {
        let constellation = VNRequestFaceLandmarksConstellation::Constellation76Points;
        let revision = deadpan_jobs::landmarks::REQUEST_REVISION as usize;
        // SAFETY: plain constructors and properties on live retained objects.
        // The revision/constellation pair is Apple's declared pair. A failed
        // support query refuses inspection rather than selecting a fallback.
        let (handler, request) = unsafe {
            if !VNDetectFaceLandmarksRequest::revision_supportsConstellation(
                revision,
                constellation,
            ) {
                return Err("Vision does not support landmark revision 3, constellation 76".into());
            }
            let options = NSDictionary::<VNImageOption, AnyObject>::new();
            let handler = VNImageRequestHandler::initWithCVPixelBuffer_options(
                VNImageRequestHandler::alloc(),
                &buffer,
                &options,
            );
            let request = VNDetectFaceLandmarksRequest::new();
            request.setRevision(revision);
            request.setConstellation(constellation);
            (handler, request)
        };
        let base: &VNRequest = &request;
        let requests = NSArray::from_slice(&[base]);
        handler
            .performRequests_error(&requests)
            .map_err(|error| error.localizedDescription().to_string())?;
        // SAFETY: property reads on the performed request.
        let (actual_revision, actual_constellation, results) = unsafe {
            (
                request.revision(),
                request.constellation(),
                request.results(),
            )
        };
        if actual_revision != revision || actual_constellation != constellation {
            return Err("Vision changed the pinned landmark configuration".into());
        }
        let Some(results) = results else {
            return Ok(FaceObservationSet::Detected { faces: Vec::new() });
        };
        if results.count() > MAX_FACES_PER_PICTURE {
            return Ok(FaceObservationSet::Unavailable {
                reason: FaceSetUnavailableReason::TooManyFaces,
            });
        }
        let mut faces = Vec::with_capacity(results.count());
        for face in results.iter() {
            // SAFETY: read the performed request's live retained observation.
            let (rect, confidence, landmarks) =
                unsafe { (face.boundingBox(), face.confidence(), face.landmarks()) };
            if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
                return Err("Vision returned invalid face confidence".into());
            }
            let box_ = VisionRect {
                x: rect.origin.x,
                y: rect.origin.y,
                width: rect.size.width,
                height: rect.size.height,
            };
            // A clipped/partial box cannot establish reliable feature geometry.
            let Ok(coded) =
                NormalizedRect::new(box_.x, 1.0 - box_.y - box_.height, box_.width, box_.height)
            else {
                return Ok(FaceObservationSet::Unavailable {
                    reason: FaceSetUnavailableReason::InvalidGeometry,
                });
            };
            let region = coded.displayed_from_coded(rotation);
            let landmarks = if let Some(landmarks) = landmarks {
                // SAFETY: property reads while the retained landmark object lives.
                let (confidence, left_eye, right_eye, nose, outer_lips, inner_lips) = unsafe {
                    (
                        landmarks.confidence(),
                        landmarks.leftEye(),
                        landmarks.rightEye(),
                        landmarks.nose(),
                        landmarks.outerLips(),
                        landmarks.innerLips(),
                    )
                };
                if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
                    return Err("Vision returned invalid landmark confidence".into());
                }
                let mut total = 0;
                LandmarkAvailability::Available {
                    confidence,
                    left_eye: copy_landmark_region(left_eye, box_, rotation, &mut total)?,
                    right_eye: copy_landmark_region(right_eye, box_, rotation, &mut total)?,
                    nose: copy_landmark_region(nose, box_, rotation, &mut total)?,
                    outer_lips: copy_landmark_region(outer_lips, box_, rotation, &mut total)?,
                    inner_lips: copy_landmark_region(inner_lips, box_, rotation, &mut total)?,
                }
            } else {
                LandmarkAvailability::Unavailable {
                    reason: LandmarkUnavailableReason::Missing,
                }
            };
            faces.push(FaceLandmarks {
                region,
                confidence,
                landmarks,
            });
        }
        faces.sort_by(FaceLandmarks::order);
        let result = FaceObservationSet::Detected { faces };
        // Duplicate indistinguishable boxes are ambiguous. Do not delete one
        // or renumber it into an invented persistent subject identity.
        if result.validate().is_err() {
            Ok(FaceObservationSet::Unavailable {
                reason: FaceSetUnavailableReason::InvalidGeometry,
            })
        } else {
            Ok(result)
        }
    })
}

fn copy_landmark_region(
    region: Option<Retained<VNFaceLandmarkRegion2D>>,
    face: VisionRect,
    rotation: u8,
    total: &mut usize,
) -> Result<LandmarkRegion, String> {
    let Some(region) = region else {
        return Ok(LandmarkRegion::Unavailable {
            reason: LandmarkUnavailableReason::Missing,
        });
    };
    // SAFETY: read the count before reading a pointer. The retained region
    // owns its point buffer until after every bounded point has been copied.
    let count = unsafe { region.pointCount() };
    if count == 0 {
        return Ok(LandmarkRegion::Unavailable {
            reason: LandmarkUnavailableReason::Missing,
        });
    }
    if count > MAX_LANDMARK_POINTS_PER_REGION
        || total
            .checked_add(count)
            .is_none_or(|count| count > MAX_LANDMARK_POINTS_PER_FACE)
    {
        return Err("Vision landmark point count exceeds the pinned constellation bound".into());
    }
    *total += count;
    // SAFETY: pointCount was bounded above; region stays retained throughout
    // this read/copy. Apple guarantees pointCount CGPoints at this pointer.
    let pointer = unsafe { region.normalizedPoints() };
    if pointer.is_null() {
        return Err("Vision returned a null nonempty landmark buffer".into());
    }
    // SAFETY: the non-null buffer is owned by the retained region and has the
    // checked count above. No borrowed slice escapes this function.
    let points = unsafe { std::slice::from_raw_parts(pointer, count) };
    let mut copied = Vec::with_capacity(count);
    for point in points {
        let Some(point) = displayed_landmark_point([point.x, point.y], face, rotation) else {
            return Ok(LandmarkRegion::Unavailable {
                reason: LandmarkUnavailableReason::InvalidGeometry,
            });
        };
        copied.push(point);
    }
    if copied.len() < 2 || copied.iter().all(|point| *point == copied[0]) {
        return Ok(LandmarkRegion::Unavailable {
            reason: LandmarkUnavailableReason::InvalidGeometry,
        });
    }
    Ok(LandmarkRegion::Detected { points: copied })
}

/// Vision points are normalized within their lower-left-origin face box.
/// Convert them to the complete displayed raster without clipping distortion.
fn displayed_landmark_point(point: [f64; 2], face: VisionRect, rotation: u8) -> Option<[f64; 2]> {
    if !point
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        return None;
    }
    let x = face.x + point[0] * face.width;
    let y = 1.0 - (face.y + point[1] * face.height);
    let point = match rotation {
        0 => [x, y],
        1 => [1.0 - y, x],
        2 => [1.0 - x, 1.0 - y],
        3 => [y, 1.0 - x],
        _ => return None,
    };
    point
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .then_some(point)
}

/// Copy a picture into a new BGRA pixel buffer owned by CoreVideo.
fn pixel_buffer(picture: &Picture<'_>) -> Result<CFRetained<CVPixelBuffer>, String> {
    let (width, height) = (picture.width as usize, picture.height as usize);
    let row_bytes = width.checked_mul(4).ok_or("picture width overflows")?;
    let minimum_bytes = height
        .checked_sub(1)
        .and_then(|rows| picture.row_stride_bytes.checked_mul(rows))
        .and_then(|bytes| bytes.checked_add(row_bytes));
    if width == 0
        || height == 0
        || picture.row_stride_bytes < row_bytes
        || minimum_bytes.is_none_or(|minimum| picture.rgba.len() < minimum)
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
        let Some(length) = stride
            .checked_mul(height)
            .filter(|length| *length <= isize::MAX as usize)
        else {
            // SAFETY: release the lock before returning the checked-size error.
            unsafe { CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0)) };
            return Err("CoreVideo buffer size overflows".into());
        };
        let destination = unsafe { std::slice::from_raw_parts_mut(base, length) };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_local_points_convert_to_the_full_raster_and_display_rotation() {
        let face = VisionRect {
            x: 0.2,
            y: 0.3,
            width: 0.4,
            height: 0.5,
        };
        for (rotation, expected) in [
            (0, [0.3, 0.3]),
            (1, [0.7, 0.3]),
            (2, [0.7, 0.7]),
            (3, [0.3, 0.7]),
        ] {
            let actual = displayed_landmark_point([0.25, 0.8], face, rotation).unwrap();
            assert!((actual[0] - expected[0]).abs() < 1e-12);
            assert!((actual[1] - expected[1]).abs() < 1e-12);
        }
        for point in [[f64::NAN, 0.2], [-0.01, 0.2], [0.2, 1.01]] {
            assert!(displayed_landmark_point(point, face, 0).is_none());
        }
        assert!(displayed_landmark_point([0.5, 0.5], face, 4).is_none());
    }

    #[test]
    fn pixel_buffer_rejects_truncated_or_overflowing_input_before_allocation() {
        for picture in [
            Picture {
                width: 0,
                height: 1,
                row_stride_bytes: 4,
                rgba: &[0; 4],
            },
            Picture {
                width: 1,
                height: 1,
                row_stride_bytes: 3,
                rgba: &[0; 4],
            },
            Picture {
                width: 1,
                height: 1,
                row_stride_bytes: 4,
                rgba: &[0; 3],
            },
            Picture {
                width: 1,
                height: 3,
                row_stride_bytes: usize::MAX,
                rgba: &[0; 4],
            },
        ] {
            assert!(pixel_buffer(&picture).is_err());
        }
    }
}
