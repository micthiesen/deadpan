//! Project a stored Original transcript onto the Edit clock.
//!
//! Each project frame that presents an Original picture is assigned the word
//! being spoken during that picture: the latest word that begins before the
//! picture ends and has not ended when it starts. Consecutive frames of one
//! word in one occurrence form a run. Freezes, generated pictures, stills and
//! gaps carry no speech. Speech follows the picture mapping of linked beats.

use std::sync::Arc;

use deadpan_analysis::{Transcript, picture_seconds};
use deadpan_core::{
    AssetId, EditError, EditErrorCode, ExactRatio, FrameRange, ProjectDocument, ProjectFrame,
    SourceFrameIndex, SpeechRun, SpeechTimeline,
};
use deadpan_plan::{Picture, RenderPlan};

/// Speech runs for every frame of a compiled plan. A run continues while
/// adjacent frames show one word with source pictures that do not go back, so
/// a split or cut inside a word keeps one run and a replay starts another.
pub fn project_speech(
    plan: &RenderPlan,
    asset: &AssetId,
    index: &SourceFrameIndex,
    transcript: &Transcript,
) -> Result<SpeechTimeline, EditError> {
    let words = timed_words(transcript)?;
    let pictures = picture_words(index, &words)?;
    let mut runs: Vec<SpeechRun> = Vec::new();
    // The previous frame's word and source picture, if it showed speech.
    let mut previous: Option<(usize, usize)> = None;
    for frame in 0..plan.duration().frames() {
        let sample = plan
            .picture(ProjectFrame(frame))
            .map_err(|error| failed(&error.to_string()))?;
        let shown = match &sample.picture {
            Picture::Source { asset: shown, .. } if shown == asset => {
                let selected = sample
                    .picture
                    .select_source_frame(index)
                    .map_err(|error| failed(&error.to_string()))?;
                let picture = usize::try_from(selected.identity.0)
                    .map_err(|_| failed("source picture ordinal"))?;
                pictures
                    .get(picture)
                    .copied()
                    .ok_or_else(|| failed("source picture ordinal"))?
                    .map(|word| (word, picture))
            }
            _ => None,
        };
        let Some((word, picture)) = shown else {
            previous = None;
            continue;
        };
        let continues = previous.is_some_and(|(last, at)| last == word && picture >= at);
        if continues && let Some(run) = runs.last_mut() {
            run.range = FrameRange::new(run.range.start(), ProjectFrame(frame + 1))
                .map_err(|_| failed("speech run"))?;
        } else {
            runs.push(SpeechRun {
                range: FrameRange::new(ProjectFrame(frame), ProjectFrame(frame + 1))
                    .map_err(|_| failed("speech run"))?,
                word: u32::try_from(word).map_err(|_| failed("word index"))?,
                sentence: words[word].2,
            });
        }
        previous = Some((word, picture));
    }
    SpeechTimeline::new(runs)
}

/// The word spoken during each Original picture, computed once per plan.
fn picture_words(
    index: &SourceFrameIndex,
    words: &[(ExactRatio, ExactRatio, u32)],
) -> Result<Vec<Option<usize>>, EditError> {
    let terminal = ratio(index.terminal_end(), index)?;
    (0..index.frames().len())
        .map(|picture| {
            let start = picture_seconds(index, picture).ok_or_else(|| failed("picture"))?;
            let end = picture_seconds(index, picture + 1).unwrap_or(terminal);
            Ok(spoken(words, start, end))
        })
        .collect()
}

/// Speech over the Original's own pictures, for motions in Original context.
/// Frame numbers are source picture ordinals, so `ProjectFrame(k)` is picture
/// `k` of the index; every picture is one continuous occurrence.
pub fn original_speech(
    index: &SourceFrameIndex,
    transcript: &Transcript,
) -> Result<SpeechTimeline, EditError> {
    let words = timed_words(transcript)?;
    let mut runs: Vec<SpeechRun> = Vec::new();
    let mut previous = None;
    for (picture, word) in picture_words(index, &words)?.into_iter().enumerate() {
        let frame = i64::try_from(picture).map_err(|_| failed("picture ordinal"))?;
        match (word, previous) {
            (Some(word), Some(last)) if word == last => {
                let run = runs.last_mut().expect("a previous word has a run");
                run.range = FrameRange::new(run.range.start(), ProjectFrame(frame + 1))
                    .map_err(|_| failed("speech run"))?;
            }
            (Some(word), _) => runs.push(SpeechRun {
                range: FrameRange::new(ProjectFrame(frame), ProjectFrame(frame + 1))
                    .map_err(|_| failed("speech run"))?,
                word: u32::try_from(word).map_err(|_| failed("word index"))?,
                sentence: words[word].2,
            }),
            (None, _) => {}
        }
        previous = word;
    }
    SpeechTimeline::new(runs)
}

fn timed_words(transcript: &Transcript) -> Result<Vec<(ExactRatio, ExactRatio, u32)>, EditError> {
    transcript
        .words()
        .iter()
        .map(|word| {
            Ok((
                transcript.seconds(word.start_cs)?,
                transcript.seconds(word.end_cs)?,
                word.segment,
            ))
        })
        .collect::<Result<Vec<_>, deadpan_analysis::TranscriptError>>()
        .map_err(|error| failed(&error.to_string()))
}

/// The preferred stored transcript of one Original: an approved model's
/// English transcript first. Transcripts are rebuildable annotations, so an
/// unreadable one is skipped rather than failing its caller.
pub fn stored_transcript(
    store: &deadpan_store::ProjectStore,
    content: &str,
) -> Option<(deadpan_store::TranscriptKey, Transcript)> {
    let mut keys = store.transcript_keys_for_content(content).ok()?;
    let approved = deadpan_models::packs::approved_packs()
        .into_iter()
        .flat_map(|pack| pack.files.into_iter().map(|file| file.sha256))
        .collect::<Vec<_>>();
    keys.sort_by_key(|key| (!approved.contains(&key.model_sha256), key.language != "en"));
    keys.into_iter()
        .find_map(|key| Some((key.clone(), store.transcript(&key).ok()??)))
}

/// What a planner needs to place words: the ready Original, its qualified
/// picture index and its preferred transcript.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub struct StoredSpeech {
    pub asset: AssetId,
    /// The qualified picture index, bound to the project's asset identity.
    pub index: SourceFrameIndex,
    pub transcript: Transcript,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl StoredSpeech {
    /// Why words are unavailable when the project has no ready Original,
    /// qualified picture or readable transcript.
    pub fn load(store: &deadpan_store::ProjectStore) -> Result<Self, EditError> {
        use deadpan_store::single_source::SingleSourceState;
        let not_ready = |reason: &str| EditError {
            code: EditErrorCode::SelectionUnavailable,
            message: format!("words are not ready: {reason}"),
            current_revision: None,
        };
        let Ok(Some(SingleSourceState::Ready { asset, .. })) = store.single_source_state() else {
            return Err(not_ready("the project has no ready Original"));
        };
        let receipt = crate::transcription::analysed_receipt(store, None)
            .map_err(|error| not_ready(&error.to_string()))?;
        let measured = receipt
            .snapshot()
            .video()
            .ok_or_else(|| not_ready("the Original has no qualified picture"))?
            .index()
            .index();
        let index = SourceFrameIndex::new(
            asset.clone(),
            measured.time_base(),
            measured.frames().to_vec(),
            measured.terminal_end(),
            measured.terminal_provenance(),
        )
        .map_err(|error| not_ready(&error.to_string()))?;
        let (_, transcript) = stored_transcript(store, &receipt.original().content().to_string())
            .ok_or_else(deadpan_core::speech_unavailable)?;
        Ok(Self {
            asset,
            index,
            transcript,
        })
    }

    pub fn project(&self, document: &ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError> {
        project_document_speech(document, &self.asset, &self.index, &self.transcript)
    }
}

/// Compile a staged document and project speech through it.
pub fn project_document_speech(
    document: &ProjectDocument,
    asset: &AssetId,
    index: &SourceFrameIndex,
    transcript: &Transcript,
) -> Result<Arc<SpeechTimeline>, EditError> {
    let plan = RenderPlan::compile(document).map_err(|error| failed(&error.to_string()))?;
    project_speech(&plan, asset, index, transcript).map(Arc::new)
}

/// The latest word beginning before `end` that is still spoken at `start`.
fn spoken(
    words: &[(ExactRatio, ExactRatio, u32)],
    start: ExactRatio,
    end: ExactRatio,
) -> Option<usize> {
    let begun = words.partition_point(|(word_start, _, _)| word_start.compare(end).is_lt());
    let index = begun.checked_sub(1)?;
    words[index].1.compare(start).is_gt().then_some(index)
}

fn ratio(pts: i64, index: &SourceFrameIndex) -> Result<ExactRatio, EditError> {
    let base = index.time_base();
    ExactRatio::new(
        i128::from(pts) * i128::from(base.numerator()),
        i128::from(base.denominator()),
    )
    .map_err(|_| failed("terminal time"))
}

fn failed(reason: &str) -> EditError {
    EditError {
        code: EditErrorCode::SelectionUnavailable,
        message: format!("words could not be placed on the edit: {reason}"),
        current_revision: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(hundredths: i128) -> ExactRatio {
        ExactRatio::new(hundredths, 100).unwrap()
    }

    #[test]
    fn a_picture_shows_the_latest_word_spoken_during_it() {
        // Words at 0.10–0.50 and 0.60–0.90 s; pictures last 0.04 s.
        let words = [(seconds(10), seconds(50), 0), (seconds(60), seconds(90), 0)];
        let at = |start: i128| spoken(&words, seconds(start), seconds(start + 4));
        assert_eq!(at(0), None, "before speech");
        assert_eq!(at(8), Some(0), "a word beginning inside the picture");
        assert_eq!(at(40), Some(0));
        assert_eq!(at(52), None, "a pause");
        assert_eq!(at(48), Some(0), "still spoken when the picture starts");
        assert_eq!(
            at(58),
            Some(1),
            "the later word begins before the picture ends"
        );
        assert_eq!(at(90), None, "a word ending exactly at the picture start");
    }
}
