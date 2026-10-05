//! SDR and HDR encoder pixels from the canonical, already composited working
//! picture. No geometry, source interpretation, timing, encoder or automatic
//! output-branch policy lives here.

use crate::color::{
    HDR_REFERENCE_WHITE_NITS, HLG_NOMINAL_PEAK_NITS, PQ_PEAK_NITS, REC2020_LUMA, conversion, dot,
    encode_rec709, hlg_inverse_oetf, hlg_inverse_ootf, hlg_oetf, hlg_ootf, multiply, pq_eotf,
    pq_inverse_eotf,
};
use crate::surface::validate_dimensions;
use crate::{HdrTransfer, MAX_PIXELS, Primaries, RenderError};

/// At most 128 MiB of pixels plus at most 2 MiB of GPU row padding. This is
/// separate from the smaller RGBA8 upload bound. Readback temporarily owns a
/// GPU staging allocation and an equally bounded CPU allocation.
pub const MAX_WORKING_FRAME_BYTES: u64 = MAX_PIXELS * 8 + 2 * 1024 * 1024;

/// Owned little-endian IEEE binary16 RGBA, in linear Rec.2020 D65. Rows include
/// final-row padding. These bytes represent the canonical opaque composite,
/// not a new source image: conversion rejects nonfinite channels and any alpha
/// other than exactly 1.0, without compositing or unpremultiplying it again.
#[derive(Debug)]
pub struct WorkingRgba16Frame {
    width: u32,
    height: u32,
    row_stride_bytes: u32,
    bytes: Vec<u8>,
}

impl WorkingRgba16Frame {
    pub fn new(
        width: u32,
        height: u32,
        row_stride_bytes: u32,
        bytes: Vec<u8>,
    ) -> Result<Self, RenderError> {
        let required = working_layout(width, height, row_stride_bytes)?;
        if u64::try_from(bytes.len()).ok() != Some(required) {
            return Err(RenderError::WorkingLayout);
        }
        Ok(Self {
            width,
            height,
            row_stride_bytes,
            bytes,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }
    pub const fn height(&self) -> u32 {
        self.height
    }
    pub const fn row_stride_bytes(&self) -> u32 {
        self.row_stride_bytes
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn pixel(&self, x: u32, y: u32) -> Result<[f64; 3], RenderError> {
        let offset =
            usize::try_from(u64::from(y) * u64::from(self.row_stride_bytes) + u64::from(x) * 8)
                .map_err(|_| RenderError::WorkingLayout)?;
        let channels: [f64; 4] = std::array::from_fn(|channel| {
            let start = offset + channel * 2;
            half(u16::from_le_bytes([
                self.bytes[start],
                self.bytes[start + 1],
            ]))
        });
        if channels.iter().any(|value| !value.is_finite()) {
            return Err(RenderError::WorkingNonFinite { x, y });
        }
        if channels[3] != 1.0 {
            return Err(RenderError::WorkingAlpha { x, y });
        }
        Ok([channels[0], channels[1], channels[2]])
    }
}

/// Fixed encoder pixel contract. Signal BT.709 primaries, BT.709 transfer and
/// BT.709 YCbCr matrix, limited range (Y 16..235, Cb/Cr 16..240, neutral 128),
/// progressive 4:2:0, square pixels and left chroma location. Chroma is centered
/// horizontally on even luma columns and vertically between each row pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Yuv420Policy {
    Rec709LimitedLeft,
}

/// Bounded owned planar 8-bit Y, Cb, Cr, with tightly packed rows and even
/// dimensions. The caller supplies project-frame timestamps separately; this
/// conversion neither invents a clock nor propagates source PTS into an edit.
#[derive(Debug, PartialEq, Eq)]
pub struct Rec709Yuv420Frame {
    width: u32,
    height: u32,
    luma_len: usize,
    chroma_len: usize,
    bytes: Vec<u8>,
}

impl Rec709Yuv420Frame {
    /// Worker-side, bounded CPU conversion. Transform signed working RGB to
    /// linear Rec.709, clip to SDR gamut/reference white, apply the BT.709 OETF,
    /// then BT.709 YCbCr. Filter unquantized chroma with horizontal [1,2,1]/4
    /// centered on even x (edge clamping) and vertical [1,1]/2 at rows 2y/2y+1.
    /// Quantize once, rounding nonnegative code values to nearest, ties upward.
    /// Only two chroma rows are retained. This does not qualify HDR or an encoder.
    pub fn from_working(frame: &WorkingRgba16Frame) -> Result<Self, RenderError> {
        if !frame.width.is_multiple_of(2) || !frame.height.is_multiple_of(2) {
            return Err(RenderError::EncoderDimensions);
        }
        let luma_len = usize::try_from(u64::from(frame.width) * u64::from(frame.height))
            .map_err(|_| RenderError::WorkingLayout)?;
        let chroma_len = luma_len / 4;
        let length = luma_len
            .checked_add(
                chroma_len
                    .checked_mul(2)
                    .ok_or(RenderError::WorkingLayout)?,
            )
            .ok_or(RenderError::WorkingLayout)?;
        let mut bytes = allocated(length, 0_u8)?;
        let width = usize::try_from(frame.width).map_err(|_| RenderError::WorkingLayout)?;
        let mut rows = allocated(
            width.checked_mul(2).ok_or(RenderError::WorkingLayout)?,
            [0.0; 2],
        )?;
        let matrix = conversion(Primaries::Rec2020, Primaries::Rec709);
        for y in (0..frame.height).step_by(2) {
            for dy in 0..2 {
                let row = usize::try_from(dy).expect("two-row index") * width;
                let destination = usize::try_from(u64::from(y + dy) * u64::from(frame.width))
                    .map_err(|_| RenderError::WorkingLayout)?;
                for x in 0..frame.width {
                    let rgb = multiply(matrix, frame.pixel(x, y + dy)?).map(encode_rec709);
                    let [luma, cb, cr] = rec709_ycbcr(rgb);
                    let x = usize::try_from(x).expect("validated width fits address space");
                    bytes[destination + x] = quantize(16.0 + 219.0 * luma, 16.0, 235.0);
                    rows[row + x] = [cb, cr];
                }
            }
            let chroma_row = usize::try_from(u64::from(y / 2) * u64::from(frame.width / 2))
                .map_err(|_| RenderError::WorkingLayout)?;
            for x in (0..width).step_by(2) {
                let left = x.saturating_sub(1);
                let right = (x + 1).min(width - 1);
                for channel in 0..2 {
                    let filtered = (rows[left][channel]
                        + 2.0 * rows[x][channel]
                        + rows[right][channel]
                        + rows[width + left][channel]
                        + 2.0 * rows[width + x][channel]
                        + rows[width + right][channel])
                        / 8.0;
                    bytes[luma_len + channel * chroma_len + chroma_row + x / 2] =
                        quantize(128.0 + 224.0 * filtered, 16.0, 240.0);
                }
            }
        }
        Ok(Self {
            width: frame.width,
            height: frame.height,
            luma_len,
            chroma_len,
            bytes,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }
    pub const fn height(&self) -> u32 {
        self.height
    }
    pub const fn policy(&self) -> Yuv420Policy {
        Yuv420Policy::Rec709LimitedLeft
    }
    pub const fn y_stride_bytes(&self) -> u32 {
        self.width
    }
    pub const fn chroma_stride_bytes(&self) -> u32 {
        self.width / 2
    }
    pub fn y_plane(&self) -> &[u8] {
        &self.bytes[..self.luma_len]
    }
    pub fn cb_plane(&self) -> &[u8] {
        &self.bytes[self.luma_len..self.luma_len + self.chroma_len]
    }
    pub fn cr_plane(&self) -> &[u8] {
        &self.bytes[self.luma_len + self.chroma_len..]
    }
    /// All tightly packed planes in Y, Cb, Cr order (I420).
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Fixed HDR encoder pixel contract: BT.2020 primaries, BT.2020 non-constant
/// luminance YCbCr matrix, PQ (SMPTE ST 2084) or HLG (ARIB STD-B67) transfer,
/// 10-bit limited range (Y 64..940, Cb/Cr 64..960, neutral 512), progressive
/// 4:2:0, square pixels and left chroma location with the same chroma filter
/// as [`Yuv420Policy::Rec709LimitedLeft`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Yuv420P10Policy {
    Rec2100PqLimitedLeft,
    Rec2100HlgLimitedLeft,
}

/// Per-frame light statistics in cd/m^2 (CTA-861.3 inputs), from the clipped
/// linear display light used for coding: `max_nits` is the frame maximum of
/// per-pixel max(R, G, B); `mean_nits` is the frame average of that value.
/// MaxCLL is the maximum `max_nits` and MaxFALL the maximum `mean_nits` over
/// all frames of a program.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameLight {
    pub max_nits: f64,
    pub mean_nits: f64,
}

/// Bounded owned planar 10-bit Y, Cb, Cr: little-endian u16 samples with codes
/// in the low ten bits, tight rows, even dimensions, all of Y then Cb then Cr
/// (yuv420p10le). The caller supplies project-frame timestamps separately.
#[derive(Debug, PartialEq, Eq)]
pub struct Rec2100Yuv420P10Frame {
    width: u32,
    height: u32,
    policy: Yuv420P10Policy,
    luma_len: usize,
    chroma_len: usize,
    bytes: Vec<u8>,
}

impl Rec2100Yuv420P10Frame {
    /// Worker-side, bounded CPU conversion. Signed linear working Rec.2020
    /// (identity primaries stage) is scaled to cd/m^2 (x203) and clipped per
    /// channel to [0, 10000] (PQ) or [0, 1000] (HLG); light statistics use
    /// this clipped light. PQ applies the inverse EOTF per channel. HLG applies
    /// the inverse OOTF (1000 cd/m^2, gamma 1.2) on Rec.2020 luminance, clips
    /// scene light to [0, 1] per channel, then the OETF. Then the BT.2020 NCL
    /// matrix; chroma uses the SDR boundary's left-sited filter before one
    /// nearest (ties upward) 10-bit quantization.
    pub fn from_working(
        frame: &WorkingRgba16Frame,
        transfer: HdrTransfer,
    ) -> Result<(Self, FrameLight), RenderError> {
        if !frame.width.is_multiple_of(2) || !frame.height.is_multiple_of(2) {
            return Err(RenderError::EncoderDimensions);
        }
        let luma_len = usize::try_from(u64::from(frame.width) * u64::from(frame.height))
            .map_err(|_| RenderError::WorkingLayout)?;
        let chroma_len = luma_len / 4;
        let samples = luma_len
            .checked_add(
                chroma_len
                    .checked_mul(2)
                    .ok_or(RenderError::WorkingLayout)?,
            )
            .ok_or(RenderError::WorkingLayout)?;
        let mut bytes = allocated(
            samples.checked_mul(2).ok_or(RenderError::WorkingLayout)?,
            0_u8,
        )?;
        let width = usize::try_from(frame.width).map_err(|_| RenderError::WorkingLayout)?;
        let mut rows = allocated(
            width.checked_mul(2).ok_or(RenderError::WorkingLayout)?,
            [0.0; 2],
        )?;
        let mut maximum = 0.0_f64;
        let mut total = 0.0_f64;
        let store = |bytes: &mut [u8], sample: usize, code: u16| {
            bytes[sample * 2..sample * 2 + 2].copy_from_slice(&code.to_le_bytes());
        };
        for y in (0..frame.height).step_by(2) {
            for dy in 0..2 {
                let row = usize::try_from(dy).expect("two-row index") * width;
                let destination = usize::try_from(u64::from(y + dy) * u64::from(frame.width))
                    .map_err(|_| RenderError::WorkingLayout)?;
                for x in 0..frame.width {
                    let light = display_light(frame.pixel(x, y + dy)?, transfer);
                    let brightest = light[0].max(light[1]).max(light[2]);
                    maximum = maximum.max(brightest);
                    total += brightest;
                    let [luma, cb, cr] = rec2020_ycbcr(encode_light(light, transfer));
                    let x = usize::try_from(x).expect("validated width fits address space");
                    store(
                        &mut bytes,
                        destination + x,
                        quantize10(64.0 + 876.0 * luma, 64.0, 940.0),
                    );
                    rows[row + x] = [cb, cr];
                }
            }
            let chroma_row = usize::try_from(u64::from(y / 2) * u64::from(frame.width / 2))
                .map_err(|_| RenderError::WorkingLayout)?;
            for x in (0..width).step_by(2) {
                let left = x.saturating_sub(1);
                let right = (x + 1).min(width - 1);
                let filtered: [f64; 2] = std::array::from_fn(|channel| {
                    (rows[left][channel]
                        + 2.0 * rows[x][channel]
                        + rows[right][channel]
                        + rows[width + left][channel]
                        + 2.0 * rows[width + x][channel]
                        + rows[width + right][channel])
                        / 8.0
                });
                for (channel, value) in filtered.into_iter().enumerate() {
                    store(
                        &mut bytes,
                        luma_len + channel * chroma_len + chroma_row + x / 2,
                        quantize10(512.0 + 896.0 * value, 64.0, 960.0),
                    );
                }
            }
        }
        let policy = match transfer {
            HdrTransfer::Pq => Yuv420P10Policy::Rec2100PqLimitedLeft,
            HdrTransfer::Hlg => Yuv420P10Policy::Rec2100HlgLimitedLeft,
        };
        // luma_len is nonzero: validated dimensions are nonzero.
        let light = FrameLight {
            max_nits: maximum,
            mean_nits: total / luma_len as f64,
        };
        Ok((
            Self {
                width: frame.width,
                height: frame.height,
                policy,
                luma_len,
                chroma_len,
                bytes,
            },
            light,
        ))
    }

    pub const fn width(&self) -> u32 {
        self.width
    }
    pub const fn height(&self) -> u32 {
        self.height
    }
    pub const fn policy(&self) -> Yuv420P10Policy {
        self.policy
    }
    pub const fn transfer(&self) -> HdrTransfer {
        match self.policy {
            Yuv420P10Policy::Rec2100PqLimitedLeft => HdrTransfer::Pq,
            Yuv420P10Policy::Rec2100HlgLimitedLeft => HdrTransfer::Hlg,
        }
    }
    /// Two bytes per sample.
    pub const fn y_stride_bytes(&self) -> u32 {
        self.width * 2
    }
    pub const fn chroma_stride_bytes(&self) -> u32 {
        self.width
    }
    pub fn y_plane(&self) -> &[u8] {
        &self.bytes[..self.luma_len * 2]
    }
    pub fn cb_plane(&self) -> &[u8] {
        &self.bytes[self.luma_len * 2..(self.luma_len + self.chroma_len) * 2]
    }
    pub fn cr_plane(&self) -> &[u8] {
        &self.bytes[(self.luma_len + self.chroma_len) * 2..]
    }
    /// All tightly packed planes in Y, Cb, Cr order (yuv420p10le).
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Number of u16 samples in [`Self::bytes`].
    pub const fn sample_count(&self) -> usize {
        self.luma_len + 2 * self.chroma_len
    }
    /// The code at a sample index in Y, Cb, Cr order.
    pub fn code(&self, sample: usize) -> Option<u16> {
        let start = sample.checked_mul(2)?;
        let pair = self.bytes.get(start..start.checked_add(2)?)?;
        Some(u16::from_le_bytes([pair[0], pair[1]]))
    }
}

/// Working Rec.2020 to clipped display light in cd/m^2 for the output transfer.
fn display_light(rgb: [f64; 3], transfer: HdrTransfer) -> [f64; 3] {
    let peak = match transfer {
        HdrTransfer::Pq => PQ_PEAK_NITS,
        HdrTransfer::Hlg => HLG_NOMINAL_PEAK_NITS,
    };
    rgb.map(|value| (value * HDR_REFERENCE_WHITE_NITS).clamp(0.0, peak))
}

fn encode_light(light: [f64; 3], transfer: HdrTransfer) -> [f64; 3] {
    match transfer {
        HdrTransfer::Pq => light.map(pq_inverse_eotf),
        HdrTransfer::Hlg => hlg_inverse_ootf(light).map(hlg_oetf),
    }
}

/// CPU f64 reference of the per-pixel HDR nonlinear signal: signed working
/// Rec.2020 to nonlinear R'G'B' in [0, 1] for the output transfer, using the
/// same clip as [`Rec2100Yuv420P10Frame::from_working`].
pub fn working_to_rec2100_nonlinear(rgb: [f64; 3], transfer: HdrTransfer) -> [f64; 3] {
    encode_light(display_light(rgb, transfer), transfer)
}

/// Verification inverse: one limited-range 10-bit Y, Cb, Cr triple (as
/// decoded, no chroma reconstruction) back to linear working Rec.2020 through
/// the inverse BT.2020 NCL matrix and the transfer's EOTF (HLG: inverse OETF
/// then OOTF). Nonlinear values are clamped to [0, 1].
pub fn rec2100_p10_to_working(transfer: HdrTransfer, ycbcr: [u16; 3]) -> [f64; 3] {
    let luma = (f64::from(ycbcr[0]) - 64.0) / 876.0;
    let cb = (f64::from(ycbcr[1]) - 512.0) / 896.0;
    let cr = (f64::from(ycbcr[2]) - 512.0) / 896.0;
    let r = luma + 2.0 * (1.0 - REC2020_LUMA[0]) * cr;
    let b = luma + 2.0 * (1.0 - REC2020_LUMA[2]) * cb;
    let g = (luma - REC2020_LUMA[0] * r - REC2020_LUMA[2] * b) / REC2020_LUMA[1];
    let nonlinear = [r, g, b].map(|value| value.clamp(0.0, 1.0));
    let light = match transfer {
        HdrTransfer::Pq => nonlinear.map(pq_eotf),
        HdrTransfer::Hlg => hlg_ootf(nonlinear.map(hlg_inverse_oetf)),
    };
    light.map(|nits| nits / HDR_REFERENCE_WHITE_NITS)
}

fn rec2020_ycbcr(rgb: [f64; 3]) -> [f64; 3] {
    let y = dot(REC2020_LUMA, rgb);
    [
        y,
        (rgb[2] - y) / (2.0 * (1.0 - REC2020_LUMA[2])),
        (rgb[0] - y) / (2.0 * (1.0 - REC2020_LUMA[0])),
    ]
}

fn quantize10(value: f64, minimum: f64, maximum: f64) -> u16 {
    value.clamp(minimum, maximum).round() as u16
}

fn rec709_ycbcr([r, g, b]: [f64; 3]) -> [f64; 3] {
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [y, (b - y) / 1.8556, (r - y) / 1.5748]
}

fn quantize(value: f64, minimum: f64, maximum: f64) -> u8 {
    value.clamp(minimum, maximum).round() as u8
}

pub(crate) fn allocated<T: Clone>(length: usize, value: T) -> Result<Vec<T>, RenderError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(length)
        .map_err(|_| RenderError::Allocation)?;
    result.resize(length, value);
    Ok(result)
}

pub(crate) fn working_layout(width: u32, height: u32, stride: u32) -> Result<u64, RenderError> {
    validate_dimensions(width, height)?;
    let row = width.checked_mul(8).ok_or(RenderError::WorkingLayout)?;
    let length = u64::from(stride)
        .checked_mul(u64::from(height))
        .ok_or(RenderError::WorkingLayout)?;
    if !stride.is_multiple_of(8)
        || stride < row
        || length > MAX_WORKING_FRAME_BYTES
        || usize::try_from(length).is_err()
    {
        return Err(RenderError::WorkingLayout);
    }
    Ok(length)
}

/// Check the shared raster, pixel, address-space and padded readback byte bounds
/// before allocating a render target. Device-specific limits are checked by
/// the renderer when creating the target and beginning its readback.
pub fn validate_working_readback_dimensions(width: u32, height: u32) -> Result<(), RenderError> {
    readback_layout(width, height).map(|_| ())
}

pub(crate) fn readback_layout(width: u32, height: u32) -> Result<(u32, u64), RenderError> {
    validate_dimensions(width, height)?;
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let row = width.checked_mul(8).ok_or(RenderError::WorkingLayout)?;
    let stride = row
        .checked_add(alignment - 1)
        .and_then(|value| (value / alignment).checked_mul(alignment))
        .ok_or(RenderError::WorkingLayout)?;
    Ok((stride, working_layout(width, height, stride)?))
}

// Same IEEE binary16 interpretation as the existing picture qualification
// reader, without an additional dependency or an unsafe host-endian cast.
fn half(bits: u16) -> f64 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f64::from(bits & 1023);
    match exponent {
        0 => sign * 2.0_f64.powi(-14) * fraction / 1024.0,
        31 if fraction == 0.0 => sign * f64::INFINITY,
        31 => f64::NAN,
        _ => sign * 2.0_f64.powi(exponent - 15) * (1.0 + fraction / 1024.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32, padding: u32, pixels: &[[u16; 4]]) -> WorkingRgba16Frame {
        assert_eq!(pixels.len(), (width * height) as usize);
        let stride = width * 8 + padding;
        let mut bytes = vec![0xff; (stride * height) as usize];
        for (index, pixel) in pixels.iter().enumerate() {
            let offset = index / width as usize * stride as usize + index % width as usize * 8;
            for (channel, bits) in pixel.iter().enumerate() {
                bytes[offset + channel * 2..offset + channel * 2 + 2]
                    .copy_from_slice(&bits.to_le_bytes());
            }
        }
        WorkingRgba16Frame::new(width, height, stride, bytes).expect("working fixture")
    }

    const BLACK: [u16; 4] = [0, 0, 0, 0x3c00];
    const RED: [u16; 4] = [0x3c00, 0, 0, 0x3c00];

    #[test]
    fn independent_sdr_vectors_have_limited_range_and_planar_order() {
        // Unit Rec.2020 primaries lie outside the 709 gamut and clip to the
        // corresponding pure 709 primary. Expected codes use BT.709's named
        // luma coefficients and limited-range code spans, not our converter.
        for (rgb, expected) in [
            ([0, 0, 0], [16, 128, 128]),
            ([0x3c00; 3], [235, 128, 128]),
            ([0x3c00, 0, 0], [63, 102, 240]),
            ([0, 0x3c00, 0], [173, 42, 26]),
            ([0, 0, 0x3c00], [32, 240, 118]),
            ([0x3800; 3], [171, 128, 128]),
        ] {
            let pixel = [rgb[0], rgb[1], rgb[2], 0x3c00];
            let result = Rec709Yuv420Frame::from_working(&frame(2, 2, 8, &[pixel; 4])).unwrap();
            assert_eq!(result.y_plane(), &[expected[0]; 4]);
            assert_eq!(result.cb_plane(), &[expected[1]]);
            assert_eq!(result.cr_plane(), &[expected[2]]);
            assert_eq!(
                result.bytes(),
                &[
                    expected[0],
                    expected[0],
                    expected[0],
                    expected[0],
                    expected[1],
                    expected[2]
                ]
            );
            assert_eq!(result.policy(), Yuv420Policy::Rec709LimitedLeft);
            assert_eq!(
                (result.y_stride_bytes(), result.chroma_stride_bytes()),
                (2, 1)
            );
        }
    }

    #[test]
    fn left_chroma_impulses_and_alternating_columns_preserve_filter_phase() {
        let mut pixels = [BLACK; 12];
        pixels[0] = RED;
        pixels[6] = RED;
        let left = Rec709Yuv420Frame::from_working(&frame(6, 2, 0, &pixels)).unwrap();
        assert_eq!(left.cr_plane(), &[212, 128, 128]); // clamped edge: 3/4 red
        assert_eq!(left.cb_plane(), &[109, 128, 128]);
        let mut pixels = [BLACK; 12];
        pixels[1] = RED; // odd column, only the upper row: 1/8 at each neighbor
        let odd = Rec709Yuv420Frame::from_working(&frame(6, 2, 0, &pixels)).unwrap();
        assert_eq!(odd.cr_plane(), &[142, 142, 128]);
        assert_eq!(odd.cb_plane(), &[125, 125, 128]);
        let pixels = [
            RED, BLACK, RED, BLACK, RED, BLACK, RED, BLACK, RED, BLACK, RED, BLACK,
        ];
        let alternating = Rec709Yuv420Frame::from_working(&frame(6, 2, 0, &pixels)).unwrap();
        assert_eq!(alternating.cr_plane(), &[212, 184, 184]);
        assert_eq!(alternating.cb_plane(), &[109, 115, 115]);
        assert_eq!(
            alternating.y_plane(),
            &[63, 16, 63, 16, 63, 16, 63, 16, 63, 16, 63, 16]
        );
    }

    #[test]
    fn signed_working_values_survive_until_the_rec709_gamut_clip() {
        let input = [0.5, 0.25, -0.125];
        // Independently tabulated D65 Rec.2020 -> Rec.709 primary transform.
        let expected = [
            1.6604910021084345 * input[0]
                - 0.5876411387885495 * input[1]
                - 0.07284986331988483 * input[2],
            -0.12455047452159074 * input[0] + 1.1328998971259597 * input[1]
                - 0.00834942260436944 * input[2],
            -0.018150763354905277 * input[0] - 0.10057889800800736 * input[1]
                + 1.118729661362913 * input[2],
        ]
        .map(encode_rec709);
        let actual =
            multiply(conversion(Primaries::Rec2020, Primaries::Rec709), input).map(encode_rec709);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12);
        }
        let with_negative = frame(2, 2, 0, &[[0x3800, 0x3400, 0xb000, 0x3c00]; 4]);
        let prematurely_clipped = frame(2, 2, 0, &[[0x3800, 0x3400, 0, 0x3c00]; 4]);
        assert_ne!(
            Rec709Yuv420Frame::from_working(&with_negative).unwrap(),
            Rec709Yuv420Frame::from_working(&prematurely_clipped).unwrap()
        );
    }

    #[test]
    fn rejects_nonfinite_or_nonopaque_pixels_but_never_samples_padding() {
        for channel in 0..4 {
            for invalid in [0x7c00, 0xfc00, 0x7e00] {
                let mut pixel = BLACK;
                pixel[channel] = invalid;
                assert!(matches!(
                    Rec709Yuv420Frame::from_working(&frame(2, 2, 0, &[pixel; 4])),
                    Err(RenderError::WorkingNonFinite { x: 0, y: 0 })
                ));
            }
        }
        let mut transparent = BLACK;
        transparent[3] = 0;
        assert!(matches!(
            Rec709Yuv420Frame::from_working(&frame(2, 2, 0, &[transparent; 4])),
            Err(RenderError::WorkingAlpha { .. })
        ));
        assert_eq!(
            Rec709Yuv420Frame::from_working(&frame(2, 2, 0, &[BLACK; 4])).unwrap(),
            Rec709Yuv420Frame::from_working(&frame(2, 2, 248, &[BLACK; 4])).unwrap()
        );
    }

    // Independent f64 Rec.2100 reference: published constants (HLG c rounded
    // as printed in BT.2100), separate matrix/filter/quantization code.
    fn reference_signal(rgb: [f64; 3], transfer: HdrTransfer) -> [f64; 3] {
        match transfer {
            HdrTransfer::Pq => rgb.map(|working| {
                let y = (working * 203.0).clamp(0.0, 10_000.0) / 10_000.0;
                let (m1, m2) = (0.1593017578125, 78.84375);
                let (c1, c2, c3) = (0.8359375, 18.8515625, 18.6875);
                ((c1 + c2 * y.powf(m1)) / (1.0 + c3 * y.powf(m1))).powf(m2)
            }),
            HdrTransfer::Hlg => {
                let display = rgb.map(|working| (working * 203.0).clamp(0.0, 1000.0));
                let yd = 0.2627 * display[0] + 0.6780 * display[1] + 0.0593 * display[2];
                display.map(|fd| {
                    let e = if yd <= 0.0 {
                        0.0
                    } else {
                        // Es = (Fd / Lw) * (Yd / Lw)^((1 - gamma) / gamma)
                        (fd / 1000.0 * (yd / 1000.0).powf(-0.2 / 1.2)).min(1.0)
                    };
                    if e <= 1.0 / 12.0 {
                        (3.0 * e).sqrt()
                    } else {
                        0.17883277 * (12.0 * e - 0.28466892).ln() + 0.55991073
                    }
                })
            }
        }
    }

    fn reference_codes(
        width: usize,
        height: usize,
        pixels: &[[f64; 3]],
        transfer: HdrTransfer,
    ) -> Vec<u16> {
        let ycbcr: Vec<[f64; 3]> = pixels
            .iter()
            .map(|rgb| {
                let [r, g, b] = reference_signal(*rgb, transfer);
                let y = 0.2627 * r + 0.6780 * g + 0.0593 * b;
                [y, (b - y) / 1.8814, (r - y) / 1.4746]
            })
            .collect();
        let code = |value: f64, low: f64, high: f64| (value.clamp(low, high) + 0.5).floor() as u16;
        let mut output: Vec<u16> = ycbcr
            .iter()
            .map(|value| code(64.0 + 876.0 * value[0], 64.0, 940.0))
            .collect();
        for channel in [1, 2] {
            for cy in 0..height / 2 {
                for cx in 0..width / 2 {
                    let x = cx * 2;
                    let mut sum = 0.0;
                    for y in [cy * 2, cy * 2 + 1] {
                        for (column, weight) in [
                            (x.saturating_sub(1), 0.125),
                            (x, 0.25),
                            ((x + 1).min(width - 1), 0.125),
                        ] {
                            sum += weight * ycbcr[y * width + column][channel];
                        }
                    }
                    output.push(code(512.0 + 896.0 * sum, 64.0, 960.0));
                }
            }
        }
        output
    }

    fn half_bits(value: f64) -> u16 {
        // Exact for the fixture values below, which are chosen representable.
        let value32 = value as f32;
        let bits = value32.to_bits();
        let sign = ((bits >> 16) & 0x8000) as u16;
        if value32 == 0.0 {
            return sign;
        }
        let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
        assert!(
            (1..31).contains(&exponent),
            "fixture {value} outside normal binary16"
        );
        assert_eq!(
            bits & 0x1fff,
            0,
            "fixture {value} not representable in binary16"
        );
        sign | ((exponent as u16) << 10) | ((bits >> 13) & 0x3ff) as u16
    }

    fn working_frame(
        width: u32,
        height: u32,
        padding: u32,
        pixels: &[[f64; 3]],
    ) -> WorkingRgba16Frame {
        let pixels: Vec<[u16; 4]> = pixels
            .iter()
            .map(|rgb| {
                [
                    half_bits(rgb[0]),
                    half_bits(rgb[1]),
                    half_bits(rgb[2]),
                    0x3c00,
                ]
            })
            .collect();
        frame(width, height, padding, &pixels)
    }

    fn codes(frame: &Rec2100Yuv420P10Frame) -> Vec<u16> {
        (0..frame.sample_count())
            .map(|index| frame.code(index).unwrap())
            .collect()
    }

    #[test]
    fn hdr_p10_vectors_match_independent_reference_and_layout() {
        for (transfer, white, policy) in [
            (HdrTransfer::Pq, 573, Yuv420P10Policy::Rec2100PqLimitedLeft),
            (
                HdrTransfer::Hlg,
                721,
                Yuv420P10Policy::Rec2100HlgLimitedLeft,
            ),
        ] {
            let (black, light) = Rec2100Yuv420P10Frame::from_working(
                &working_frame(2, 2, 8, &[[0.0; 3]; 4]),
                transfer,
            )
            .unwrap();
            assert_eq!(codes(&black), [64, 64, 64, 64, 512, 512]);
            assert_eq!(
                light,
                FrameLight {
                    max_nits: 0.0,
                    mean_nits: 0.0
                }
            );
            let (reference_white, light) = Rec2100Yuv420P10Frame::from_working(
                &working_frame(2, 2, 0, &[[1.0; 3]; 4]),
                transfer,
            )
            .unwrap();
            assert_eq!(
                codes(&reference_white),
                [white, white, white, white, 512, 512]
            );
            assert_eq!(
                light,
                FrameLight {
                    max_nits: 203.0,
                    mean_nits: 203.0
                }
            );
            assert_eq!(reference_white.policy(), policy);
            assert_eq!(reference_white.transfer(), transfer);
            assert_eq!(
                (
                    reference_white.y_stride_bytes(),
                    reference_white.chroma_stride_bytes()
                ),
                (4, 2)
            );
            assert_eq!(
                reference_white.y_plane(),
                &[white as u8, (white >> 8) as u8].repeat(4)[..]
            );
            assert_eq!(reference_white.cb_plane(), &[0, 2]);
            assert_eq!(reference_white.cr_plane(), &[0, 2]);
            assert_eq!(reference_white.bytes().len(), 12);
            assert_eq!(reference_white.code(6), None);
        }
        // PQ 10000 cd/m^2 neutral (working 49.26..) is the nominal peak code 940.
        let (peak, _) = Rec2100Yuv420P10Frame::from_working(
            &working_frame(2, 2, 0, &[[64.0; 3]; 4]),
            HdrTransfer::Pq,
        )
        .unwrap();
        assert_eq!(codes(&peak), [940, 940, 940, 940, 512, 512]);
    }

    #[test]
    fn hdr_p10_patterns_match_the_independent_reference_exactly() {
        let values = [
            0.0,
            0.000_061_035_156_25,
            0.0625,
            0.25,
            0.5,
            1.0,
            1.5,
            2.0,
            4.875,
            5.0,
            19.75,
            49.25,
            60.0,
            -0.25,
            -2.0,
        ];
        let (width, height) = (10_usize, 6_usize);
        let mut state = 17_u32;
        let pixels: Vec<[f64; 3]> = (0..width * height)
            .map(|_| {
                std::array::from_fn(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    values[(state >> 16) as usize % values.len()]
                })
            })
            .collect();
        for transfer in [HdrTransfer::Pq, HdrTransfer::Hlg] {
            let (actual, light) = Rec2100Yuv420P10Frame::from_working(
                &working_frame(width as u32, height as u32, 24, &pixels),
                transfer,
            )
            .unwrap();
            assert_eq!(
                codes(&actual),
                reference_codes(width, height, &pixels, transfer)
            );
            let peak = if transfer == HdrTransfer::Pq {
                10_000.0
            } else {
                1000.0
            };
            let per_pixel: Vec<f64> = pixels
                .iter()
                .map(|rgb| {
                    rgb.map(|v| (v * 203.0).clamp(0.0, peak))
                        .into_iter()
                        .fold(0.0, f64::max)
                })
                .collect();
            let max = per_pixel.iter().copied().fold(0.0, f64::max);
            let mean = per_pixel.iter().sum::<f64>() / per_pixel.len() as f64;
            assert_eq!(light.max_nits, max);
            assert!((light.mean_nits - mean).abs() < 1e-9);
            assert!(
                codes(&actual)
                    .iter()
                    .all(|&code| (64..=960).contains(&code))
            );
        }
    }

    #[test]
    fn hdr_light_statistics_clip_per_transfer_and_above_reference_values_survive() {
        let pixels = [
            [1.0, 0.5, 0.0],
            [100.0, 0.0, 0.0],
            [-1.0, -1.0, -1.0],
            [2.0, 4.0, 1.0],
        ];
        let (_, pq) =
            Rec2100Yuv420P10Frame::from_working(&working_frame(2, 2, 0, &pixels), HdrTransfer::Pq)
                .unwrap();
        assert_eq!(pq.max_nits, 10_000.0);
        assert_eq!(pq.mean_nits, (203.0 + 10_000.0 + 0.0 + 812.0) / 4.0);
        let (_, hlg) =
            Rec2100Yuv420P10Frame::from_working(&working_frame(2, 2, 0, &pixels), HdrTransfer::Hlg)
                .unwrap();
        assert_eq!(hlg.max_nits, 1000.0);
        assert_eq!(hlg.mean_nits, (203.0 + 1000.0 + 0.0 + 812.0) / 4.0);
        // Working values above reference white are coded, not clipped at 1.0.
        let (one, _) = Rec2100Yuv420P10Frame::from_working(
            &working_frame(2, 2, 0, &[[1.0; 3]; 4]),
            HdrTransfer::Pq,
        )
        .unwrap();
        let (five, _) = Rec2100Yuv420P10Frame::from_working(
            &working_frame(2, 2, 0, &[[5.0; 3]; 4]),
            HdrTransfer::Pq,
        )
        .unwrap();
        assert!(five.code(0).unwrap() > one.code(0).unwrap() + 100);
    }

    #[test]
    fn hdr_p10_rejects_invalid_input_and_ignores_padding() {
        for transfer in [HdrTransfer::Pq, HdrTransfer::Hlg] {
            let mut nonfinite = BLACK;
            nonfinite[1] = 0x7c00;
            assert!(matches!(
                Rec2100Yuv420P10Frame::from_working(&frame(2, 2, 0, &[nonfinite; 4]), transfer),
                Err(RenderError::WorkingNonFinite { x: 0, y: 0 })
            ));
            let mut transparent = BLACK;
            transparent[3] = 0x3800;
            assert!(matches!(
                Rec2100Yuv420P10Frame::from_working(&frame(2, 2, 0, &[transparent; 4]), transfer),
                Err(RenderError::WorkingAlpha { .. })
            ));
            assert!(matches!(
                Rec2100Yuv420P10Frame::from_working(&frame(1, 2, 0, &[BLACK; 2]), transfer),
                Err(RenderError::EncoderDimensions)
            ));
            assert_eq!(
                Rec2100Yuv420P10Frame::from_working(&frame(2, 2, 0, &[RED; 4]), transfer).unwrap(),
                Rec2100Yuv420P10Frame::from_working(&frame(2, 2, 248, &[RED; 4]), transfer)
                    .unwrap()
            );
        }
    }

    #[test]
    fn p10_inverse_recovers_working_light_within_quantization() {
        for transfer in [HdrTransfer::Pq, HdrTransfer::Hlg] {
            for level in [0.01, 0.25, 1.0, 2.5, 4.0] {
                let rgb = [level; 3];
                let signal = working_to_rec2100_nonlinear(rgb, transfer);
                let code = (64.0 + 876.0 * signal[0] + 0.5).floor() as u16;
                let back = rec2100_p10_to_working(transfer, [code, 512, 512]);
                for value in back {
                    // One 10-bit step near these levels is below 1% in light.
                    assert!(
                        (value / level - 1.0).abs() < 0.012,
                        "{transfer:?} {level} {value}"
                    );
                }
            }
        }
        assert_eq!(
            rec2100_p10_to_working(HdrTransfer::Hlg, [64, 512, 512]),
            [0.0; 3]
        );
    }

    #[test]
    fn dimensions_lengths_and_padded_readback_are_checked_before_allocation() {
        for (width, height) in [(0, 2), (2, 0), (8193, 2), (8192, 8192), (u32::MAX, 2)] {
            assert!(WorkingRgba16Frame::new(width, height, 16, vec![]).is_err());
            assert!(readback_layout(width, height).is_err());
        }
        for stride in [0, 8, 15, 17, u32::MAX - 7] {
            assert!(WorkingRgba16Frame::new(2, 2, stride, vec![]).is_err());
        }
        for length in [31, 33] {
            assert!(WorkingRgba16Frame::new(2, 2, 16, vec![0; length]).is_err());
        }
        assert!(matches!(
            Rec709Yuv420Frame::from_working(&frame(1, 2, 0, &[BLACK; 2])),
            Err(RenderError::EncoderDimensions)
        ));
        assert!(matches!(
            Rec709Yuv420Frame::from_working(&frame(2, 1, 0, &[BLACK; 2])),
            Err(RenderError::EncoderDimensions)
        ));
        assert_eq!(readback_layout(2, 2).unwrap(), (256, 512));
        assert_eq!(readback_layout(8192, 2048).unwrap(), (65536, 134217728));
        assert_eq!(half(1), 2.0_f64.powi(-24));
        assert_eq!(half(0x8001), -2.0_f64.powi(-24));
    }
}
