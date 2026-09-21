use deadpan_core::SourceTimestamp;

use crate::{RenderError, SourceColor};

pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_PIXELS: u64 = 16_777_216;
pub const MAX_FRAME_BYTES: u64 = 64 * 1024 * 1024;

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
#[derive(Debug)]
pub struct Rgba8Frame {
    metadata: FrameMetadata,
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
        Ok(Self { metadata, bytes })
    }

    pub const fn metadata(&self) -> &FrameMetadata {
        &self.metadata
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let offset = usize::try_from(
            u64::from(y) * u64::from(self.metadata.row_stride_bytes) + u64::from(x) * 4,
        )
        .expect("validated frame fits address space");
        self.bytes[offset..offset + 4]
            .try_into()
            .expect("validated RGBA8 pixel")
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
    }
}
