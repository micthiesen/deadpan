//! Explicit picture interpretation, separate from observed source metadata.

use crate::{
    ChromaLocation, ColorMatrix, ColorRange, ColorTransfer, SourceDecodeError, SourceStreamInfo,
};

/// A rendering choice over the same decoded pictures and exact source clocks.
/// `None` retains the admitted source interpretation. These choices do not
/// relabel the container, codec observations, or raw export inspection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VideoInterpretation {
    pub range: Option<ColorRange>,
    pub chroma_location: Option<ChromaLocation>,
}

/// The choices supported by an already qualified pixel/color format. Compact
/// enough to retain with receipt metadata while checking an entire history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoInterpretationSupport {
    required_range: Option<ColorRange>,
    chroma: bool,
}

impl VideoInterpretationSupport {
    pub fn for_qualified_stream(info: &SourceStreamInfo) -> Self {
        Self {
            required_range: if matches!(info.color.transfer, ColorTransfer::Pq | ColorTransfer::Hlg)
            {
                Some(ColorRange::Limited)
            } else if info.color.matrix == ColorMatrix::Rgb {
                Some(ColorRange::Full)
            } else {
                None
            },
            chroma: crate::ffi::subsampled_yuv(&info.pixel_format),
        }
    }

    pub fn allows_range(self, range: ColorRange) -> bool {
        self.required_range.is_none_or(|required| required == range)
    }

    pub fn allows_chroma_position(self) -> bool {
        self.chroma
    }

    pub fn validate(self, interpretation: VideoInterpretation) -> Result<(), SourceDecodeError> {
        interpretation.native()?;
        if interpretation
            .range
            .is_some_and(|range| !self.allows_range(range))
        {
            return Err(SourceDecodeError::InvalidConfiguration(
                "range interpretation is unsupported for this qualified color format",
            ));
        }
        if interpretation.chroma_location.is_some() && !self.chroma {
            return Err(SourceDecodeError::InvalidConfiguration(
                "chroma positioning requires subsampled YUV",
            ));
        }
        Ok(())
    }
}

impl VideoInterpretation {
    pub const SOURCE: Self = Self {
        range: None,
        chroma_location: None,
    };

    /// Check a choice against retained, already qualified stream metadata.
    /// This does not admit unqualified metadata or establish source identity.
    pub fn validate_for(self, info: &SourceStreamInfo) -> Result<(), SourceDecodeError> {
        VideoInterpretationSupport::for_qualified_stream(info).validate(self)
    }

    pub(crate) fn native(self) -> Result<[i32; 2], SourceDecodeError> {
        let range = match self.range {
            None => 0,
            Some(ColorRange::Limited) => 1,
            Some(ColorRange::Full) => 2,
        };
        let chroma = match self.chroma_location {
            None => 0,
            Some(ChromaLocation::Left) => 1,
            Some(ChromaLocation::Center) => 2,
            Some(ChromaLocation::TopLeft) => 3,
            Some(ChromaLocation::Top) => 4,
            Some(ChromaLocation::BottomLeft) => 5,
            Some(ChromaLocation::Bottom) => 6,
            Some(ChromaLocation::Unspecified) => {
                return Err(SourceDecodeError::InvalidConfiguration(
                    "an explicit chroma interpretation must name a position",
                ));
            }
        };
        Ok([range, chroma])
    }
}

impl From<deadpan_core::SourceInterpretation> for VideoInterpretation {
    fn from(value: deadpan_core::SourceInterpretation) -> Self {
        use deadpan_core::{SourceChromaLocation as Chroma, SourceColorRange as Range};
        Self {
            range: value.range.map(|range| match range {
                Range::Limited => ColorRange::Limited,
                Range::Full => ColorRange::Full,
            }),
            chroma_location: value.chroma_location.map(|chroma| match chroma {
                Chroma::Left => ChromaLocation::Left,
                Chroma::Center => ChromaLocation::Center,
                Chroma::TopLeft => ChromaLocation::TopLeft,
                Chroma::Top => ChromaLocation::Top,
                Chroma::BottomLeft => ChromaLocation::BottomLeft,
                Chroma::Bottom => ChromaLocation::Bottom,
            }),
        }
    }
}
