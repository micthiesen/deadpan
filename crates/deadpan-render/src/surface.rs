use deadpan_core::SourceTimestamp;

use crate::{RenderError, SourceColor};

pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_PIXELS: u64 = 16_777_216;
pub const MAX_FRAME_BYTES: u64 = 64 * 1024 * 1024;
/// Separate bound for packed RGBA64 (16-bit) source frames, including padding.
pub const MAX_FRAME16_BYTES: u64 = 128 * 1024 * 1024;

/// Bits per channel of an owned source frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleDepth {
    /// Four bytes per pixel: R, G, B, A as u8 (code / 255).
    Eight,
    /// Eight bytes per pixel: R, G, B, A as little-endian u16 (code / 65535).
    Sixteen,
}

/// Display rotation applied clockwise after interpreting source pixel aspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleAspectRatio {
    numerator: u32,
    denominator: u32,
}

impl SampleAspectRatio {
    pub const SQUARE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    pub fn new(numerator: u32, denominator: u32) -> Result<Self, RenderError> {
        if numerator == 0 || denominator == 0 {
            return Err(RenderError::AspectRatio);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }
}

/// Metadata for progressive, full-range, straight-alpha RGBA8. A decoder must
/// perform qualified YUV/range conversion before constructing this surface.
/// There is deliberately no default color interpretation or PTS normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMetadata {
    pub width: u32,
    pub height: u32,
    pub row_stride_bytes: u32,
    pub sample_aspect_ratio: SampleAspectRatio,
    pub rotation: Rotation,
    pub color: SourceColor,
    pub pts: SourceTimestamp,
}

/// A single owned plane, validated once and immutable thereafter. The allocation
/// includes the padding of the final row; padding is never sampled as pixels.
/// Despite the historical name, a frame is either RGBA8 ([`Rgba8Frame::new`])
/// or packed little-endian RGBA64 ([`Rgba8Frame::new_rgba16`]); see
/// [`Rgba8Frame::sample_depth`].
#[derive(Debug)]
pub struct Rgba8Frame {
    metadata: FrameMetadata,
    depth: SampleDepth,
    bytes: Vec<u8>,
}

impl Rgba8Frame {
    pub fn new(metadata: FrameMetadata, bytes: Vec<u8>) -> Result<Self, RenderError> {
        validate_dimensions(metadata.width, metadata.height)?;
        let required = u64::from(metadata.row_stride_bytes) * u64::from(metadata.height);
        if !metadata.row_stride_bytes.is_multiple_of(4)
            || metadata.row_stride_bytes < metadata.width * 4
            || required > MAX_FRAME_BYTES
            || u64::try_from(bytes.len()).ok() != Some(required)
        {
            return Err(RenderError::Layout);
        }
        Ok(Self {
            metadata,
            depth: SampleDepth::Eight,
            bytes,
        })
    }

    /// Full-range, straight-alpha RGBA with four little-endian u16 channels per
    /// pixel (normalized as code / 65535). Rows are a multiple of eight bytes,
    /// at least `width * 8`, and the total is at most [`MAX_FRAME16_BYTES`].
    pub fn new_rgba16(metadata: FrameMetadata, bytes: Vec<u8>) -> Result<Self, RenderError> {
        validate_dimensions(metadata.width, metadata.height)?;
        let required = u64::from(metadata.row_stride_bytes) * u64::from(metadata.height);
        if !metadata.row_stride_bytes.is_multiple_of(8)
            || u64::from(metadata.row_stride_bytes) < u64::from(metadata.width) * 8
            || required > MAX_FRAME16_BYTES
            || u64::try_from(bytes.len()).ok() != Some(required)
        {
            return Err(RenderError::Layout16);
        }
        Ok(Self {
            metadata,
            depth: SampleDepth::Sixteen,
            bytes,
        })
    }

    pub const fn sample_depth(&self) -> SampleDepth {
        self.depth
    }

    pub const fn metadata(&self) -> &FrameMetadata {
        &self.metadata
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[cfg(test)]
    pub(crate) fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let offset = usize::try_from(
            u64::from(y) * u64::from(self.metadata.row_stride_bytes) + u64::from(x) * 4,
        )
        .expect("validated frame fits address space");
        self.bytes[offset..offset + 4]
            .try_into()
            .expect("validated RGBA8 pixel")
    }

    /// Normalized straight RGBA of either depth, exactly code / (2^bits - 1).
    pub(crate) fn normalized(&self, x: u32, y: u32) -> [f64; 4] {
        let row = u64::from(y) * u64::from(self.metadata.row_stride_bytes);
        match self.depth {
            SampleDepth::Eight => {
                let offset = usize::try_from(row + u64::from(x) * 4)
                    .expect("validated frame fits address space");
                std::array::from_fn(|channel| f64::from(self.bytes[offset + channel]) / 255.0)
            }
            SampleDepth::Sixteen => {
                let offset = usize::try_from(row + u64::from(x) * 8)
                    .expect("validated frame fits address space");
                std::array::from_fn(|channel| {
                    let start = offset + channel * 2;
                    f64::from(u16::from_le_bytes([
                        self.bytes[start],
                        self.bytes[start + 1],
                    ])) / 65535.0
                })
            }
        }
    }
}

pub(crate) fn validate_dimensions(width: u32, height: u32) -> Result<(), RenderError> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(RenderError::Dimensions);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Primaries, Transfer};
    use deadpan_core::SourceTimeBase;

    pub(crate) fn metadata(width: u32, height: u32) -> FrameMetadata {
        FrameMetadata {
            width,
            height,
            row_stride_bytes: width * 4,
            sample_aspect_ratio: SampleAspectRatio::SQUARE,
            rotation: Rotation::None,
            color: SourceColor {
                transfer: Transfer::Srgb,
                primaries: Primaries::Rec709,
            },
            pts: SourceTimestamp {
                ticks: -37,
                time_base: SourceTimeBase::new(1, 90_000).expect("valid time base"),
            },
        }
    }

    #[test]
    fn rejects_invalid_size_stride_and_buffer_before_gpu_work() {
        assert!(matches!(
            Rgba8Frame::new(metadata(0, 1), vec![]),
            Err(RenderError::Dimensions)
        ));
        assert!(matches!(
            Rgba8Frame::new(metadata(8193, 1), vec![]),
            Err(RenderError::Dimensions)
        ));
        assert!(matches!(
            Rgba8Frame::new(metadata(8192, 8192), vec![]),
            Err(RenderError::Dimensions)
        ));
        for stride in [0, 3, 7, u32::MAX - 3] {
            let mut meta = metadata(2, 2);
            meta.row_stride_bytes = stride;
            assert!(matches!(
                Rgba8Frame::new(meta, vec![]),
                Err(RenderError::Layout)
            ));
        }
        assert!(Rgba8Frame::new(metadata(2, 2), vec![0; 15]).is_err());
        assert!(Rgba8Frame::new(metadata(2, 2), vec![0; 17]).is_err());
        assert!(SampleAspectRatio::new(0, 1).is_err());
        assert!(SampleAspectRatio::new(1, 0).is_err());
    }

    #[test]
    fn preserves_pts_and_ignores_owned_row_padding() {
        let mut meta = metadata(1, 2);
        meta.row_stride_bytes = 8;
        let frame = Rgba8Frame::new(
            meta,
            vec![1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8, 88, 88, 88, 88],
        )
        .expect("padded rows");
        assert_eq!(frame.pixel(0, 1), [5, 6, 7, 8]);
        assert_eq!(frame.metadata().pts.ticks, -37);
        assert_eq!(frame.sample_depth(), SampleDepth::Eight);
        assert_eq!(frame.normalized(0, 1)[3], 8.0 / 255.0);
    }

    #[test]
    fn rgba16_frames_have_their_own_layout_and_bound() {
        let mut meta = metadata(2, 2);
        meta.row_stride_bytes = 16;
        let frame = Rgba8Frame::new_rgba16(meta, vec![0; 32]).expect("rgba16");
        assert_eq!(frame.sample_depth(), SampleDepth::Sixteen);
        meta.row_stride_bytes = 24;
        let mut bytes = vec![0xee; 48];
        // Pixel (1, 1): R=0, G=1023<<6, B=65535, A=0x8000, little-endian.
        bytes[32..40].copy_from_slice(&[0, 0, 0xc0, 0xff, 0xff, 0xff, 0, 0x80]);
        let padded = Rgba8Frame::new_rgba16(meta, bytes).expect("padded rgba16");
        assert_eq!(
            padded.normalized(1, 1),
            [0.0, 65472.0 / 65535.0, 1.0, 32768.0 / 65535.0]
        );
        for stride in [0, 8, 12, 20, u32::MAX - 7] {
            let mut meta = metadata(2, 2);
            meta.row_stride_bytes = stride;
            assert!(matches!(
                Rgba8Frame::new_rgba16(meta, vec![]),
                Err(RenderError::Layout16)
            ));
        }
        let mut meta = metadata(2, 2);
        meta.row_stride_bytes = 16;
        for length in [31, 33] {
            assert!(matches!(
                Rgba8Frame::new_rgba16(meta, vec![0; length]),
                Err(RenderError::Layout16)
            ));
        }
        // 4096 x 4096 RGBA64 is exactly 128 MiB; 8 more bytes per row exceed it,
        // while an RGBA8 frame would already exceed its own 64 MiB bound.
        let mut meta = metadata(4096, 4096);
        meta.row_stride_bytes = 4096 * 8 + 8;
        assert!(matches!(
            Rgba8Frame::new_rgba16(meta, vec![]),
            Err(RenderError::Layout16)
        ));
        meta.row_stride_bytes = 4096 * 8;
        assert!(Rgba8Frame::new_rgba16(meta, vec![0; 128 * 1024 * 1024]).is_ok());
        assert!(matches!(
            Rgba8Frame::new_rgba16(metadata(0, 1), vec![]),
            Err(RenderError::Dimensions)
        ));
    }
}
