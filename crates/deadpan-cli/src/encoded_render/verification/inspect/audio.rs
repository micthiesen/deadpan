use deadpan_encode::{AUDIO_FRAME_SAMPLES, AUDIO_PRIMING_SAMPLES};
use deadpan_source::{
    Mp4TrackInspection,
    audio::{
        AudioChannelLayout, AudioDecodeLimits, AudioDecodeMode, AudioDecoder, AudioSampleFormat,
    },
};
use sha2::{Digest, Sha256};

use super::{Context, Result, require};
use crate::encoded_render::verification::{VerificationProgress, VerificationStage};

pub(super) struct AudioResult {
    pub first_sample: i64,
    pub physical_samples: u64,
    pub presented_digest: [u8; 32],
}

pub(super) fn inspect(
    context: &Context<'_>,
    track: &Mp4TrackInspection,
    mode: AudioDecodeMode,
    progress: &mut impl FnMut(VerificationProgress) -> Result<()>,
) -> Result<AudioResult> {
    let native = context.manifest.contract.native_contract()?;
    let authored = i64::try_from(native.audio_samples()).map_err(|_| "audio endpoint overflow")?;
    let limits = AudioDecodeLimits {
        max_input_bytes: context.limits.max_input_bytes,
        max_frames: u64::from(track.sample_count) + 1,
        max_packets: context.limits.max_packets,
        max_decoded_samples: u64::from(track.sample_count) * u64::from(AUDIO_FRAME_SAMPLES),
        // FFmpeg's AAC decoder allocates a 2048-sample internal frame before
        // returning the 1024-sample LC block checked below.
        max_samples_per_frame: 2048,
        max_channels: 2,
        max_sample_rate: 48_000,
        ..AudioDecodeLimits::default()
    };
    let mut decoder = AudioDecoder::open_with_mode(
        context.file()?,
        track.index,
        mode,
        limits,
        context.control()?,
    )
    .map_err(|e| e.to_string())?;
    let info = decoder.info();
    let stereo = AudioChannelLayout::Native {
        channels: 2,
        mask: 3,
    };
    require(
        info.stream_index == track.index
            && info.codec == "aac"
            && info.time_base_num == 1
            && info.time_base_den == 48_000
            && info.sample_rate == 48_000
            && info.channel_layout == stereo
            && info.sample_format == AudioSampleFormat::Float32Planar
            && info.stream_start == Some(0)
            && info.stream_duration == Some(authored),
        "AAC stream interpretation or presented range differs",
    )?;
    let first = if mode == AudioDecodeMode::Manual {
        -i64::from(AUDIO_PRIMING_SAMPLES)
    } else {
        0
    };
    let mut expected = first;
    let mut physical = 0_u64;
    let mut presented = 0_i64;
    let mut digest = Sha256::new();
    let progress_step = native.audio_samples().div_ceil(32);
    let mut next_progress = 0;
    let stage = if mode == AudioDecodeMode::Manual {
        VerificationStage::ManualAudio
    } else {
        VerificationStage::OrdinaryAudio
    };
    while let Some(metadata) = decoder
        .next_metadata(context.control()?)
        .map_err(|e| e.to_string())?
    {
        let remainder = authored
            .checked_sub(expected)
            .ok_or("audio remainder overflow")?;
        require(
            remainder > 0
                && metadata.pts == expected
                && metadata.nb_samples == AUDIO_FRAME_SAMPLES
                && metadata.sample_rate == 48_000
                && metadata.channel_layout == stereo
                && metadata.sample_format == AudioSampleFormat::Float32Planar
                && metadata.reported_duration
                    == Some(remainder.min(i64::from(AUDIO_FRAME_SAMPLES)))
                && metadata.decode_timestamp.is_none_or(|dts| dts == expected),
            "decoded AAC clock, sample count or duration differs",
        )?;
        let evidence = decoder.evidence();
        require(
            evidence.decoder_name == "aac"
                && evidence.decoder_profile == Some(1)
                && evidence
                    .container_profile
                    .is_none_or(|profile| profile == 1)
                && evidence
                    .container_format
                    .split(',')
                    .any(|name| name == "mp4"),
            "decoded audio is not the qualified AAC-LC MP4 path",
        )?;
        if mode == AudioDecodeMode::Manual && expected == first {
            require(
                metadata.discard
                    && metadata.skip_samples.is_some_and(|skip| {
                        skip.leading == AUDIO_PRIMING_SAMPLES
                            && skip.trailing == 0
                            && skip.leading_reason == 0
                            && skip.trailing_reason == 0
                    }),
                "AAC opening decode does not explain the measured priming edit",
            )?;
        } else if mode == AudioDecodeMode::Manual && expected < 0 {
            // The first returned frame declares the complete edit. FFmpeg
            // retains the second physical preroll frame as discarded without
            // repeating that declaration; both must precede authored zero.
            require(
                metadata.discard && metadata.skip_samples.is_none(),
                "AAC preroll decode does not retain the measured discarded block",
            )?;
        } else {
            require(
                !metadata.discard
                    && metadata
                        .skip_samples
                        .is_none_or(|skip| skip.leading == 0 && skip.trailing == 0),
                "unexpected AAC skip or discard inside authored audio",
            )?;
        }
        let frame = decoder
            .copy_current_interleaved_f32(context.control()?)
            .map_err(|e| e.to_string())?;
        require(
            frame.metadata == metadata
                && frame.samples.len() == 2048
                && frame.samples.iter().all(|sample| sample.is_finite()),
            "AAC output changed metadata, length or finite PCM",
        )?;
        let end = expected
            .checked_add(i64::from(AUDIO_FRAME_SAMPLES))
            .ok_or("audio clock overflow")?;
        let low = expected.max(0);
        let high = end.min(authored);
        if high > low {
            require(low == presented, "AAC presentation has a gap or overlap")?;
            for sample in low..high {
                let offset = usize::try_from(sample - expected)
                    .map_err(|_| "AAC sample offset overflow")?
                    * 2;
                digest.update(frame.samples[offset].to_bits().to_le_bytes());
                digest.update(frame.samples[offset + 1].to_bits().to_le_bytes());
            }
            presented = high;
            let completed = u64::try_from(presented).map_err(|_| "audio progress overflow")?;
            if completed >= next_progress || completed == native.audio_samples() {
                progress(VerificationProgress {
                    stage,
                    completed,
                    total: native.audio_samples(),
                })?;
                next_progress = completed.saturating_add(progress_step);
            }
        }
        physical += u64::from(AUDIO_FRAME_SAMPLES);
        expected = end;
        require(
            physical <= u64::from(track.sample_count) * u64::from(AUDIO_FRAME_SAMPLES),
            "AAC physical decode exceeds packet budget",
        )?;
    }
    let expected_physical = native
        .audio_samples()
        .div_ceil(u64::from(AUDIO_FRAME_SAMPLES))
        * u64::from(AUDIO_FRAME_SAMPLES)
        + if mode == AudioDecodeMode::Manual {
            u64::from(AUDIO_PRIMING_SAMPLES)
        } else {
            0
        };
    require(
        physical == expected_physical && presented == authored,
        "AAC decode omits the authored range or codec drain",
    )?;
    Ok(AudioResult {
        first_sample: first,
        physical_samples: physical,
        presented_digest: digest.finalize().into(),
    })
}
