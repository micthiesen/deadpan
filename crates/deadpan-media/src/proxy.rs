//! Preview proxies: a rebuildable, intra-only, reduced-raster picture copy of
//! one Original video stream, for interactive seeking only.
//!
//! A proxy is never authoritative. Its pictures carry exactly the Original's
//! measured presentation timestamps and durations, verified independently
//! after encoding, so proxy ordinal `k` is Original frame `k`. The pixels are
//! a scaled, lossy copy: limited-range BT.709 4:2:0 H.264 tagged with the
//! Original's transfer and primaries, compared with the Original on sampled
//! pictures within [`FIDELITY_MEAN_LIMIT`], [`FIDELITY_BLOCK_LIMIT`] and
//! [`FIDELITY_BIAS_LIMIT`].
//!
//! This module defines the worker wire contract, the eligibility and raster
//! policy, the sidecar index and independent verification. It reads no
//! project, publishes nothing and serves no pictures: the only proxy picture
//! reader is the native app's private preview worker module.

use std::fs::File;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{AssetId, SourceFrameId, SourceFrameIndex};
use deadpan_source::{
    ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, DecodedRgbaFrame, SourceStreamInfo,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::protocol::ContractError;
use crate::source_index::{SourceContentIdentity, SourceIndexError, SourceIndexSnapshot};
use crate::source_input::{SourceInputError, VerifiedSourceInput};
use crate::source_qualification::SourceStreamInfoWire;
use crate::source_session::{SourceSession, SourceSessionError, SourceSessionLimits};

/// First worker argument selecting proxy encoding.
pub const PROXY_ARGUMENT: &str = "proxy";
pub const PROXY_PROTOCOL_VERSION: u32 = 1;
/// Raster rule, codec, quality and color handling. A change needs a new
/// version: cached proxies of other versions are never served.
pub const PROXY_RECIPE_VERSION: u32 = 1;
/// Schema 2: sixteen fidelity samples with per-channel signed bias.
pub const PROXY_SIDECAR_SCHEMA: u32 = 2;
/// The proxy fits inside this box in the picture's own orientation.
pub const PROXY_MAX_LONG_SIDE: u32 = 1920;
pub const PROXY_MAX_SHORT_SIDE: u32 = 1080;
/// Originals with more pixels than this get a proxy. Specification §16.4
/// asks for proxies only where they materially improve seeking. Measured on
/// 2026-10-05, threaded 1080p long-GOP seeks already meet the 80 ms target
/// (48 ms p95 with keyframes 250 pictures apart), while 4K misses it (211 ms),
/// so long keyframe spacing alone no longer qualifies.
pub const PROXY_RASTER_THRESHOLD: u64 = 1920 * 1080;
/// VideoToolbox constant quality (0–100) of the intra pictures.
pub const PROXY_QUALITY: u32 = 60;
pub const PROXY_ENCODER: &str = "h264_videotoolbox intra";
/// Hard bound of one proxy file.
pub const MAX_PROXY_BYTES: u64 = 32 * 1024 * 1024 * 1024;
/// Output cap per proxy pixel and picture, in bytes: half the size of raw
/// 4:2:0. Measured recipe-1 intra pictures use about 0.02 B/px.
const MAX_BYTES_PER_PIXEL_NUM: u64 = 3;
const MAX_BYTES_PER_PIXEL_DEN: u64 = 4;
/// Planning estimate per proxy pixel and picture (1/8 B/px), six times the
/// measured rate, for budget and free-space checks.
const ESTIMATED_BYTES_PER_PIXEL_DEN: u64 = 8;
const MAX_PROXY_TIMEOUT_MS: u64 = 24 * 60 * 60 * 1000;
const MAX_PROXY_FRAMES: u64 = 10_000_000;
/// Mean absolute difference over sampled block averages, per 8-bit channel.
pub const FIDELITY_MEAN_LIMIT: f64 = 3.0;
/// Largest single block-average difference, per 8-bit channel.
pub const FIDELITY_BLOCK_LIMIT: f64 = 16.0;
/// Largest mean signed difference of one RGB channel over every sample: a
/// systematic tint or level shift that averages out of absolute errors.
pub const FIDELITY_BIAS_LIMIT: f64 = 1.5;
const FIDELITY_COLUMNS: u32 = 16;
const FIDELITY_ROWS: u32 = 9;
const FIDELITY_SAMPLES: u64 = 16;
const SEEK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyTransfer {
    Bt709,
    Srgb,
    Linear,
}

impl ProxyTransfer {
    pub fn of(transfer: ColorTransfer) -> Self {
        match transfer {
            ColorTransfer::Bt709 => Self::Bt709,
            ColorTransfer::Srgb => Self::Srgb,
            ColorTransfer::Linear => Self::Linear,
        }
    }
    /// The FFmpeg `AVColorTransferCharacteristic` code.
    pub const fn code(self) -> u32 {
        match self {
            Self::Bt709 => 1,
            Self::Srgb => 13,
            Self::Linear => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyPrimaries {
    Bt709,
    Bt2020,
    DisplayP3,
}

impl ProxyPrimaries {
    pub fn of(primaries: ColorPrimaries) -> Self {
        match primaries {
            ColorPrimaries::Bt709 => Self::Bt709,
            ColorPrimaries::Bt2020 => Self::Bt2020,
            ColorPrimaries::DisplayP3 => Self::DisplayP3,
        }
    }
    /// The FFmpeg `AVColorPrimaries` code.
    pub const fn code(self) -> u32 {
        match self {
            Self::Bt709 => 1,
            Self::Bt2020 => 9,
            Self::DisplayP3 => 12,
        }
    }
}

/// Decode the Original on stdin, encode the proxy to stdout. The worker
/// decodes through the qualified source adapter and receives no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyRequest {
    pub protocol: u32,
    pub input_byte_length: u64,
    pub stream_index: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub width: u32,
    pub height: u32,
    pub time_base_num: u32,
    pub time_base_den: u32,
    pub sar_num: u32,
    pub sar_den: u32,
    pub rotation_quarter_turns: u8,
    pub transfer: ProxyTransfer,
    pub primaries: ProxyPrimaries,
    pub quality: u32,
    pub frames: u64,
    pub decode_threads: u32,
    pub max_output_bytes: u64,
    pub timeout_ms: u64,
}

impl ProxyRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.protocol != PROXY_PROTOCOL_VERSION {
            return Err(ContractError("unsupported proxy protocol"));
        }
        if self.input_byte_length == 0 || self.input_byte_length > 64 * 1024 * 1024 * 1024 {
            return Err(ContractError("proxy input length is out of range"));
        }
        if self.stream_index >= 33
            || self.source_width == 0
            || self.source_height == 0
            || self.source_width > 8192
            || self.source_height > 8192
            || self.width < 2
            || self.height < 2
            || self.width > 4096
            || self.height > 4096
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || self.width > self.source_width + 1
            || self.height > self.source_height + 1
        {
            return Err(ContractError("proxy raster is out of range"));
        }
        if self.time_base_num == 0
            || self.time_base_den == 0
            || self.time_base_num > i32::MAX as u32
            || self.time_base_den > i32::MAX as u32
            || self.sar_num == 0
            || self.sar_den == 0
            || self.sar_num > i32::MAX as u32
            || self.sar_den > i32::MAX as u32
            || self.rotation_quarter_turns > 3
        {
            return Err(ContractError("proxy clock, aspect or rotation is invalid"));
        }
        if !(1..=100).contains(&self.quality)
            || !(1..=MAX_PROXY_FRAMES).contains(&self.frames)
            || !(1..=16).contains(&self.decode_threads)
            || !(1..=MAX_PROXY_BYTES).contains(&self.max_output_bytes)
            || !(1..=MAX_PROXY_TIMEOUT_MS).contains(&self.timeout_ms)
        {
            return Err(ContractError(
                "proxy quality, frame, byte or time budget is invalid",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyReport {
    pub protocol: u32,
    pub output_bytes: u64,
    pub frames: u64,
    pub packets: u64,
    pub keyframes: u64,
    pub width: u32,
    pub height: u32,
}

impl ProxyReport {
    pub fn validate_for(&self, request: &ProxyRequest) -> Result<(), ContractError> {
        if self.protocol != PROXY_PROTOCOL_VERSION
            || self.output_bytes == 0
            || self.output_bytes > request.max_output_bytes
            || self.frames != request.frames
            || self.packets != request.frames
            || self.keyframes != request.frames
            || self.width != request.width
            || self.height != request.height
        {
            return Err(ContractError("proxy report violates its contract"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProxyReply {
    Success { report: ProxyReport },
    Failure { code: String, message: String },
}

/// Why an Original gets a proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyReason {
    /// Policy: the Original exceeds [`PROXY_RASTER_THRESHOLD`].
    Raster,
    /// An explicit plan from a test or benchmark.
    Requested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyPlan {
    pub width: u32,
    pub height: u32,
    pub reason: ProxyReason,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ProxyIneligible {
    #[error("the Original has no indexed pictures")]
    Empty,
    #[error("picture durations differ from presentation intervals, which MP4 cannot reproduce")]
    IrregularDurations,
    #[error("the Original's last picture has no measured duration")]
    MissingDuration,
}

/// Decide whether an Original needs a proxy and at which raster. `Ok(None)`
/// means the Original already seeks fast enough.
pub fn proxy_plan(
    info: &SourceStreamInfo,
    index: &SourceFrameIndex,
) -> Result<Option<ProxyPlan>, ProxyIneligible> {
    let frames = index.frames();
    if frames.is_empty() {
        return Err(ProxyIneligible::Empty);
    }
    let pixels = u64::from(info.width) * u64::from(info.height);
    if pixels <= PROXY_RASTER_THRESHOLD {
        return Ok(None);
    }
    expressible(index)?;
    let (width, height) = proxy_raster(info.width, info.height);
    Ok(Some(ProxyPlan {
        width,
        height,
        reason: ProxyReason::Raster,
    }))
}

/// MP4 stores each sample's duration as the distance to the next decode
/// timestamp. Without reordering that is the presentation interval, so a
/// proxy reproduces the Original's durations only where they equal it.
pub fn expressible(index: &SourceFrameIndex) -> Result<(), ProxyIneligible> {
    let frames = index.frames();
    if frames.is_empty() {
        return Err(ProxyIneligible::Empty);
    }
    for pair in frames.windows(2) {
        if pair[0].reported_duration != pair[1].pts.checked_sub(pair[0].pts) {
            return Err(ProxyIneligible::IrregularDurations);
        }
    }
    if frames
        .last()
        .and_then(|frame| frame.reported_duration)
        .is_none_or(|duration| duration <= 0)
    {
        return Err(ProxyIneligible::MissingDuration);
    }
    Ok(())
}

/// Codec threads of the worker's sequential Original decode. Proxy building is
/// background work; this bounds its share of the machine.
pub const PROXY_DECODE_THREADS: u32 = 4;

/// Planning size of a proxy, for budget and free-space checks.
pub fn estimated_proxy_bytes(plan: &ProxyPlan, frames: u64) -> u64 {
    (u64::from(plan.width) * u64::from(plan.height) / ESTIMATED_BYTES_PER_PIXEL_DEN)
        .saturating_mul(frames)
        .saturating_add(1024 * 1024)
}

/// The worker request for `plan`. The output bound is three quarters of a
/// byte per proxy pixel and picture plus 16 MiB, capped at [`MAX_PROXY_BYTES`].
pub fn proxy_request(
    input_byte_length: u64,
    info: &SourceStreamInfo,
    frames: u64,
    plan: &ProxyPlan,
    timeout: Duration,
) -> ProxyRequest {
    let per_picture = u64::from(plan.width) * u64::from(plan.height) * MAX_BYTES_PER_PIXEL_NUM
        / MAX_BYTES_PER_PIXEL_DEN;
    ProxyRequest {
        protocol: PROXY_PROTOCOL_VERSION,
        input_byte_length,
        stream_index: info.stream_index,
        source_width: info.width,
        source_height: info.height,
        width: plan.width,
        height: plan.height,
        time_base_num: info.time_base_num,
        time_base_den: info.time_base_den,
        sar_num: info.sample_aspect_num,
        sar_den: info.sample_aspect_den,
        rotation_quarter_turns: info.rotation_quarter_turns,
        transfer: ProxyTransfer::of(info.color.transfer),
        primaries: ProxyPrimaries::of(info.color.primaries),
        quality: PROXY_QUALITY,
        frames,
        decode_threads: PROXY_DECODE_THREADS,
        max_output_bytes: per_picture
            .saturating_mul(frames)
            .saturating_add(16 * 1024 * 1024)
            .min(MAX_PROXY_BYTES),
        timeout_ms: u64::try_from(timeout.as_millis())
            .unwrap_or(MAX_PROXY_TIMEOUT_MS)
            .clamp(1, MAX_PROXY_TIMEOUT_MS),
    }
}

/// Uniform scale fitting the box in the picture's orientation, never
/// upscaling, rounded to even dimensions of at least two. The sample aspect
/// ratio is unchanged, so the display aspect is kept up to that rounding.
pub fn proxy_raster(width: u32, height: u32) -> (u32, u32) {
    let (long, short) = (width.max(height), width.min(height));
    // scale = min(1, MAX_LONG / long, MAX_SHORT / short), as an exact ratio.
    let (num, den) = if u64::from(PROXY_MAX_LONG_SIDE) * u64::from(short)
        <= u64::from(PROXY_MAX_SHORT_SIDE) * u64::from(long)
    {
        (u64::from(PROXY_MAX_LONG_SIDE), u64::from(long))
    } else {
        (u64::from(PROXY_MAX_SHORT_SIDE), u64::from(short))
    };
    let (num, den) = if num >= den { (1, 1) } else { (num, den) };
    let even = |value: u32| {
        // Round to the nearest even integer, at least 2.
        let scaled = u64::from(value) * num;
        let halves = (scaled + den) / (2 * den);
        u32::try_from((halves * 2).max(2)).unwrap_or(u32::MAX)
    };
    (even(width), even(height))
}

/// The Original bytes and stream a proxy belongs to. BLAKE3 is the retained
/// object's content address; SHA-256 is the snapshot identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyOriginal {
    pub blake3: String,
    pub sha256: String,
    pub byte_length: u64,
    pub stream_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyFileIdentity {
    pub sha256: String,
    pub blake3: String,
    pub byte_length: u64,
}

/// Block-average comparison with the Original on sampled pictures, in
/// thousandths of an 8-bit code value so the record round-trips exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyFidelity {
    pub samples: Vec<u64>,
    pub mean_abs_difference_milli: u32,
    pub max_block_difference_milli: u32,
    /// Mean signed (proxy minus Original) difference of R, G and B.
    pub bias_milli: [i32; 3],
    /// The most colorful sample: ordinal and the standard deviations of its
    /// Original's block luma and chroma, showing the samples covered color.
    pub widest_sample: u64,
    pub widest_luma_spread_milli: u32,
    pub widest_chroma_spread_milli: u32,
}

impl ProxyFidelity {
    pub fn mean(&self) -> f64 {
        f64::from(self.mean_abs_difference_milli) / 1000.0
    }
    pub fn max_block(&self) -> f64 {
        f64::from(self.max_block_difference_milli) / 1000.0
    }
    pub fn bias(&self) -> [f64; 3] {
        self.bias_milli.map(|value| f64::from(value) / 1000.0)
    }
    fn within_limits(&self) -> bool {
        self.mean() <= FIDELITY_MEAN_LIMIT
            && self.max_block() <= FIDELITY_BLOCK_LIMIT
            && self
                .bias()
                .iter()
                .all(|bias| bias.abs() <= FIDELITY_BIAS_LIMIT)
    }
}

fn signed_milli(value: f64) -> i32 {
    // Bounded by ±255; round away from zero so limits stay strict.
    let scaled = value * 1000.0;
    (if scaled < 0.0 {
        scaled.floor()
    } else {
        scaled.ceil()
    })
    .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn milli(value: f64) -> u32 {
    // Values are bounded by 255 per channel; round up so limits stay strict.
    (value * 1000.0).ceil().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// The proxy's sidecar index. Written only after independent verification
/// and validated again before any picture is served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxySidecar {
    pub schema: u32,
    pub recipe: u32,
    pub encoder: String,
    pub reason: ProxyReason,
    pub original: ProxyOriginal,
    pub file: ProxyFileIdentity,
    /// The proxy's own freshly measured stream interpretation.
    #[serde(with = "SourceStreamInfoWire")]
    pub info: SourceStreamInfo,
    /// The proxy's own freshly measured index; entry `k` has Original frame
    /// `k`'s exact presentation time and duration.
    pub index: SourceIndexSnapshot,
    pub fidelity: ProxyFidelity,
}

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("proxy does not correspond to the Original: {0}")]
    Correspondence(&'static str),
    #[error(
        "proxy pictures differ from the Original beyond tolerance (mean {mean:.2}, block {block:.2}, bias {bias:?})"
    )]
    Fidelity {
        mean: f64,
        block: f64,
        bias: [f64; 3],
    },
    #[error("proxy sidecar is invalid: {0}")]
    Sidecar(&'static str),
    #[error("proxy verification was cancelled")]
    Cancelled,
    #[error("proxy verification exceeded its deadline")]
    Deadline,
    #[error(transparent)]
    Session(#[from] SourceSessionError),
    #[error(transparent)]
    Input(#[from] SourceInputError),
    #[error(transparent)]
    Index(#[from] SourceIndexError),
    #[error(transparent)]
    Document(#[from] deadpan_core::DocumentError),
    #[error("proxy I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("proxy JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl ProxyError {
    /// Deadline, cancellation or I/O rather than a verdict about the bytes.
    pub fn is_interruption(&self) -> bool {
        match self {
            Self::Cancelled | Self::Deadline | Self::Io(_) => true,
            Self::Session(error) => error.is_interruption(),
            Self::Input(SourceInputError::Snapshot(error)) => matches!(
                error,
                crate::ConversionError::Cancelled
                    | crate::ConversionError::Deadline
                    | crate::ConversionError::Io(_)
            ),
            _ => false,
        }
    }
}

fn same_time(a: i64, a_base: (u32, u32), b: i64, b_base: (u32, u32)) -> bool {
    i128::from(a) * i128::from(a_base.0) * i128::from(b_base.1)
        == i128::from(b) * i128::from(b_base.0) * i128::from(a_base.1)
}

impl ProxySidecar {
    /// Check that this sidecar describes a proxy of exactly `original`'s
    /// stream with `original_index`'s pictures, under the current recipe.
    /// Linear in the number of pictures; call it once per admission.
    pub fn validate_for(
        &self,
        original: &ProxyOriginal,
        original_index: &SourceFrameIndex,
        original_info: &SourceStreamInfo,
    ) -> Result<(), ProxyError> {
        if self.schema != PROXY_SIDECAR_SCHEMA
            || self.recipe != PROXY_RECIPE_VERSION
            || self.encoder != PROXY_ENCODER
        {
            return Err(ProxyError::Sidecar("schema or recipe is not current"));
        }
        if &self.original != original {
            return Err(ProxyError::Sidecar("it belongs to other Original bytes"));
        }
        let content = self.index.content();
        if hex(&content.sha256()) != self.file.sha256
            || content.byte_length() != self.file.byte_length
            || self.index.stream_index() != self.info.stream_index
        {
            return Err(ProxyError::Sidecar("index and file identities disagree"));
        }
        correspondence(&self.index, &self.info, original_index, original_info)?;
        if !self.fidelity.within_limits() {
            return Err(ProxyError::Sidecar("recorded fidelity exceeds tolerance"));
        }
        Ok(())
    }

    /// The proxy file's content identity for snapshot verification.
    pub fn content(&self) -> SourceContentIdentity {
        self.index.content()
    }

    pub fn to_json(&self) -> Result<Vec<u8>, ProxyError> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Bounded before parsing; the caller still calls [`Self::validate_for`].
    pub fn from_json(bytes: &[u8]) -> Result<Self, ProxyError> {
        if bytes.len() > crate::source_index::MAX_SOURCE_INDEX_JSON_BYTES + 64 * 1024 {
            return Err(ProxyError::Sidecar("sidecar exceeds its byte bound"));
        }
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// Every proxy picture has the same exact presentation time and duration as
/// the Original picture with the same ordinal, is a keyframe, and the stream
/// has the planned interpretation: limited BT.709 4:2:0 H.264 with the
/// Original's transfer, primaries, sample aspect and rotation.
fn correspondence(
    proxy: &SourceIndexSnapshot,
    proxy_info: &SourceStreamInfo,
    original: &SourceFrameIndex,
    original_info: &SourceStreamInfo,
) -> Result<(), ProxyError> {
    let wrong = ProxyError::Correspondence;
    if proxy_info.codec != "h264"
        || proxy_info.pixel_format != "yuv420p"
        || proxy_info.color.range != ColorRange::Limited
        || proxy_info.color.matrix != ColorMatrix::Bt709
        || proxy_info.color.transfer != original_info.color.transfer
        || proxy_info.color.primaries != original_info.color.primaries
        || proxy_info.rotation_quarter_turns != original_info.rotation_quarter_turns
        || u64::from(proxy_info.sample_aspect_num) * u64::from(original_info.sample_aspect_den)
            != u64::from(original_info.sample_aspect_num) * u64::from(proxy_info.sample_aspect_den)
    {
        return Err(wrong("stream interpretation differs from the plan"));
    }
    if (proxy_info.width, proxy_info.height)
        != proxy_raster(original_info.width, original_info.height)
    {
        return Err(wrong("raster differs from the plan"));
    }
    let index = proxy.index();
    let proxy_base = (
        index.time_base().numerator(),
        index.time_base().denominator(),
    );
    let original_base = (
        original.time_base().numerator(),
        original.time_base().denominator(),
    );
    if index.frames().len() != original.frames().len() {
        return Err(wrong("picture counts differ"));
    }
    for (proxy_frame, original_frame) in index.frames().iter().zip(original.frames()) {
        let duration = |frame: &deadpan_core::IndexedSourceFrame| frame.reported_duration;
        let same_duration = match (duration(proxy_frame), duration(original_frame)) {
            (Some(a), Some(b)) => same_time(a, proxy_base, b, original_base),
            _ => false,
        };
        if !proxy_frame.keyframe
            || !same_duration
            || !same_time(
                proxy_frame.pts,
                proxy_base,
                original_frame.pts,
                original_base,
            )
        {
            return Err(wrong("a picture's time or duration differs"));
        }
    }
    if !same_time(
        index.terminal_end(),
        proxy_base,
        original.terminal_end(),
        original_base,
    ) {
        return Err(wrong("terminal endpoints differ"));
    }
    Ok(())
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

struct Control<'a> {
    end: Instant,
    cancelled: &'a AtomicBool,
    pause: Option<&'a AtomicBool>,
}

impl Control<'_> {
    /// Fail on cancellation or deadline; wait while paused.
    fn check(&self) -> Result<(), ProxyError> {
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(ProxyError::Cancelled);
            }
            if Instant::now() >= self.end {
                return Err(ProxyError::Deadline);
            }
            if !self
                .pause
                .is_some_and(|pause| pause.load(Ordering::Acquire))
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fn remaining(&self) -> Result<Duration, ProxyError> {
        self.check()?;
        Ok(self.end.saturating_duration_since(Instant::now()))
    }
}

/// Supervision of [`verify_proxy`].
#[derive(Clone, Copy)]
pub struct VerifyControl<'a> {
    pub timeout: Duration,
    pub cancelled: &'a AtomicBool,
    /// While set, verification waits between steps. A step already started
    /// (the proxy's index measurement) runs to completion.
    pub pause: Option<&'a AtomicBool>,
}

/// SHA-256 and BLAKE3 of a whole file read positionally, checking `control`
/// between chunks. The length must not change while hashing.
fn hash_file(file: &File, control: &Control<'_>) -> Result<([u8; 32], String, u64), ProxyError> {
    use std::os::unix::fs::FileExt;
    let length = file.metadata()?.len();
    let mut sha = Sha256::new();
    let mut blake = blake3::Hasher::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut offset = 0_u64;
    loop {
        control.check()?;
        let count = file.read_at(&mut buffer, offset)?;
        if count == 0 {
            break;
        }
        sha.update(&buffer[..count]);
        blake.update(&buffer[..count]);
        offset += count as u64;
    }
    if offset != length || file.metadata()?.len() != length {
        return Err(ProxyError::Correspondence("proxy changed while hashing"));
    }
    Ok((
        sha.finalize().into(),
        blake.finalize().to_hex().to_string(),
        length,
    ))
}

/// SHA-256 of a whole published proxy, for re-verifying a cache entry.
pub fn sha256_file(file: &File, cancelled: &AtomicBool) -> Result<String, ProxyError> {
    let control = Control {
        end: Instant::now() + Duration::from_secs(3600),
        cancelled,
        pause: None,
    };
    Ok(hex(&hash_file(file, &control)?.0))
}

/// Hash a finished proxy file, then measure it afresh, check correspondence
/// with the Original's receipt index and compare sampled pictures with the
/// Original. Runs on a background job thread, after the worker has exited.
/// `proxy` is the private staged output, read in place without copying;
/// `original` is a verified private snapshot of the Original bytes.
pub fn verify_proxy(
    proxy: &File,
    original: &VerifiedSourceInput,
    original_object_blake3: &str,
    original_index: Arc<SourceIndexSnapshot>,
    original_info: &SourceStreamInfo,
    reason: ProxyReason,
    control: VerifyControl<'_>,
) -> Result<ProxySidecar, ProxyError> {
    let cancelled = control.cancelled;
    let control = Control {
        end: Instant::now() + control.timeout,
        cancelled,
        pause: control.pause,
    };
    let (sha256, blake3, length) = hash_file(proxy, &control)?;
    let identity = SourceContentIdentity::new(sha256, length)?;
    let input = VerifiedSourceInput::from_verified_file(proxy.try_clone()?, identity)?;
    let asset = AssetId::new("proxy")?;
    let limits = SourceSessionLimits {
        opening_timeout: control.remaining()?,
        ..SourceSessionLimits::default()
    };
    // One codec thread, as for every measured index.
    let mut session = SourceSession::open_input(input, asset, limits, cancelled)?;
    let proxy_index = session.index().clone();
    let proxy_info = session.info().clone();
    correspondence(
        &proxy_index,
        &proxy_info,
        original_index.index(),
        original_info,
    )?;
    let original_identity = ProxyOriginal {
        blake3: original_object_blake3.to_owned(),
        sha256: hex(&original.identity().sha256()),
        byte_length: original.identity().byte_length(),
        stream_index: original_index.stream_index(),
    };
    if original.identity() != original_index.content() {
        return Err(ProxyError::Correspondence(
            "the Original snapshot is not the indexed bytes",
        ));
    }
    let mut original_session = SourceSession::open_input_indexed(
        original.clone(),
        Arc::clone(&original_index),
        original_info,
        SourceSessionLimits {
            opening_timeout: control.remaining()?,
            ..SourceSessionLimits::default()
        },
        cancelled,
    )?;
    let count = original_index.index().frames().len() as u64;
    let samples = sample_ordinals(count);
    let mut comparison = Comparison::default();
    for ordinal in &samples {
        control.check()?;
        let id = SourceFrameId(*ordinal);
        let timeout = control.remaining()?.min(SEEK_TIMEOUT);
        let expected = original_session.frame(id, timeout, cancelled)?;
        let actual = session.frame(id, timeout, cancelled)?;
        comparison.add(*ordinal, &expected, &actual);
    }
    let fidelity = comparison.finish(samples);
    if !fidelity.within_limits() {
        return Err(ProxyError::Fidelity {
            mean: fidelity.mean(),
            block: fidelity.max_block(),
            bias: fidelity.bias(),
        });
    }
    control.check()?;
    Ok(ProxySidecar {
        schema: PROXY_SIDECAR_SCHEMA,
        recipe: PROXY_RECIPE_VERSION,
        encoder: PROXY_ENCODER.to_owned(),
        reason,
        original: original_identity,
        file: ProxyFileIdentity {
            sha256: hex(&sha256),
            blake3,
            byte_length: length,
        },
        info: proxy_info,
        index: proxy_index,
        fidelity,
    })
}

/// First, last and evenly spaced interior ordinals.
fn sample_ordinals(count: u64) -> Vec<u64> {
    let mut samples: Vec<u64> = (0..FIDELITY_SAMPLES)
        .map(|step| step * (count.saturating_sub(1)) / (FIDELITY_SAMPLES - 1))
        .collect();
    samples.dedup();
    samples
}

/// Running block-average comparison of sampled picture pairs.
#[derive(Default)]
struct Comparison {
    absolute: f64,
    signed: [f64; 3],
    values: u64,
    max_block: f64,
    /// (chroma spread, luma spread, ordinal) of the most colorful sample.
    widest: Option<(f64, f64, u64)>,
}

impl Comparison {
    /// Average both pictures onto the same coarse grid in their own pixel
    /// coordinates and compare the RGB block means, in absolute and signed
    /// terms. The Original's block luma and chroma spread record how much
    /// color variation the samples covered.
    fn add(&mut self, ordinal: u64, expected: &DecodedRgbaFrame, actual: &DecodedRgbaFrame) {
        let columns = FIDELITY_COLUMNS
            .min(actual.width)
            .min(expected.width)
            .max(1);
        let rows = FIDELITY_ROWS.min(actual.height).min(expected.height).max(1);
        let expected = block_means(expected, columns, rows);
        let actual = block_means(actual, columns, rows);
        for (index, (a, b)) in expected.iter().zip(&actual).enumerate() {
            let difference = b - a;
            self.absolute += difference.abs();
            self.signed[index % 3] += difference;
            self.max_block = self.max_block.max(difference.abs());
        }
        self.values += expected.len() as u64;
        let blocks = expected.len() / 3;
        let (mut luma, mut luma_square, mut chroma_square) = (0.0, 0.0, 0.0);
        for block in expected.chunks_exact(3) {
            let y = 0.2126 * block[0] + 0.7152 * block[1] + 0.0722 * block[2];
            luma += y;
            luma_square += y * y;
            chroma_square += (block[2] - y).powi(2) + (block[0] - y).powi(2);
        }
        let blocks = blocks.max(1) as f64;
        let luma_spread = (luma_square / blocks - (luma / blocks).powi(2))
            .max(0.0)
            .sqrt();
        let chroma_spread = (chroma_square / blocks).sqrt();
        if self
            .widest
            .is_none_or(|(widest, _, _)| chroma_spread > widest)
        {
            self.widest = Some((chroma_spread, luma_spread, ordinal));
        }
    }

    fn finish(self, samples: Vec<u64>) -> ProxyFidelity {
        let values = self.values.max(1) as f64;
        let per_channel = (self.values / 3).max(1) as f64;
        let (chroma, luma, ordinal) = self.widest.unwrap_or((0.0, 0.0, 0));
        ProxyFidelity {
            samples,
            mean_abs_difference_milli: milli(self.absolute / values),
            max_block_difference_milli: milli(self.max_block),
            bias_milli: self.signed.map(|sum| signed_milli(sum / per_channel)),
            widest_sample: ordinal,
            widest_luma_spread_milli: milli(luma),
            widest_chroma_spread_milli: milli(chroma),
        }
    }
}

fn block_means(frame: &DecodedRgbaFrame, columns: u32, rows: u32) -> Vec<f64> {
    let mut means = Vec::with_capacity((columns * rows * 3) as usize);
    let width = u64::from(frame.width);
    let height = u64::from(frame.height);
    for row in 0..u64::from(rows) {
        let (y0, y1) = (
            row * height / u64::from(rows),
            (row + 1) * height / u64::from(rows),
        );
        for column in 0..u64::from(columns) {
            let (x0, x1) = (
                column * width / u64::from(columns),
                (column + 1) * width / u64::from(columns),
            );
            let mut channels = [0_u64; 3];
            for y in y0..y1 {
                let line = y as usize * frame.row_stride_bytes;
                for x in x0..x1 {
                    let pixel = line + x as usize * 4;
                    for (channel, total) in channels.iter_mut().enumerate() {
                        *total += u64::from(frame.rgba[pixel + channel]);
                    }
                }
            }
            let area = ((y1 - y0) * (x1 - x0)).max(1) as f64;
            means.extend(channels.iter().map(|total| *total as f64 / area));
        }
    }
    means
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_fits_the_box_in_either_orientation_without_upscaling() {
        assert_eq!(proxy_raster(3840, 2160), (1920, 1080));
        assert_eq!(proxy_raster(2160, 3840), (1080, 1920));
        assert_eq!(proxy_raster(1920, 1080), (1920, 1080));
        assert_eq!(proxy_raster(640, 360), (640, 360));
        assert_eq!(proxy_raster(4096, 2160), (1920, 1012));
        assert_eq!(proxy_raster(1440, 1080), (1440, 1080));
        assert_eq!(proxy_raster(3, 3), (4, 4));
        assert_eq!(proxy_raster(5000, 1000), (1920, 384));
        let (w, h) = proxy_raster(7680, 4320);
        assert_eq!((w, h), (1920, 1080));
    }

    #[test]
    fn requests_and_reports_are_bounded() {
        let request = ProxyRequest {
            protocol: PROXY_PROTOCOL_VERSION,
            input_byte_length: 10,
            stream_index: 0,
            source_width: 3840,
            source_height: 2160,
            width: 1920,
            height: 1080,
            time_base_num: 1,
            time_base_den: 15360,
            sar_num: 1,
            sar_den: 1,
            rotation_quarter_turns: 0,
            transfer: ProxyTransfer::Bt709,
            primaries: ProxyPrimaries::Bt709,
            quality: PROXY_QUALITY,
            frames: 3,
            decode_threads: 4,
            max_output_bytes: 1024,
            timeout_ms: 1000,
        };
        request.validate().unwrap();
        for invalid in [
            ProxyRequest {
                width: 1921,
                ..request
            },
            ProxyRequest {
                width: 4000,
                source_width: 3840,
                ..request
            },
            ProxyRequest {
                frames: 0,
                ..request
            },
            ProxyRequest {
                protocol: 2,
                ..request
            },
            ProxyRequest {
                rotation_quarter_turns: 4,
                ..request
            },
        ] {
            assert!(invalid.validate().is_err());
        }
        let report = ProxyReport {
            protocol: PROXY_PROTOCOL_VERSION,
            output_bytes: 100,
            frames: 3,
            packets: 3,
            keyframes: 3,
            width: 1920,
            height: 1080,
        };
        report.validate_for(&request).unwrap();
        assert!(
            ProxyReport {
                keyframes: 2,
                ..report
            }
            .validate_for(&request)
            .is_err()
        );
        assert!(
            ProxyReport {
                output_bytes: 2048,
                ..report
            }
            .validate_for(&request)
            .is_err()
        );
        assert!(
            serde_json::from_str::<ProxyRequest>(
                &serde_json::to_string(&request)
                    .unwrap()
                    .replace("\"protocol\"", "\"path\":\"x\",\"protocol\"")
            )
            .is_err()
        );
    }

    #[test]
    fn exact_rational_time_comparison() {
        assert!(same_time(512, (1, 15360), 1, (1, 30)));
        assert!(same_time(1001, (1, 30000), 1, (1001, 30000)));
        assert!(!same_time(513, (1, 15360), 1, (1, 30)));
    }

    #[test]
    fn samples_cover_both_ends() {
        assert_eq!(sample_ordinals(1), vec![0]);
        assert_eq!(sample_ordinals(3), vec![0, 1, 2]);
        let samples = sample_ordinals(1800);
        assert_eq!(samples.len(), 16);
        assert_eq!((samples[0], samples[15]), (0, 1799));
    }
}
