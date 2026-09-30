//! SDR encoder pixels from the canonical, already composited working picture.
//! No geometry, source interpretation, timing, encoder or HDR policy lives here.

use crate::color::{conversion, encode_rec709, multiply};
use crate::surface::validate_dimensions;
use crate::{MAX_PIXELS, Primaries, RenderError};

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
