//! Recognized speech on the Edit clock: word, sentence and pause motions and
//! objects.
//!
//! The host projects a stored transcript through the staged document's render
//! plan and supplies one run per visible word occurrence, in project frames,
//! and projects detected pauses the same way. Analysis never mutates the
//! document; it only answers where words and pauses lie in the current
//! arrangement, so motions and objects follow every edit. Words and pauses come
//! from separate analyses, so each carries its own reason when it is missing.

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

/// Visible word occurrences and pauses in Edit order, each nonempty, sorted
/// and disjoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechTimeline {
    runs: Vec<SpeechRun>,
    /// Why words are unavailable; `runs` is then empty.
    words_missing: Option<String>,
    /// Quiet intervals, or why they are unavailable.
    pauses: Result<Vec<FrameRange>, String>,
}

impl Default for SpeechTimeline {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            words_missing: None,
            pauses: Err(PAUSES_NOT_DETECTED.into()),
        }
    }
}

const PAUSES_NOT_DETECTED: &str =
    "pauses are not ready: the Original's speech has not been analysed";

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
    /// `ip`: a detected pause.
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    InnerPause,
    /// `ap`: a pause with short handles of the content around it.
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    AroundPause,
}

/// What a speech motion or object steps between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechUnit {
    Word,
    Sentence,
    Pause,
}

impl SpeechObject {
    pub const fn unit(self) -> SpeechUnit {
        match self {
            Self::InnerWord | Self::AroundWord => SpeechUnit::Word,
            Self::InnerSentence | Self::AroundSentence => SpeechUnit::Sentence,
            Self::InnerPause | Self::AroundPause => SpeechUnit::Pause,
        }
    }

    const fn around(self) -> bool {
        matches!(
            self,
            Self::AroundWord | Self::AroundSentence | Self::AroundPause
        )
    }
}

impl SpeechTimeline {
    /// Words without detected pauses.
    pub fn new(runs: Vec<SpeechRun>) -> Result<Self, EditError> {
        check_ranges(runs.iter().map(|run| run.range), runs.len(), "speech runs")?;
        Ok(Self {
            runs,
            ..Self::default()
        })
    }

    /// No words, for the given reason (shown when a word key is used).
    pub fn without_words(reason: impl Into<String>) -> Self {
        Self {
            words_missing: Some(reason.into()),
            ..Self::default()
        }
    }

    /// Add detected pauses: nonempty, sorted and disjoint.
    pub fn with_pauses(mut self, pauses: Vec<FrameRange>) -> Result<Self, EditError> {
        check_ranges(pauses.iter().copied(), pauses.len(), "pauses")?;
        self.pauses = Ok(pauses);
        Ok(self)
    }

    /// Record why pauses are unavailable.
    pub fn without_pauses(mut self, reason: impl Into<String>) -> Self {
        self.pauses = Err(reason.into());
        self
    }

    pub fn runs(&self) -> &[SpeechRun] {
        &self.runs
    }

    /// Detected pauses, when available.
    pub fn pauses(&self) -> Option<&[FrameRange]> {
        self.pauses.as_deref().ok()
    }

    /// Fail with the reason a unit's analysis is missing.
    pub fn require(&self, unit: SpeechUnit) -> Result<(), EditError> {
        match unit {
            SpeechUnit::Word | SpeechUnit::Sentence => match &self.words_missing {
                Some(reason) => Err(unavailable(reason)),
                None => Ok(()),
            },
            SpeechUnit::Pause => self
                .pauses
                .as_ref()
                .map(|_| ())
                .map_err(|reason| unavailable(reason)),
        }
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

    fn units(&self, bounds: (ProjectFrame, ProjectFrame), unit: SpeechUnit) -> Vec<FrameRange> {
        match unit {
            SpeechUnit::Word => self
                .clipped(bounds)
                .into_iter()
                .map(|run| run.range)
                .collect(),
            SpeechUnit::Sentence => Self::sentences(&self.clipped(bounds)),
            SpeechUnit::Pause => {
                let pauses = self.pauses.as_deref().unwrap_or_default();
                let first = pauses.partition_point(|pause| pause.end() <= bounds.0);
                pauses[first..]
                    .iter()
                    .take_while(|pause| pause.start() < bounds.1)
                    .filter_map(|pause| {
                        let start = pause.start().max(bounds.0);
                        let end = pause.end().min(bounds.1);
                        (start < end)
                            .then(|| FrameRange::new(start, end).ok())
                            .flatten()
                    })
                    .collect()
            }
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
        let units = self.units(
            bounds,
            if motion.sentence {
                SpeechUnit::Sentence
            } else {
                SpeechUnit::Word
            },
        );
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

    /// The word, sentence or pause at the picture after the cursor (or before
    /// it at the scope end). Around objects add handles from what adjoins it:
    /// half of each neighboring gap, capped at 80 ms, never reaching the next
    /// unit. A word's handles come from pauses; a pause's from speech.
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
        self.require(object.unit())?;
        let units = self.units(bounds, object.unit());
        let index = units.partition_point(|unit| unit.end() <= frame);
        let unit = units
            .get(index)
            .filter(|unit| unit.contains(frame))
            .copied()
            .ok_or_else(|| {
                unavailable(match object.unit() {
                    SpeechUnit::Word => "the cursor is not on a recognized word",
                    SpeechUnit::Sentence => "the cursor is not in a recognized sentence",
                    SpeechUnit::Pause => "the cursor is not in a pause",
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

    /// Destination of `]p` or `[p`: the start of the `count`th pause after
    /// (or before) the cursor. With none in that direction there is no
    /// motion; a shorter supply stops at the last pause found.
    pub fn pause_target(
        &self,
        cursor: ProjectFrame,
        bounds: (ProjectFrame, ProjectFrame),
        forward: bool,
        count: u32,
    ) -> Result<ProjectFrame, EditError> {
        self.require(SpeechUnit::Pause)?;
        let starts: Vec<ProjectFrame> = self
            .units(bounds, SpeechUnit::Pause)
            .into_iter()
            .map(|pause| pause.start())
            .collect();
        let count = usize::try_from(count.max(1)).unwrap_or(usize::MAX);
        let found = if forward {
            let first = starts.partition_point(|start| *start <= cursor);
            starts
                .get(first..)
                .and_then(|later| later.get(count.min(later.len()).checked_sub(1)?).copied())
        } else {
            let earlier = &starts[..starts.partition_point(|start| *start < cursor)];
            earlier
                .len()
                .checked_sub(count.min(earlier.len()))
                .and_then(|index| earlier.get(index))
                .copied()
        };
        found.ok_or_else(|| {
            unavailable(if forward {
                "there is no later pause here"
            } else {
                "there is no earlier pause here"
            })
        })
    }

    /// A Visual selection of a word, sentence or pause object, extending at
    /// its end.
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

fn check_ranges(
    ranges: impl Iterator<Item = FrameRange>,
    count: usize,
    what: &str,
) -> Result<(), EditError> {
    if count > MAX_SPEECH_RUNS {
        return Err(invalid(&format!("speech timeline has too many {what}")));
    }
    let mut previous: Option<FrameRange> = None;
    for range in ranges {
        if range.start() == range.end() {
            return Err(invalid(&format!("{what} must be nonempty")));
        }
        if previous.is_some_and(|previous| previous.end() > range.start()) {
            return Err(invalid(&format!("{what} must be sorted and disjoint")));
        }
        previous = Some(range);
    }
    Ok(())
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
