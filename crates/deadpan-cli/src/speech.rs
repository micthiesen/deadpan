//! Project a stored Original transcript and its detected pauses onto the Edit
//! clock.
//!
//! Each project frame that presents an Original picture is assigned the word
//! being spoken during that picture: the latest word that begins before the
//! picture ends and has not ended when it starts. Consecutive frames of one
//! word in one occurrence form a run. Freezes, generated pictures, stills and
//! gaps carry no speech. Speech follows the picture mapping of linked beats.
//!
//! A frame is quiet when it presents an Original picture lying wholly inside a
//! detected pause, or when it shows a freeze, generated picture, blank or
//! background, which carry no Original speech. Consecutive quiet frames form
//! one pause, so a pause inserted after a sentence lengthens the pause there.
//!
//! A frame belongs to a shot when it presents an Original picture: the shot
//! containing that picture. Consecutive frames of one shot form an occurrence
//! while their pictures do not go back, so a cut inside a shot keeps one
//! occurrence and a replay starts another.

use std::sync::Arc;

use deadpan_analysis::{SpeechActivity, Transcript, picture_seconds};
use deadpan_core::{
    AssetId, EditError, EditErrorCode, ExactRatio, FrameRange, ProjectDocument, ProjectFrame,
    ShotRun, SourceFrameIndex, SpeechRun, SpeechTimeline,
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

/// Detected pauses as exact container times, in order.
pub fn pause_seconds(
    activity: &SpeechActivity,
) -> Result<Vec<(ExactRatio, ExactRatio)>, EditError> {
    activity
        .pauses()
        .into_iter()
        .map(|pause| Ok((activity.seconds(pause.start)?, activity.seconds(pause.end)?)))
        .collect::<Result<Vec<_>, deadpan_analysis::ActivityError>>()
        .map_err(|error| failed(&error.to_string()))
}

/// Whether each Original picture lies wholly inside a pause.
fn quiet_pictures(
    index: &SourceFrameIndex,
    pauses: &[(ExactRatio, ExactRatio)],
) -> Result<Vec<bool>, EditError> {
    let terminal = ratio(index.terminal_end(), index)?;
    (0..index.frames().len())
        .map(|picture| {
            let start = picture_seconds(index, picture).ok_or_else(|| failed("picture"))?;
            let end = picture_seconds(index, picture + 1).unwrap_or(terminal);
            // The last pause beginning at or before the picture start.
            let begun =
                pauses.partition_point(|(pause_start, _)| pause_start.compare(start).is_le());
            Ok(begun
                .checked_sub(1)
                .is_some_and(|index| pauses[index].1.compare(end).is_ge()))
        })
        .collect()
}

/// Pauses on the Edit clock of a compiled plan.
pub fn project_pauses(
    plan: &RenderPlan,
    asset: &AssetId,
    index: &SourceFrameIndex,
    pauses: &[(ExactRatio, ExactRatio)],
) -> Result<Vec<FrameRange>, EditError> {
    let quiet = quiet_pictures(index, pauses)?;
    let mut ranges = Vec::new();
    let mut start: Option<i64> = None;
    let frames = plan.duration().frames();
    for frame in 0..frames {
        let sample = plan
            .picture(ProjectFrame(frame))
            .map_err(|error| failed(&error.to_string()))?;
        let is_quiet = match &sample.picture {
            Picture::Source { asset: shown, .. } if shown == asset => {
                let selected = sample
                    .picture
                    .select_source_frame(index)
                    .map_err(|error| failed(&error.to_string()))?;
                let picture = usize::try_from(selected.identity.0)
                    .map_err(|_| failed("source picture ordinal"))?;
                *quiet
                    .get(picture)
                    .ok_or_else(|| failed("source picture ordinal"))?
            }
            Picture::Freeze { .. }
            | Picture::Accepted { .. }
            | Picture::Blank
            | Picture::Background => true,
            _ => false,
        };
        match (is_quiet, start) {
            (true, None) => start = Some(frame),
            (false, Some(first)) => {
                ranges.push(frame_range(first, frame)?);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        ranges.push(frame_range(first, frames)?);
    }
    Ok(ranges)
}

/// Pauses over the Original's own pictures, numbered by picture ordinal.
pub fn original_pauses(
    index: &SourceFrameIndex,
    pauses: &[(ExactRatio, ExactRatio)],
) -> Result<Vec<FrameRange>, EditError> {
    let mut ranges = Vec::new();
    let mut start: Option<i64> = None;
    let quiet = quiet_pictures(index, pauses)?;
    let pictures = i64::try_from(quiet.len()).map_err(|_| failed("picture ordinal"))?;
    for (picture, is_quiet) in (0..pictures).zip(quiet) {
        match (is_quiet, start) {
            (true, None) => start = Some(picture),
            (false, Some(first)) => {
                ranges.push(frame_range(first, picture)?);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        ranges.push(frame_range(first, pictures)?);
    }
    Ok(ranges)
}

/// Shot occurrences on the Edit clock. `boundaries` are the Original
/// pictures that begin a shot after the first, in order.
pub fn project_shots(
    plan: &RenderPlan,
    asset: &AssetId,
    index: &SourceFrameIndex,
    boundaries: &[usize],
) -> Result<Vec<ShotRun>, EditError> {
    let mut runs: Vec<ShotRun> = Vec::new();
    let mut previous: Option<(u32, usize)> = None;
    for frame in 0..plan.duration().frames() {
        let sample = plan
            .picture(ProjectFrame(frame))
            .map_err(|error| failed(&error.to_string()))?;
        let Picture::Source { asset: shown, .. } = &sample.picture else {
            previous = None;
            continue;
        };
        if shown != asset {
            previous = None;
            continue;
        }
        let selected = sample
            .picture
            .select_source_frame(index)
            .map_err(|error| failed(&error.to_string()))?;
        let picture =
            usize::try_from(selected.identity.0).map_err(|_| failed("source picture ordinal"))?;
        let shot = u32::try_from(boundaries.partition_point(|start| *start <= picture))
            .map_err(|_| failed("shot index"))?;
        match (previous, runs.last_mut()) {
            (Some((last, at)), Some(run)) if last == shot && picture >= at => {
                run.range = frame_range(run.range.start().0, frame + 1)?;
            }
            _ => runs.push(ShotRun {
                range: frame_range(frame, frame + 1)?,
                shot,
            }),
        }
        previous = Some((shot, picture));
    }
    Ok(runs)
}

/// Shots over the Original's own pictures, numbered by picture ordinal.
pub fn original_shots(pictures: usize, boundaries: &[usize]) -> Result<Vec<ShotRun>, EditError> {
    let starts =
        std::iter::once(0).chain(boundaries.iter().copied().filter(|start| *start < pictures));
    let ends = boundaries
        .iter()
        .copied()
        .filter(|start| *start < pictures)
        .chain(std::iter::once(pictures));
    starts
        .zip(ends)
        .enumerate()
        .filter(|(_, (start, end))| start < end)
        .map(|(shot, (start, end))| {
            Ok(ShotRun {
                range: frame_range(
                    i64::try_from(start).map_err(|_| failed("picture ordinal"))?,
                    i64::try_from(end).map_err(|_| failed("picture ordinal"))?,
                )?,
                shot: u32::try_from(shot).map_err(|_| failed("shot index"))?,
            })
        })
        .collect()
}

fn frame_range(start: i64, end: i64) -> Result<FrameRange, EditError> {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).map_err(|_| failed("pause range"))
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

/// What a planner needs to place words and pauses: the ready Original, its
/// qualified picture index and whichever analyses are stored.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub struct StoredSpeech {
    pub asset: AssetId,
    /// The qualified picture index, bound to the project's asset identity.
    pub index: SourceFrameIndex,
    pub transcript: Option<Transcript>,
    /// Detected pauses as exact container times.
    pub pauses: Option<Vec<(ExactRatio, ExactRatio)>>,
    /// Original pictures that begin a shot after the first.
    pub shots: Option<Vec<usize>>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl StoredSpeech {
    /// Why speech is unavailable when the project has no ready Original or
    /// qualified picture. Missing analyses are reported when a key needs them.
    pub fn load(store: &deadpan_store::ProjectStore) -> Result<Self, EditError> {
        use deadpan_store::single_source::SingleSourceState;
        let not_ready = |reason: &str| EditError {
            code: EditErrorCode::SelectionUnavailable,
            message: format!("speech is not ready: {reason}"),
            current_revision: None,
        };
        let Ok(Some(SingleSourceState::Ready { asset, .. })) = store.single_source_state() else {
            return Err(not_ready("the project has no ready Original"));
        };
        let receipt = crate::transcription::analysed_receipt(store, None)
            .map_err(|error| not_ready(&error.to_string()))?;
        let qualified = receipt
            .snapshot()
            .video()
            .ok_or_else(|| not_ready("the Original has no qualified picture"))?
            .index();
        let measured = qualified.index();
        let index = SourceFrameIndex::new(
            asset.clone(),
            measured.time_base(),
            measured.frames().to_vec(),
            measured.terminal_end(),
            measured.terminal_provenance(),
        )
        .map_err(|error| not_ready(&error.to_string()))?;
        let content = receipt.original().content().to_string();
        let transcript = stored_transcript(store, &content).map(|(_, transcript)| transcript);
        let pauses = crate::activity::stored_activity(store, &content)
            .map(|(_, activity)| pause_seconds(&activity))
            .transpose()?;
        let shots = crate::shots::stored_shots(
            store,
            &content,
            qualified.stream_index(),
            index.frames().len(),
        )
        .map(|(_, analysis)| analysis.boundaries());
        Ok(Self {
            asset,
            index,
            transcript,
            pauses,
            shots,
        })
    }

    pub fn project(&self, document: &ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError> {
        let plan = RenderPlan::compile(document).map_err(|error| failed(&error.to_string()))?;
        let timeline = match &self.transcript {
            Some(transcript) => project_speech(&plan, &self.asset, &self.index, transcript)?,
            None => SpeechTimeline::without_words(deadpan_core::speech_unavailable().message),
        };
        let timeline = match &self.pauses {
            Some(pauses) => {
                timeline.with_pauses(project_pauses(&plan, &self.asset, &self.index, pauses)?)?
            }
            None => timeline.without_pauses(
                "pauses are not ready: the Original's speech has not been analysed",
            ),
        };
        Ok(Arc::new(match &self.shots {
            Some(boundaries) => {
                timeline.with_shots(project_shots(&plan, &self.asset, &self.index, boundaries)?)?
            }
            None => timeline.without_shots(
                "shots are not ready: the Original's pictures have not been analysed",
            ),
        }))
    }
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
        message: format!("speech could not be placed on the edit: {reason}"),
        current_revision: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(hundredths: i128) -> ExactRatio {
        ExactRatio::new(hundredths, 100).unwrap()
    }

    /// Ten pictures of 0.1 s each.
    fn pictures() -> SourceFrameIndex {
        use deadpan_core::{IndexedSourceFrame, SourceFrameId, SourceTimeBase, TerminalProvenance};
        SourceFrameIndex::new(
            AssetId::new("original").unwrap(),
            SourceTimeBase::new(1, 10).unwrap(),
            (0..10)
                .map(|number| IndexedSourceFrame {
                    identity: SourceFrameId(number),
                    pts: number as i64,
                    reported_duration: Some(1),
                    keyframe: true,
                    seek_from: None,
                    decode_timestamp: None,
                })
                .collect(),
            10,
            TerminalProvenance::DecodedFrameDuration,
        )
        .unwrap()
    }

    #[test]
    fn original_shots_split_pictures_at_boundaries() {
        let range = |start, end| FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap();
        let shots = original_shots(10, &[4, 7]).unwrap();
        assert_eq!(
            shots
                .iter()
                .map(|run| (run.range, run.shot))
                .collect::<Vec<_>>(),
            [(range(0, 4), 0), (range(4, 7), 1), (range(7, 10), 2)]
        );
        assert_eq!(original_shots(3, &[]).unwrap().len(), 1);
    }

    #[test]
    fn pictures_wholly_inside_a_pause_are_quiet() {
        // Pauses 0.15–0.45 s and 0.70 s to the end.
        let pauses = [(seconds(15), seconds(45)), (seconds(70), seconds(100))];
        let quiet = quiet_pictures(&pictures(), &pauses).unwrap();
        assert_eq!(
            quiet,
            [
                false, false, true, true, false, false, false, true, true, true
            ],
            "pictures 0.1–0.2 and 0.4–0.5 straddle the first pause's edges"
        );
        let range = |start, end| FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap();
        assert_eq!(
            original_pauses(&pictures(), &pauses).unwrap(),
            [range(2, 4), range(7, 10)]
        );
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
