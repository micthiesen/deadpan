use std::{
    fs::File,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_encode::{AUDIO_FRAME_SAMPLES, AUDIO_PRIMING_SAMPLES, EncodeContract, HdrTransfer};
use deadpan_source::{
    DecodeControl, DecodeLimits, Mp4Inspection, Mp4PacketReader, Mp4TrackInspection, Mp4TrackKind,
};

use super::{
    ContentLightEvidence, VerificationLimits, VerificationProgress, VerificationReport,
    VerificationStage,
};
use crate::{encoded_render::protocol::EncodedManifest, render_worker::worker::check_control};

mod audio;
#[cfg(all(test, target_os = "macos"))]
mod hdr_tests;
mod light;
pub(super) use light::DecodedLight;
mod pictures;

type Result<T> = std::result::Result<T, String>;
const IDENTITY: [i32; 9] = [65_536, 0, 0, 0, 65_536, 0, 0, 0, 1 << 30];

struct Context<'a> {
    file: &'a File,
    manifest: &'a EncodedManifest,
    limits: DecodeLimits,
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl Context<'_> {
    fn control(&self) -> Result<DecodeControl<'_>> {
        check_control(self.cancelled, self.deadline)?;
        Ok(DecodeControl {
            cancelled: self.cancelled,
            timeout: self
                .deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(60)),
        })
    }
    fn file(&self) -> Result<File> {
        self.file.try_clone().map_err(|e| e.to_string())
    }
}

pub(crate) fn inspect(
    file: &File,
    manifest: &EncodedManifest,
    limits: VerificationLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(VerificationProgress) -> Result<()>,
) -> Result<VerificationReport> {
    limits.validate()?;
    manifest.validate()?;
    let native = manifest.contract.native_contract()?;
    let pixels = super::decode_pixel_budget(native.raster(), native.video_format().is_hdr());
    let context = Context {
        file,
        manifest,
        cancelled,
        deadline,
        limits: DecodeLimits {
            progressive_only: true,
            max_input_bytes: limits.maximum_bytes,
            max_frames: native.video_frames() + 1,
            max_packets: limits.maximum_packets + 1,
            max_packet_bytes: 16 * 1024 * 1024,
            max_pixels: pixels,
            ..DecodeLimits::default()
        },
    };
    let mut packets = Mp4PacketReader::open(context.file()?, context.limits, context.control()?)
        .map_err(|e| e.to_string())?;
    let movie = packets.inspection().clone();
    let (video, sound) = container(&movie, manifest)?;
    let (keys, reorder_delay) = packet_scan(&context, &mut packets, video, sound, &mut progress)?;
    drop(packets);
    let video_result = pictures::inspect(
        &context,
        video,
        &keys,
        native.video_format().is_hdr().then_some(reorder_delay),
        &mut progress,
    )?;
    let content_light = match (video.content_light, video_result.light) {
        (Some(declared), Some(decoded)) => Some(ContentLightEvidence::new(declared, decoded)?),
        (None, None) => None,
        _ => return Err("content light declaration and decoded measurement disagree".into()),
    };
    let manual = audio::inspect(
        &context,
        sound,
        deadpan_source::audio::AudioDecodeMode::Manual,
        &mut progress,
    )?;
    let ordinary = audio::inspect(
        &context,
        sound,
        deadpan_source::audio::AudioDecodeMode::Ordinary,
        &mut progress,
    )?;
    require(
        manual.presented_digest == ordinary.presented_digest,
        "ordinary and manual AAC disagree at fixed authored sample coordinates",
    )?;
    check_control(cancelled, deadline)?;
    let report = VerificationReport {
        policy_version: 2,
        contract: manifest.contract.clone(),
        document_sha256: manifest.document_sha256.clone(),
        movie_sha256: manifest.movie.sha256().clone(),
        movie_bytes: manifest.movie.byte_length(),
        video_frames: native.video_frames(),
        audio_samples: native.audio_samples(),
        video_packets: u64::from(video.sample_count),
        audio_packets: u64::from(sound.sample_count),
        gops: u64::try_from(keys.iter().filter(|&&value| value).count())
            .map_err(|_| "GOP count overflow")?,
        fresh_gop_frames: video_result.fresh_frames,
        maximum_b_run: video_result.maximum_b_run,
        runtime_versions: video_result.runtime_versions,
        movie_timescale: movie.movie_timescale,
        video_edit_media_time: video.edits[0].media_time,
        audio_edit_media_time: sound.edits[0].media_time,
        manual_first_sample: manual.first_sample,
        manual_physical_samples: manual.physical_samples,
        ordinary_first_sample: ordinary.first_sample,
        ordinary_physical_samples: ordinary.physical_samples,
        content_light,
    };
    report.validate(limits)?;
    Ok(report)
}

fn require(ok: bool, message: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(format!("ExportVerificationFailed: {message}"))
    }
}

fn scaled(value: u64, from: u32, to: u32) -> Result<u64> {
    let amount = u128::from(value) * u128::from(to);
    if from == 0 || amount % u128::from(from) != 0 {
        return Err("output clock is not exactly representable".into());
    }
    u64::try_from(amount / u128::from(from)).map_err(|_| "output clock overflow".into())
}

fn container<'a>(
    movie: &'a Mp4Inspection,
    manifest: &EncodedManifest,
) -> Result<(&'a Mp4TrackInspection, &'a Mp4TrackInspection)> {
    let native = manifest.contract.native_contract()?;
    let [num, den] = native.frame_rate();
    let video_ticks = native
        .video_frames()
        .checked_mul(u64::from(den))
        .ok_or("video duration overflow")?;
    let video_movie = scaled(video_ticks, num, movie.movie_timescale)?;
    let audio_movie = scaled(native.audio_samples(), 48_000, movie.movie_timescale)?;
    require(
        movie.moov_before_mdat
            && movie.movie_matrix == IDENTITY
            && movie.tracks.len() == 2
            && movie.movie_timescale == native.policy().movie_timescale
            && movie.movie_duration == Some(video_movie.max(audio_movie)),
        "movie geometry, duration or fast-start differs",
    )?;
    let video = movie
        .tracks
        .iter()
        .find(|t| t.kind == Mp4TrackKind::Video)
        .ok_or("missing video track")?;
    let sound = movie
        .tracks
        .iter()
        .find(|t| t.kind == Mp4TrackKind::Audio)
        .ok_or("missing audio track")?;
    for (track, duration) in [(video, video_movie), (sound, audio_movie)] {
        require(
            track.flags & 3 == 3
                && track.matrix == IDENTITY
                && track.duration == Some(duration)
                && track.edits.len() == 1
                && track.edits[0].segment_duration == duration
                && track.edits[0].media_time >= 0
                && track.edits[0].media_rate_integer == 1
                && track.edits[0].media_rate_fraction == 0,
            "track requires one exact normal-rate media edit and identity transform",
        )?;
    }
    let audio_media = native
        .audio_samples()
        .checked_add(u64::from(AUDIO_PRIMING_SAMPLES))
        .ok_or("audio duration overflow")?;
    require(
        video.sample_dimensions == Some(native.raster())
            && video.display_width_16_16 == native.raster()[0] << 16
            && video.display_height_16_16 == native.raster()[1] << 16
            && video.media_timescale == num
            && video.media_duration == Some(video_ticks)
            && video.timing_duration == video_ticks
            && u64::from(video.sample_count) == native.video_frames()
            && sample_description(video, &native)?
            && video
                .pixel_aspect_ratio
                .is_none_or(|ratio| ratio[0] == ratio[1]),
        "video sample description or media clock differs",
    )?;
    require(
        sound.display_width_16_16 == 0
            && sound.display_height_16_16 == 0
            && sound.media_timescale == 48_000
            && sound.media_duration == Some(audio_media)
            && sound.timing_duration == audio_media
            && sound.edits[0].media_time == i64::from(AUDIO_PRIMING_SAMPLES)
            && sound.sample_audio_channels == Some(2)
            && sound.sample_audio_rate == Some(48_000)
            && u64::from(sound.sample_count)
                == native
                    .audio_samples()
                    .div_ceil(u64::from(AUDIO_FRAME_SAMPLES))
                    + u64::from(AUDIO_PRIMING_SAMPLES / AUDIO_FRAME_SAMPLES),
        "audio sample tables do not retain exact AAC priming and authored endpoint",
    )?;
    let reorder = u64::try_from(video.edits[0].media_time).map_err(|_| "negative reorder edit")?;
    require(
        reorder.is_multiple_of(u64::from(den))
            && reorder / u64::from(den) <= u64::from(native.policy().b_frames),
        "video media edit exceeds the captured reordering policy",
    )?;
    Ok((video, sound))
}

/// Codec configuration, `colr` and static HDR boxes for the contract format.
/// SDR is H.264 High `avc1` with limited Rec.709 and no HDR boxes. HDR is
/// HEVC Main10 `hvc1` (4:2:0, 10-bit) with limited BT.2020 NCL and the PQ or
/// HLG transfer. PQ carries exactly the contract `mdcv` (or none) and a
/// `clli`; HLG carries neither box.
fn sample_description(video: &Mp4TrackInspection, native: &EncodeContract) -> Result<bool> {
    let colr = |transfer: u16, primaries_matrix: u16| {
        video.color.is_some_and(|color| {
            color.primaries == primaries_matrix
                && color.transfer == transfer
                && color.matrix == primaries_matrix
                && !color.full_range
                && color.range_byte == 0
        })
    };
    let Some(hdr) = native.hdr() else {
        return Ok(video.avc.is_some_and(|avc| avc.profile == 100)
            && video.hevc.is_none()
            && video.mastering.is_none()
            && video.content_light.is_none()
            && colr(1, 1));
    };
    let main10 = video.avc.is_none()
        && video.hevc.is_some_and(|hevc| {
            hevc.profile_space == 0
                && hevc.profile_idc == 2
                && hevc.chroma_format_idc == 1
                && hevc.bit_depth_luma == 10
                && hevc.bit_depth_chroma == 10
        });
    let mastering = video
        .mastering
        .map(|m| (m.primaries, m.white_point, m.max_luminance, m.min_luminance))
        == hdr
            .signal
            .mastering
            .map(|m| (m.primaries, m.white_point, m.max_luminance, m.min_luminance));
    Ok(main10
        && mastering
        && match hdr.signal.transfer {
            HdrTransfer::Pq => {
                colr(16, 9)
                    && video.content_light.is_some_and(|light| {
                        light.max_fall <= light.max_cll && light.max_cll <= 10_000
                    })
            }
            HdrTransfer::Hlg => colr(18, 9) && video.content_light.is_none(),
        })
}

fn packet_scan(
    context: &Context<'_>,
    reader: &mut Mp4PacketReader,
    video: &Mp4TrackInspection,
    sound: &Mp4TrackInspection,
    progress: &mut impl FnMut(VerificationProgress) -> Result<()>,
) -> Result<(Vec<bool>, u32)> {
    let native = context.manifest.contract.native_contract()?;
    let frames = usize::try_from(native.video_frames())
        .map_err(|_| "picture count exceeds address space")?;
    let mut seen = Vec::new();
    seen.try_reserve_exact(frames).map_err(|e| e.to_string())?;
    seen.resize(frames, false);
    let mut keys = Vec::new();
    keys.try_reserve_exact(frames).map_err(|e| e.to_string())?;
    keys.resize(frames, false);
    let den = i64::from(native.frame_rate()[1]);
    let total = u64::from(video.sample_count) + u64::from(sound.sample_count);
    let mut counts = [0_u64; 2];
    let mut completed = 0_u64;
    // Largest decode-order index minus presentation index: the number of
    // frames a picture waits for reordering. pts >= dts bounds it by the edit.
    let mut reorder_delay = 0_u64;
    while let Some(packet) = reader
        .next_packet(context.control()?)
        .map_err(|e| e.to_string())?
    {
        let timing = packet
            .presentation
            .ok_or("ambiguous packet presentation mapping")?;
        if packet.track_index == video.index {
            require(
                packet.track_id == video.id
                    && u64::from(packet.sample_index) == counts[0]
                    && i128::from(timing.dts)
                        == i128::from(counts[0]) * i128::from(den)
                            - i128::from(video.edits[0].media_time)
                    && i64::from(packet.duration) == den
                    && timing.pts >= 0
                    && timing.pts % den == 0
                    && timing.pts >= timing.dts,
                "video packet clock, identity or duration differs",
            )?;
            let index =
                usize::try_from(timing.pts / den).map_err(|_| "video PTS exceeds address space")?;
            require(
                index < seen.len() && !seen[index],
                "duplicate or out-of-range video presentation timestamp",
            )?;
            seen[index] = true;
            reorder_delay = reorder_delay
                .max(counts[0].saturating_sub(u64::try_from(index).map_err(|_| "index overflow")?));
            if native.video_format().is_hdr() {
                let hevc = packet
                    .hevc
                    .filter(|_| packet.h264.is_none())
                    .ok_or("video packet lacks admitted HEVC observations")?;
                // The encoder strips VideoToolbox's Dolby Vision RPUs (type
                // 62); an unspecified 62/63 NAL must never reach a verified file.
                require(
                    hevc.nal_types & (3_u64 << 62) == 0,
                    "HEVC packet carries Dolby Vision or unspecified NAL type 62/63",
                )?;
                // Sync samples are IDR-only access units, so a fresh decoder
                // has no leading pictures to skip; other samples carry no IRAP.
                require(
                    hevc.irap_nal_count + hevc.non_irap_vcl_nal_count > 0
                        && if packet.table_sync {
                            hevc.idr_nal_count > 0
                                && hevc.irap_nal_count == hevc.idr_nal_count
                                && hevc.non_irap_vcl_nal_count == 0
                        } else {
                            hevc.irap_nal_count == 0
                        },
                    "sync table differs from actual HEVC IDR picture NALs",
                )?;
            } else {
                let h264 = packet
                    .h264
                    .filter(|_| packet.hevc.is_none())
                    .ok_or("video packet lacks admitted AVC observations")?;
                require(
                    h264.idr_nal_count + h264.non_idr_vcl_nal_count > 0
                        && if packet.table_sync {
                            h264.idr_nal_count > 0 && h264.non_idr_vcl_nal_count == 0
                        } else {
                            h264.idr_nal_count == 0
                        },
                    "sync table differs from actual IDR picture NALs",
                )?;
            }
            keys[index] = packet.table_sync;
            counts[0] += 1;
        } else if packet.track_index == sound.index {
            let expected = i128::from(counts[1]) * i128::from(AUDIO_FRAME_SAMPLES)
                - i128::from(AUDIO_PRIMING_SAMPLES);
            let remaining = i128::from(native.audio_samples()) - expected;
            require(
                packet.track_id == sound.id
                    && u64::from(packet.sample_index) == counts[1]
                    && i128::from(timing.pts) == expected
                    && timing.dts == timing.pts
                    && remaining > 0
                    && i128::from(packet.duration)
                        == remaining.min(i128::from(AUDIO_FRAME_SAMPLES))
                    && packet.h264.is_none()
                    && packet.hevc.is_none(),
                "AAC packet clock or exact final duration differs",
            )?;
            counts[1] += 1;
        } else {
            return Err("unexpected packet stream".into());
        }
        completed += 1;
        require(
            completed <= total,
            "packet inventory exceeds captured extent",
        )?;
        if crate::render_worker::worker::progress_due(completed, total) {
            progress(VerificationProgress {
                stage: VerificationStage::Packets,
                completed,
                total,
            })?;
        }
    }
    require(
        counts == [u64::from(video.sample_count), u64::from(sound.sample_count)]
            && seen.iter().all(|&value| value)
            && keys.first() == Some(&true),
        "incomplete packet presentation inventory or opening IDR",
    )?;
    let reorder_delay = u32::try_from(reorder_delay).map_err(|_| "reorder delay overflow")?;
    Ok((keys, reorder_delay))
}
