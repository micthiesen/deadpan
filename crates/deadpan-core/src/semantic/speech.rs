//! Recognized speech on the Edit clock: word and sentence motions and objects.
//!
//! The host projects a stored transcript through the staged document's render
//! plan and supplies one run per visible word occurrence, in project frames.
//! Analysis never mutates the document; it only answers where words begin and
//! end in the current arrangement, so motions and objects follow every edit.

use serde::{Deserialize, Serialize};

use crate::{
    DocumentError, EditError, EditErrorCode, FrameRange, FrameRate, ProjectFrame, SemanticContext,
    SemanticVisualSelection,
};

/// Bound on supplied runs, well above three hours of dense speech.
pub const MAX_SPEECH_RUNS: usize = 1_000_000;

/// Handles of `aw` and `as` take at most half of each adjoining pause, capped
/// at this many milliseconds per side.
pub const SPEECH_HANDLE_MILLIS: i64 = 80;

/// One contiguous visible occurrence of a recognized word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechRun {
    pub range: FrameRange,
    /// Transcript word index; a repeated word appears in several runs.
    pub word: u32,
    /// Transcript sentence (segment) index.
    pub sentence: u32,
}

/// Visible word occurrences in Edit order: nonempty, sorted and disjoint.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpeechTimeline {
    runs: Vec<SpeechRun>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpeechObject {
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    InnerWord,
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    AroundWord,
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    InnerSentence,
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    AroundSentence,
}

impl SpeechObject {
    const fn sentence(self) -> bool {
        matches!(self, Self::InnerSentence | Self::AroundSentence)
    }

    const fn around(self) -> bool {
        matches!(self, Self::AroundWord | Self::AroundSentence)
    }
}

impl SpeechTimeline {
    pub fn new(runs: Vec<SpeechRun>) -> Result<Self, EditError> {
        if runs.len() > MAX_SPEECH_RUNS {
            return Err(invalid("speech timeline has too many runs"));
        }
        for (index, run) in runs.iter().enumerate() {
            if run.range.start() == run.range.end() {
                return Err(invalid("speech runs must be nonempty"));
            }
            if index > 0 && runs[index - 1].range.end() > run.range.start() {
                return Err(invalid("speech runs must be sorted and disjoint"));
            }
        }
        Ok(Self { runs })
    }

    pub fn runs(&self) -> &[SpeechRun] {
        &self.runs
    }

    /// Runs inside one scope, clipped to it.
    fn clipped(&self, bounds: (ProjectFrame, ProjectFrame)) -> Vec<SpeechRun> {
        let first = self.runs.partition_point(|run| run.range.end() <= bounds.0);
        self.runs[first..]
            .iter()
            .take_while(|run| run.range.start() < bounds.1)
            .filter_map(|run| {
                let start = run.range.start().max(bounds.0);
                let end = run.range.end().min(bounds.1);
                let range = FrameRange::new(start, end).ok()?;
                (start < end).then_some(SpeechRun { range, ..*run })
            })
            .collect()
    }

    /// Sentence occurrences: consecutive runs of one sentence whose words
    /// advance. Replaying a sentence starts a new occurrence.
    fn sentences(runs: &[SpeechRun]) -> Vec<FrameRange> {
        let mut sentences: Vec<(FrameRange, SpeechRun)> = Vec::new();
        for run in runs {
            match sentences.last_mut() {
                Some((range, last)) if last.sentence == run.sentence && last.word < run.word => {
                    *range = FrameRange::new(range.start(), run.range.end())
                        .expect("sorted runs extend forward");
                    *last = *run;
                }
                _ => sentences.push((run.range, *run)),
            }
        }
        sentences.into_iter().map(|(range, _)| range).collect()
    }

    fn units(&self, bounds: (ProjectFrame, ProjectFrame), sentence: bool) -> Vec<FrameRange> {
        let runs = self.clipped(bounds);
        if sentence {
            Self::sentences(&runs)
        } else {
            runs.into_iter().map(|run| run.range).collect()
        }
    }

    /// Destination of `w`, `b`, `e`, `W` or `B` from an Edit boundary.
    /// Starts are word or sentence beginnings; `end` targets the boundary
    /// after a word. Motions past the last unit clamp to the scope edge.
    pub fn motion_target(
        &self,
        cursor: ProjectFrame,
        bounds: (ProjectFrame, ProjectFrame),
        motion: SpeechMotion,
    ) -> ProjectFrame {
        let units = self.units(bounds, motion.sentence);
        let mut cursor = cursor;
        for _ in 0..motion.count {
            let next = if motion.forward {
                let boundary = |unit: &FrameRange| {
                    if motion.end { unit.end() } else { unit.start() }
                };
                let index = units.partition_point(|unit| boundary(unit) <= cursor);
                units.get(index).map(boundary).unwrap_or(bounds.1)
            } else {
                let index = units.partition_point(|unit| unit.start() < cursor);
                index
                    .checked_sub(1)
                    .map(|index| units[index].start())
                    .unwrap_or(bounds.0)
            };
            if next == cursor {
                break;
            }
            cursor = next;
        }
        cursor
    }

    /// The word or sentence at the picture after the cursor (or before it at
    /// the scope end). Around objects add handles from adjoining pauses: half
    /// of each pause, capped at 80 ms, never reaching neighboring speech.
    pub fn object_range(
        &self,
        cursor: ProjectFrame,
        bounds: (ProjectFrame, ProjectFrame),
        object: SpeechObject,
        rate: FrameRate,
    ) -> Result<FrameRange, EditError> {
        let frame = if cursor >= bounds.1 && cursor > bounds.0 {
            ProjectFrame(cursor.0 - 1)
        } else {
            cursor
        };
        let units = self.units(bounds, object.sentence());
        let index = units.partition_point(|unit| unit.end() <= frame);
        let unit = units
            .get(index)
            .filter(|unit| unit.contains(frame))
            .copied()
            .ok_or_else(|| {
                unavailable(if object.sentence() {
                    "the cursor is not in a recognized sentence"
                } else {
                    "the cursor is not on a recognized word"
                })
            })?;
        if !object.around() {
            return Ok(unit);
        }
        let cap = handle_frames(rate);
        let before = index
            .checked_sub(1)
            .map_or(bounds.0, |previous| units[previous].end());
        let after = units.get(index + 1).map_or(bounds.1, |next| next.start());
        let left = ((unit.start().0 - before.0) / 2).min(cap);
        let right = ((after.0 - unit.end().0) / 2).min(cap);
        FrameRange::new(
            ProjectFrame(unit.start().0 - left),
            ProjectFrame(unit.end().0 + right),
        )
        .map_err(|error| DocumentError::from(error).into())
    }

    /// A Visual selection of a word or sentence object, extending at its end.
    pub fn select_object(
        &self,
        context: &SemanticContext,
        bounds: (ProjectFrame, ProjectFrame),
        object: SpeechObject,
        rate: FrameRate,
    ) -> Result<SemanticContext, EditError> {
        let range = self.object_range(context.cursor, bounds, object, rate)?;
        let mut result = context.clone();
        result.cursor = range.end();
        result.visual_selection = Some(SemanticVisualSelection::Time {
            anchor: range.start(),
            head: range.end(),
            extending: true,
        });
        Ok(result)
    }
}

/// Word and sentence motions. `end` applies to word motions only (`e`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechMotion {
    pub forward: bool,
    pub count: u32,
    pub sentence: bool,
    pub end: bool,
}

/// The largest whole number of project frames within the handle cap.
fn handle_frames(rate: FrameRate) -> i64 {
    SPEECH_HANDLE_MILLIS * i64::from(rate.numerator()) / (1000 * i64::from(rate.denominator()))
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

#[cfg(test)]
mod tests;
