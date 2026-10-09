use serde::{Deserialize, Serialize};

use crate::EncodeError;

pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const AUDIO_FRAME_SAMPLES: u32 = 1_024;
/// Silent encoder input before authored sample zero; never part of the mix.
pub const AUDIO_PREROLL_SAMPLES: u32 = AUDIO_FRAME_SAMPLES;
/// Codec delay plus explicit preroll, retained in the MP4 media edit.
pub const AUDIO_PRIMING_SAMPLES: u32 = AUDIO_FRAME_SAMPLES + AUDIO_PREROLL_SAMPLES;
pub const MAX_DIMENSION: u32 = 8_192;
pub const MAX_PIXELS: u64 = 33_554_432;
pub const MAX_VIDEO_FRAMES: u64 = 1_000_000;
pub const MAX_AUDIO_SAMPLES: u64 = 4_147_200_000;
pub const MAX_OUTPUT_BYTES: u64 = 64 * 1_024 * 1_024 * 1_024;
pub const MAX_PACKETS: u64 = 2_000_000;
pub const MAX_PACKET_BYTES: u64 = 32 * 1_024 * 1_024;
const MAX_CODEC_INTEGER: u32 = 2_147_483_647;

/// Frozen SDR control derivation used by `EncodeContract::new_v1`.
/// Persist this identity alongside resolved controls when retaining a decision.
pub const SDR_POLICY_VERSION_V1: u32 = 1;

/// Frozen HDR control derivation used by `EncodeContract::new_hdr_v1`: the SDR
/// v1 clocks, GOP, AAC target and timescale with video bitrate x1.25 rounded
/// half up from the exact SDR v1 integer. Persist it like the SDR identity.
pub const HDR_POLICY_VERSION_V1: u32 = 1;

/// Largest mastering-display chromaticity coordinate (1.0 in 1/50000 units).
pub const MAX_CHROMATICITY: u16 = 50_000;
/// SMPTE ST 2086 luminance bounds in 1/10000 cd/m² units (shared with
/// `deadpan_core`, which owns the rule set).
pub const MIN_MASTERING_MAX_LUMINANCE: u32 = deadpan_core::MASTERING_MIN_PEAK;
pub const MAX_MASTERING_MAX_LUMINANCE: u32 = deadpan_core::MASTERING_MAX_PEAK;
pub const MAX_MASTERING_MIN_LUMINANCE: u32 = deadpan_core::MASTERING_MAX_BLACK;
/// CTA-861.3 content light bound in cd/m².
pub const MAX_CONTENT_LIGHT: u16 = deadpan_core::CONTENT_LIGHT_MAX;

/// HDR transfer of the encoded signal. Primaries are BT.2020, the matrix is
/// BT.2020 non-constant luminance and the range is limited for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum HdrTransfer {
    /// SMPTE ST 2084.
    Pq,
    /// ARIB STD-B67 (BT.2100 HLG).
    Hlg,
}

/// SMPTE ST 2086 static mastering display volume. Primaries are R, G, B order;
/// chromaticities use 1/50000 units and luminances 1/10000 cd/m² units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasteringDisplay {
    pub primaries: [[u16; 2]; 3],
    pub white_point: [u16; 2],
    pub max_luminance: u32,
    pub min_luminance: u32,
}

impl MasteringDisplay {
    /// Applies the shared `deadpan_core::MasteringDisplay::check` rule set, so
    /// source admission, render contracts and the encoder agree exactly.
    pub fn validate(&self) -> Result<(), EncodeError> {
        let shared = deadpan_core::MasteringDisplay {
            primaries: self.primaries,
            white_point: self.white_point,
            max_luminance: self.max_luminance,
            min_luminance: self.min_luminance,
        };
        shared.check().map_err(|error| {
            EncodeError::Configuration(match error {
                deadpan_core::MasteringDisplayError::Chromaticity => {
                    "mastering chromaticity must be a positive CIE xy point"
                }
                deadpan_core::MasteringDisplayError::Primaries => {
                    "mastering primaries must enclose the white point in R,G,B order"
                }
                deadpan_core::MasteringDisplayError::Luminance => {
                    "mastering luminance exceeds ST 2086 bounds or min is not below max"
                }
            })
        })
    }
}

/// CTA-861.3 static content light levels in cd/m². Zero means unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentLight {
    pub max_cll: u16,
    pub max_fall: u16,
}

impl ContentLight {
    pub fn validate(&self) -> Result<(), EncodeError> {
        let shared = deadpan_core::ContentLight {
            max_cll: self.max_cll,
            max_fall: self.max_fall,
        };
        if !shared.is_valid() {
            return Err(EncodeError::Configuration(
                "content light exceeds 10000 cd/m² or MaxFALL exceeds MaxCLL",
            ));
        }
        Ok(())
    }
}

/// Requested HDR signal. HLG carries no static metadata in this policy, so
/// `mastering` must be absent for HLG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HdrSignal {
    pub transfer: HdrTransfer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mastering: Option<MasteringDisplay>,
}

impl HdrSignal {
    pub fn validate(&self) -> Result<(), EncodeError> {
        match (self.transfer, &self.mastering) {
            (HdrTransfer::Hlg, Some(_)) => Err(EncodeError::Configuration(
                "HLG output carries no mastering display metadata",
            )),
            (_, Some(mastering)) => mastering.validate(),
            (_, None) => Ok(()),
        }
    }
}

/// Encoded video stream format selected by the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum VideoFormat {
    /// H.264 High, 8-bit limited-range Rec.709 I420, `avc1`.
    H264Rec709I420,
    /// HEVC Main10, 10-bit limited-range BT.2020 NCL PQ, `hvc1`.
    HevcMain10Rec2100Pq,
    /// HEVC Main10, 10-bit limited-range BT.2020 NCL HLG, `hvc1`.
    HevcMain10Rec2100Hlg,
}

impl VideoFormat {
    pub const fn is_hdr(self) -> bool {
        !matches!(self, Self::H264Rec709I420)
    }
    pub const fn hdr_transfer(self) -> Option<HdrTransfer> {
        match self {
            Self::H264Rec709I420 => None,
            Self::HevcMain10Rec2100Pq => Some(HdrTransfer::Pq),
            Self::HevcMain10Rec2100Hlg => Some(HdrTransfer::Hlg),
        }
    }
}

/// The HDR part of a contract, serialized only for HDR contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HdrEncoding {
    pub policy_version: u32,
    pub signal: HdrSignal,
}

/// A separately admitted attempt. Neither mode permits an automatic fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum EncoderMode {
    Hardware,
    Software,
}

/// Requested policy, not evidence that the emitted stream obeyed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BFramePolicy {
    TargetTwo,
    /// An explicit diagnostic or independently qualified fallback attempt.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SdrPolicy {
    pub video_bitrate: u64,
    pub audio_bitrate: u64,
    pub gop_frames: u32,
    pub b_frames: u32,
    pub movie_timescale: u32,
}

/// Immutable exact input contract. SDR pixel input is tight limited-range
/// Rec.709 I420 with left-sited chroma, progressive square pixels. HDR input is
/// the same geometry as planar 10-bit little-endian u16 BT.2020 NCL samples. Audio is already
/// mastered finite 48 kHz planar stereo, with output sample zero as its origin.
///
/// `audio_samples` is supplied as B(project_end)-B(project_start). This adapter
/// cannot reconstruct those absolute project boundaries and never inserts an
/// offset or substitutes B(duration). The count must be within one sample of
/// origin-zero rounded duration, allowing the legitimate origin phase change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EncodeContract {
    raster: [u32; 2],
    frame_rate: [u32; 2],
    video_frames: u64,
    audio_samples: u64,
    mode: EncoderMode,
    policy: SdrPolicy,
    picture_bytes: u64,
    /// Absent for SDR so the frozen v1 serialization is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    hdr: Option<HdrEncoding>,
}

impl EncodeContract {
    /// Construct using the original SDR policy. This entry point remains an
    /// alias of `new_v1`; a later policy must have a separate constructor.
    pub fn new(
        raster: [u32; 2],
        frame_rate: [u32; 2],
        video_frames: u64,
        audio_samples: u64,
        mode: EncoderMode,
        b_frames: BFramePolicy,
    ) -> Result<Self, EncodeError> {
        Self::new_v1(
            raster,
            frame_rate,
            video_frames,
            audio_samples,
            mode,
            b_frames,
        )
    }

    /// Construct using frozen SDR policy version 1. Its bitrate interpolation,
    /// rate boundary, GOP rounding, AAC target and exact movie timescale are
    /// part of retained encoding decisions and must not follow future defaults.
    /// This validates inputs anew; it does not restore native admission from a
    /// serialized contract or permit caller-selected rate controls.
    pub fn new_v1(
        raster: [u32; 2],
        frame_rate: [u32; 2],
        video_frames: u64,
        audio_samples: u64,
        mode: EncoderMode,
        b_frames: BFramePolicy,
    ) -> Result<Self, EncodeError> {
        let [width, height] = raster;
        let pixels = u64::from(width) * u64::from(height);
        if width < 2
            || height < 2
            || width > MAX_DIMENSION
            || height > MAX_DIMENSION
            || !width.is_multiple_of(2)
            || !height.is_multiple_of(2)
            || pixels > MAX_PIXELS
        {
            return Err(EncodeError::Configuration(
                "I420 raster exceeds even-axis or pixel bounds",
            ));
        }
        let [numerator, denominator] = frame_rate;
        if numerator == 0
            || denominator == 0
            || numerator > MAX_CODEC_INTEGER
            || denominator > MAX_CODEC_INTEGER
            || gcd(numerator, denominator) != 1
            || u64::from(numerator) > 60 * u64::from(denominator)
        {
            return Err(EncodeError::Configuration(
                "frame rate must be reduced, positive and at most 60 fps",
            ));
        }
        if !(1..=MAX_VIDEO_FRAMES).contains(&video_frames)
            || !(1..=MAX_AUDIO_SAMPLES).contains(&audio_samples)
        {
            return Err(EncodeError::Configuration(
                "input frame or sample count exceeds hard bounds",
            ));
        }
        let terminal_ticks = u128::from(video_frames) * u128::from(denominator);
        if terminal_ticks > 86_400 * u128::from(numerator)
            || terminal_ticks > u128::from(i64::MAX.unsigned_abs())
        {
            return Err(EncodeError::Configuration(
                "video duration exceeds 24 hours or signed timestamps",
            ));
        }
        let sample_numerator = terminal_ticks * u128::from(AUDIO_SAMPLE_RATE);
        let rounded_samples = round_even(sample_numerator, u128::from(numerator));
        if rounded_samples.abs_diff(u128::from(audio_samples)) > 1 {
            return Err(EncodeError::Configuration(
                "audio sample count does not match the exact video interval",
            ));
        }
        let movie_timescale =
            u64::from(numerator / gcd(numerator, AUDIO_SAMPLE_RATE)) * u64::from(AUDIO_SAMPLE_RATE);
        let movie_timescale = u32::try_from(movie_timescale)
            .ok()
            .filter(|value| *value <= MAX_CODEC_INTEGER)
            .ok_or(EncodeError::Configuration(
                "exact movie timescale exceeds signed field range",
            ))?;
        let gop_denominator = 2 * u64::from(denominator);
        let gop_frames = u64::from(numerator).div_euclid(gop_denominator)
            + u64::from(u64::from(numerator) % gop_denominator * 2 >= gop_denominator);
        let policy = SdrPolicy {
            video_bitrate: bitrate_v1(pixels, u64::from(numerator) > 30 * u64::from(denominator)),
            audio_bitrate: 384_000,
            gop_frames: u32::try_from(gop_frames.max(1))
                .expect("admitted frame rate bounds the GOP"),
            b_frames: match b_frames {
                BFramePolicy::TargetTwo => 2,
                BFramePolicy::None => 0,
            },
            movie_timescale,
        };
        Ok(Self {
            raster,
            frame_rate,
            video_frames,
            audio_samples,
            mode,
            policy,
            picture_bytes: pixels * 3 / 2,
            hdr: None,
        })
    }

    /// Construct an HDR HEVC Main10 contract using frozen HDR policy version 1.
    /// Clocks, GOP, AAC, timescale, B-frame request and admission are exactly
    /// SDR v1; only the bitrate is SDR v1 x1.25 (half up). Input pictures are
    /// tight planar 10-bit 4:2:0 little-endian u16 samples (Y, Cb, Cr).
    pub fn new_hdr_v1(
        raster: [u32; 2],
        frame_rate: [u32; 2],
        video_frames: u64,
        audio_samples: u64,
        mode: EncoderMode,
        b_frames: BFramePolicy,
        signal: HdrSignal,
    ) -> Result<Self, EncodeError> {
        signal.validate()?;
        let mut contract = Self::new_v1(
            raster,
            frame_rate,
            video_frames,
            audio_samples,
            mode,
            b_frames,
        )?;
        contract.policy.video_bitrate = (contract.policy.video_bitrate * 5 + 2) / 4;
        contract.picture_bytes *= 2;
        contract.hdr = Some(HdrEncoding {
            policy_version: HDR_POLICY_VERSION_V1,
            signal,
        });
        Ok(contract)
    }

    pub const fn video_format(&self) -> VideoFormat {
        match &self.hdr {
            None => VideoFormat::H264Rec709I420,
            Some(HdrEncoding {
                signal:
                    HdrSignal {
                        transfer: HdrTransfer::Pq,
                        ..
                    },
                ..
            }) => VideoFormat::HevcMain10Rec2100Pq,
            Some(_) => VideoFormat::HevcMain10Rec2100Hlg,
        }
    }
    pub const fn hdr(&self) -> Option<&HdrEncoding> {
        self.hdr.as_ref()
    }

    pub const fn raster(&self) -> [u32; 2] {
        self.raster
    }
    pub const fn frame_rate(&self) -> [u32; 2] {
        self.frame_rate
    }
    pub const fn video_frames(&self) -> u64 {
        self.video_frames
    }
    pub const fn audio_samples(&self) -> u64 {
        self.audio_samples
    }
    pub const fn mode(&self) -> EncoderMode {
        self.mode
    }
    pub const fn policy(&self) -> &SdrPolicy {
        &self.policy
    }
    pub const fn picture_bytes(&self) -> u64 {
        self.picture_bytes
    }

    pub fn picture_timing(&self, ordinal: u64) -> Result<(i64, i64), EncodeError> {
        if ordinal >= self.video_frames {
            return Err(EncodeError::Input(
                "picture ordinal is outside the captured interval",
            ));
        }
        let duration = i64::from(self.frame_rate[1]);
        let pts = i64::try_from(ordinal)
            .ok()
            .and_then(|value| value.checked_mul(duration))
            .ok_or(EncodeError::Input("picture timestamp overflow"))?;
        Ok((pts, duration))
    }
}

/// Independent hard limits for file extent, encoded packet count and one packet.
/// A low bound is useful for deliberate failure qualification, not a quality setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodeLimits {
    pub maximum_output_bytes: u64,
    pub maximum_packets: u64,
    pub maximum_packet_bytes: u64,
}

impl Default for EncodeLimits {
    fn default() -> Self {
        Self {
            maximum_output_bytes: MAX_OUTPUT_BYTES,
            maximum_packets: MAX_PACKETS,
            maximum_packet_bytes: MAX_PACKET_BYTES,
        }
    }
}

impl EncodeLimits {
    pub fn validate(self) -> Result<(), EncodeError> {
        if !(1..=MAX_OUTPUT_BYTES).contains(&self.maximum_output_bytes)
            || !(1..=MAX_PACKETS).contains(&self.maximum_packets)
            || !(1..=MAX_PACKET_BYTES).contains(&self.maximum_packet_bytes)
        {
            return Err(EncodeError::Configuration(
                "encoder limits exceed native hard bounds",
            ));
        }
        Ok(())
    }

    pub fn validate_for(self, contract: &EncodeContract) -> Result<(), EncodeError> {
        self.validate()?;
        let required_packets = contract.video_frames()
            + contract
                .audio_samples()
                .div_ceil(u64::from(AUDIO_FRAME_SAMPLES))
            + 2;
        if required_packets > self.maximum_packets {
            return Err(EncodeError::Configuration(
                "packet budget cannot retain the captured interval",
            ));
        }
        Ok(())
    }
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn round_even(numerator: u128, denominator: u128) -> u128 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    quotient
        + u128::from(
            remainder * 2 > denominator
                || remainder * 2 == denominator && !quotient.is_multiple_of(2),
        )
}

fn bitrate_v1(pixels: u64, high_rate: bool) -> u64 {
    // Nominal 16:9 class areas are represented exactly as height²*16/9,
    // including the nonintegral nominal 480p width. Actual raster area drives
    // interpolation, so portrait, square and ultrawide pictures follow area.
    const CLASSES: [(u64, u64, u64); 7] = [
        (360, 1_500_000, 2_000_000),
        (480, 3_000_000, 4_000_000),
        (720, 5_000_000, 7_500_000),
        (1080, 8_000_000, 12_000_000),
        (1440, 16_000_000, 24_000_000),
        (2160, 45_000_000, 68_000_000),
        (4320, 160_000_000, 240_000_000),
    ];
    let choose = |entry: (u64, u64, u64)| if high_rate { entry.2 } else { entry.1 };
    let area = pixels * 9;
    if area <= CLASSES[0].0 * CLASSES[0].0 * 16 {
        return choose(CLASSES[0]);
    }
    for pair in CLASSES.windows(2) {
        let [lower, upper] = [pair[0], pair[1]];
        let low_area = lower.0 * lower.0 * 16;
        let high_area = upper.0 * upper.0 * 16;
        if area <= high_area {
            let fraction = u128::from(area - low_area) * u128::from(choose(upper) - choose(lower));
            let divisor = u128::from(high_area - low_area);
            let increment = (fraction + divisor / 2) / divisor;
            return choose(lower)
                + u64::try_from(increment).expect("class interpolation is bounded");
        }
    }
    choose(CLASSES[CLASSES.len() - 1])
}
