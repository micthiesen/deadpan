//! Full index proof followed by bounded outward inspection from one anchor.
//! Reverse tracking seeks one exact generated picture at a time, retaining no
//! whole-movie pixel buffer. Returned observations use canonical chronology.

use super::*;
use deadpan_analysis::generated_geometry::extension::{
    RAW_EXTENSION_LANDMARK_SCHEMA_VERSION, RawExtensionLandmarkBatch,
};
use deadpan_analysis::generated_region::extension::RawExtensionRegionBatch;
use deadpan_jobs::landmarks::{
    EXTENSION_OBSERVATIONS_SCHEMA_VERSION, InspectionExtensionObservations,
    MAX_EXTENSION_NATIVE_FRAMES,
};

const MAX_SEEK_PICTURES: usize = 64;
const MAX_DECODED_PICTURES: u64 =
    MAX_EXTENSION_NATIVE_FRAMES as u64 + MAX_FRAMES as u64 * MAX_SEEK_PICTURES as u64;
const MAX_DECODE_PACKETS: u64 = MAX_DECODED_PICTURES * 64;
const MAX_DECODE_IO_BYTES: u64 = 64 * 1024 * 1024 * 1024;

fn check_work(decoder: &SourceDecoder) -> Result<(), String> {
    let work = decoder.work();
    if work.frames > MAX_DECODED_PICTURES
        || work.decoded_pictures > MAX_DECODED_PICTURES
        || work.packets > MAX_DECODE_PACKETS
        || work.io_bytes > MAX_DECODE_IO_BYTES
    {
        return Err("extension inspection exceeded cumulative decode work".into());
    }
    Ok(())
}

fn seek_picture(
    decoder: &mut SourceDecoder,
    ordinal: u32,
    request: &Request,
    cancelled: &AtomicBool,
) -> Result<deadpan_source::DecodedRgbaFrame, Interrupted> {
    let control = || decode_control(cancelled, request.deadline);
    let target = request.picture_pts[ordinal as usize];
    decoder
        .seek(target, control()?)
        .map_err(|error| format!("seek extension picture: {error}"))?;
    control()?;
    check_work(decoder)?;
    let mut previous = None;
    for _ in 0..MAX_SEEK_PICTURES {
        let metadata = decoder
            .next_metadata(control()?)
            .map_err(|error| format!("decode extension seek: {error}"))?
            .ok_or_else(|| "extension seek ended before its indexed picture".to_owned())?;
        control()?;
        check_work(decoder)?;
        let found = request
            .picture_pts
            .binary_search(&metadata.pts)
            .map_err(|_| "extension seek returned a picture outside the proven index".to_owned())?;
        if found > ordinal as usize || previous.is_some_and(|prior| found != prior + 1) {
            return Err("extension seek skipped or reordered indexed pictures"
                .to_owned()
                .into());
        }
        previous = Some(found);
        if found == ordinal as usize {
            let picture = decoder
                .copy_current_rgba(control()?)
                .map_err(|error| format!("convert extension picture: {error}"))?;
            control()?;
            check_work(decoder)?;
            if picture.width != request.stream.width
                || picture.height != request.stream.height
                || picture.sample_bits != 8
                || picture.metadata.pts != target
            {
                return Err("extension picture differs from its captured raster or PTS"
                    .to_owned()
                    .into());
            }
            return crate::analysis_picture(picture, decoder.info(), cancelled, request.deadline);
        }
    }
    Err("extension seek exceeded its picture budget"
        .to_owned()
        .into())
}

pub(super) fn inspect(
    job: &Job,
    request: &Request,
    extension: &ExtensionRequest,
    cancelled: &AtomicBool,
) -> Result<WorkerMessage, Interrupted> {
    let started = Instant::now();
    let control = || decode_control(cancelled, request.deadline);
    control()?;
    extension
        .coverage
        .validate(&request.picture_pts)
        .map_err(|error| error.to_string())?;
    let file = open_source(&request.source)?;
    verify_source(&file, &request.source, cancelled, request.deadline)?;
    let mut decoder = SourceDecoder::open(
        file,
        DecodeLimits {
            max_input_bytes: request.source.byte_length(),
            // Decoder seek limits reset internally. check_work retains a separate
            // cumulative bound across the index and every exact reverse seek.
            max_frames: MAX_EXTENSION_NATIVE_FRAMES as u64 + 1,
            max_packets: (MAX_EXTENSION_NATIVE_FRAMES as u64 + 1) * 64,
            max_pixels: MAX_PIXELS,
            max_dimension: MAX_DIMENSION,
            ..DecodeLimits::default()
        },
        control()?,
    )
    .map_err(|error| format!("open extension landmark source: {error}"))?;
    check_stream(decoder.info(), &request.stream)?;
    if decoder.info().codec != "ffv1"
        || !decoder.info().audio_streams.is_empty()
        || decoder.info().sample_aspect_num != decoder.info().sample_aspect_den
    {
        return Err(
            "extension landmark input is not a canonical silent FFV1 source"
                .to_owned()
                .into(),
        );
    }
    // Prove every canonical PTS, including context, before accepting coverage.
    for (ordinal, &pts) in request.picture_pts.iter().enumerate() {
        let metadata = decoder
            .next_metadata(control()?)
            .map_err(|error| format!("index extension picture: {error}"))?
            .ok_or_else(|| format!("extension decoding ended before picture {ordinal}"))?;
        control()?;
        check_work(&decoder)?;
        if metadata.pts != pts {
            return Err(format!(
                "extension picture {ordinal} is at PTS {}, expected {pts}",
                metadata.pts
            )
            .into());
        }
    }
    if decoder
        .next_metadata(control()?)
        .map_err(|error| format!("verify extension end: {error}"))?
        .is_some()
    {
        return Err(
            "extension source has pictures beyond the requested sequence"
                .to_owned()
                .into(),
        );
    }
    control()?;
    check_work(&decoder)?;

    let png = decode_png(&extension.anchor, request, cancelled)?;
    let mut tracker = extension.region_seed.map(RegionTracker::new).transpose()?;
    let vision_started = Instant::now();
    let anchor = png.detect()?;
    control()?;
    let region_anchor = tracker
        .as_mut()
        .map(|tracker| tracker.track(&png, false))
        .transpose()?;
    control()?;
    let mut vision_time = vision_started.elapsed();
    drop(png);
    let ordinals = extension.coverage.tracking_ordinals();
    let total = ordinals.len() + 1;
    let mut last_percent = None;
    progress(job, 1, total, &mut last_percent)?;
    let mut frames = Vec::with_capacity(ordinals.len());
    let mut region_frames = Vec::with_capacity(if tracker.is_some() { ordinals.len() } else { 0 });
    for (index, ordinal) in ordinals.into_iter().enumerate() {
        let picture = seek_picture(&mut decoder, ordinal, request, cancelled)?;
        let vision_started = Instant::now();
        let observation = detect_picture(
            picture.width,
            picture.height,
            picture.row_stride_bytes,
            &picture.rgba,
            0,
        )?;
        control()?;
        if let Some(tracker) = &mut tracker {
            region_frames.push(RawRegionFrame {
                ordinal,
                pts: picture.metadata.pts,
                observation: tracker.track_picture(
                    picture.width,
                    picture.height,
                    picture.row_stride_bytes,
                    &picture.rgba,
                    index + 2 == total,
                )?,
            });
        }
        control()?;
        vision_time += vision_started.elapsed();
        frames.push(FrameObservation {
            ordinal,
            pts: picture.metadata.pts,
            observation,
        });
        progress(job, index + 2, total, &mut last_percent)?;
    }
    frames.sort_by_key(|frame| frame.ordinal);
    region_frames.sort_by_key(|frame| frame.ordinal);
    let region = match (extension.region_seed, region_anchor) {
        (Some(seed), Some(anchor)) => Some(RawExtensionRegionBatch {
            schema_version: 1,
            coverage: extension.coverage,
            seed,
            anchor,
            frames: region_frames,
        }),
        (None, None) => None,
        _ => {
            return Err("extension lost its requested region anchor"
                .to_owned()
                .into());
        }
    };
    let batch = InspectionExtensionObservations {
        schema_version: EXTENSION_OBSERVATIONS_SCHEMA_VERSION,
        landmarks: RawExtensionLandmarkBatch {
            schema_version: RAW_EXTENSION_LANDMARK_SCHEMA_VERSION,
            coverage: extension.coverage,
            anchor,
            frames,
        },
        region,
    };
    batch.validate(
        &request.picture_pts,
        &extension.coverage,
        extension.region_seed.as_ref(),
    )?;
    control()?;
    let mut bytes = BoundedBytes {
        bytes: Vec::new(),
        maximum: request.maximum_output_bytes,
    };
    serde_json::to_writer(&mut bytes, &batch).map_err(|error| error.to_string())?;
    control()?;
    let observations = write_output(
        &request.output_scope,
        OUTPUT_FILE,
        &bytes.bytes,
        request.maximum_output_bytes,
    )?;
    control()?;
    let elapsed = started.elapsed();
    let millis = |duration: Duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    Ok(WorkerMessage::Completed {
        protocol: VERSION,
        request: job.request.clone(),
        attempt: job.attempt.clone(),
        observations,
        runtime: RuntimeReport {
            engine: ENGINE.into(),
            request_revision: REQUEST_REVISION,
            constellation: CONSTELLATION,
        },
        region_runtime: extension.region_seed.map(|_| RegionRuntimeReport {
            engine: REGION_ENGINE.into(),
            request_revision: REGION_REQUEST_REVISION,
            tracking_level: REGION_TRACKING_LEVEL.into(),
        }),
        decoded: request.picture_pts.len() as u32,
        analysed: total as u32,
        decode_millis: millis(elapsed.saturating_sub(vision_time)),
        vision_millis: millis(vision_time),
        elapsed_millis: millis(elapsed),
    })
}
