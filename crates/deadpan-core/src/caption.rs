//! Text captions: a line of text shown over part of a host beat
//! (specification §8.2 "Delayed caption", §3 attachments).
//!
//! Like a cutaway, a caption lives on a Source or Hold in that beat's local
//! clock, which is its content's clock, so moving, copying, splitting (the full
//! host stays behind Partitions), trimming, isolating and deleting the host
//! carry it. A caption that starts after the host's first frame is a delayed
//! caption. Inside a Repeat a caption may wait for a later play: `reveal`
//! names the first play (counting from one) of the innermost enclosing Repeat
//! that shows it. That is a position, not a play identity: it counts the
//! plays in their current order when the picture is drawn, so reordering,
//! adding or removing plays changes which play reaches it (this keeps the
//! "only from the third time" gag meaning the third time heard). A caption
//! changes no timing, picture provider or sound; it
//! is drawn over the composed picture by the shared renderer, in preview and
//! export alike.

use serde::{Deserialize, Serialize};

use crate::{DocumentError, DocumentErrorCode, FrameDuration, FrameRange, ProjectFrame, TimeError};

/// Captions one host may carry.
pub const MAX_CAPTIONS_PER_NODE: usize = 16;
/// Characters in one caption: one short line.
pub const MAX_CAPTION_CHARS: usize = 120;

/// Where a caption line sits on the canvas.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionPlacement {
    #[default]
    Bottom,
    Top,
    Center,
}

impl CaptionPlacement {
    pub fn is_bottom(&self) -> bool {
        *self == Self::Bottom
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Center => "center",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Caption {
    /// Half-open project frames in the host's local output clock. A caption
    /// longer than its host is clipped to the host.
    pub range: FrameRange,
    /// One line of text, shown exactly as authored.
    pub text: String,
    #[serde(default, skip_serializing_if = "CaptionPlacement::is_bottom")]
    pub placement: CaptionPlacement,
    /// The first play position (from one, in the Repeat's current play
    /// order, not a stable play identity) of the innermost enclosing Repeat
    /// that shows this caption; absent shows it in every play.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reveal: Option<std::num::NonZeroU32>,
}

impl Caption {
    /// Whether the caption shows at host-local position `local` (a picture
    /// center) in the zero-based play `play` of its innermost Repeat.
    pub fn shows(&self, local: crate::ExactRatio, play: Option<u32>) -> bool {
        !local.compare_integer(self.range.start().0).is_lt()
            && local.compare_integer(self.range.end().0).is_lt()
            && self
                .reveal
                .is_none_or(|reveal| play.unwrap_or(0) + 1 >= reveal.get())
    }

    /// The same caption after its host gains `prefix` frames before its
    /// current content.
    pub fn with_owner_prefix(&self, prefix: FrameDuration) -> Result<Self, TimeError> {
        let shift = |frame: ProjectFrame| {
            frame
                .0
                .checked_add(prefix.frames())
                .map(ProjectFrame)
                .ok_or(TimeError::Overflow)
        };
        Ok(Self {
            range: FrameRange::new(shift(self.range.start())?, shift(self.range.end())?)?,
            ..self.clone()
        })
    }
}

/// Shift every caption of a host gaining a prefix.
pub fn captions_with_owner_prefix(
    captions: &[Caption],
    prefix: FrameDuration,
) -> Result<Vec<Caption>, TimeError> {
    captions
        .iter()
        .map(|caption| caption.with_owner_prefix(prefix))
        .collect()
}

/// A caption's text: one nonempty line of at most [`MAX_CAPTION_CHARS`]
/// characters, without control characters or surrounding whitespace.
pub fn validate_caption_text(text: &str) -> Result<(), DocumentError> {
    if text.trim().is_empty()
        || text.trim() != text
        || text.chars().count() > MAX_CAPTION_CHARS
        || text.chars().any(char::is_control)
    {
        return Err(invalid(
            "a caption is one nonempty line of at most 120 characters with no control characters",
        ));
    }
    Ok(())
}

/// Check one host's captions: bounded, valid text, nonnegative nonempty
/// ranges, sorted by start and disjoint where they share a placement.
pub(crate) fn validate(captions: &[Caption]) -> Result<(), DocumentError> {
    if captions.len() > MAX_CAPTIONS_PER_NODE {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "a beat carries at most 16 captions",
        ));
    }
    for (index, caption) in captions.iter().enumerate() {
        validate_caption_text(&caption.text)?;
        if caption.range.start().0 < 0 || caption.range.duration() == FrameDuration::ZERO {
            return Err(invalid(
                "a caption range is nonempty and starts at or after 0",
            ));
        }
        if let Some(previous) = index.checked_sub(1).map(|previous| &captions[previous])
            && previous.range.start() > caption.range.start()
        {
            return Err(invalid("captions on one beat are sorted by start"));
        }
        if captions[..index].iter().any(|earlier| {
            earlier.placement == caption.placement
                && earlier.range.end() > caption.range.start()
                && caption.range.end() > earlier.range.start()
        }) {
            return Err(invalid(
                "captions at one placement on one beat do not overlap",
            ));
        }
    }
    Ok(())
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    fn caption(start: i64, end: i64, text: &str, placement: CaptionPlacement) -> Caption {
        Caption {
            range: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            text: text.into(),
            placement,
            reveal: None,
        }
    }

    #[test]
    fn captions_show_inside_their_range_from_their_reveal_play() {
        let mut later = caption(3, 6, "wait for it", CaptionPlacement::Bottom);
        let at = |frame: i64| crate::ExactRatio::new(i128::from(frame) * 2 + 1, 2).unwrap();
        assert!(!later.shows(at(2), None));
        assert!(later.shows(at(3), None) && later.shows(at(5), None));
        assert!(!later.shows(at(6), None));
        later.reveal = NonZeroU32::new(3);
        assert!(!later.shows(at(4), Some(0)) && !later.shows(at(4), Some(1)));
        assert!(later.shows(at(4), Some(2)) && later.shows(at(4), Some(5)));
        assert!(
            !later.shows(at(4), None),
            "outside a Repeat only play one exists"
        );
        let shifted = later
            .with_owner_prefix(FrameDuration::new(4).unwrap())
            .unwrap();
        assert_eq!(
            shifted.range,
            FrameRange::new(ProjectFrame(7), ProjectFrame(10)).unwrap()
        );
        assert_eq!(shifted.text, later.text);
    }

    #[test]
    fn validation_bounds_text_and_orders_disjoint_ranges_per_placement() {
        let bottom = caption(0, 4, "Are we done?", CaptionPlacement::Bottom);
        let top = caption(2, 6, "No.", CaptionPlacement::Top);
        assert!(validate(&[bottom.clone(), top.clone()]).is_ok());
        assert!(
            validate(&[top.clone(), bottom.clone()]).is_err(),
            "sorted by start"
        );
        let overlapping = caption(2, 5, "again", CaptionPlacement::Bottom);
        assert!(validate(&[bottom.clone(), overlapping]).is_err());
        for text in ["", "  padded", "two\nlines", &"x".repeat(121)] {
            assert!(validate_caption_text(text).is_err(), "{text:?}");
        }
        assert!(validate_caption_text(&"é".repeat(120)).is_ok());
        assert!(validate(&vec![bottom; 17]).is_err());
        let wire = serde_json::to_value(&top).unwrap();
        assert_eq!(wire["placement"], "top");
        assert!(wire.get("reveal").is_none());
        assert_eq!(serde_json::from_value::<Caption>(wire).unwrap(), top);
    }
}
