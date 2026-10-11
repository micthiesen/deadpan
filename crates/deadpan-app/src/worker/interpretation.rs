use super::SourceSummary;
use deadpan_source::{ChromaLocation, ColorRange};

impl SourceSummary {
    /// Describes the pixels this reply actually decoded. Presentation captures
    /// the note with the picture and reveals it only after GPU submission.
    pub fn interpretation_note(&self) -> Option<String> {
        let range = |value| match value {
            ColorRange::Limited => "limited",
            ColorRange::Full => "full",
        };
        let mut notes = Vec::new();
        if let Some(value) = self.interpretation.range {
            notes.push(format!("Source range corrected to {}", range(value)));
        } else if self.info.range_assumed {
            notes.push(format!(
                "Source range unspecified; interpreted as {}",
                range(self.info.color.range)
            ));
        }
        if let Some(value) = self.interpretation.chroma_location {
            let position = match value {
                ChromaLocation::Unspecified => "unspecified",
                ChromaLocation::Left => "left",
                ChromaLocation::Center => "center",
                ChromaLocation::TopLeft => "top-left",
                ChromaLocation::Top => "top",
                ChromaLocation::BottomLeft => "bottom-left",
                ChromaLocation::Bottom => "bottom",
            };
            notes.push(format!("Source chroma position corrected to {position}"));
        } else if self.info.chroma_assumed_left {
            notes.push("Source chroma position unspecified; interpreted as left".into());
        }
        (!notes.is_empty()).then(|| notes.join(" · "))
    }
}
