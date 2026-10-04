use super::*;

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn run(start: i64, end: i64, word: u32, sentence: u32) -> SpeechRun {
    SpeechRun {
        range: range(start, end),
        word,
        sentence,
    }
}

/// Two sentences: words 0 and 1, then words 2 and 3 back to back.
fn timeline() -> SpeechTimeline {
    SpeechTimeline::new(vec![
        run(10, 20, 0, 0),
        run(25, 30, 1, 0),
        run(40, 50, 2, 1),
        run(50, 55, 3, 1),
    ])
    .unwrap()
}

const BOUNDS: (ProjectFrame, ProjectFrame) = (ProjectFrame(0), ProjectFrame(100));

fn motion(forward: bool, count: u32, sentence: bool, end: bool) -> SpeechMotion {
    SpeechMotion {
        forward,
        count,
        sentence,
        end,
    }
}

fn target(from: i64, motion: SpeechMotion) -> i64 {
    timeline()
        .motion_target(ProjectFrame(from), BOUNDS, motion)
        .0
}

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

#[test]
fn word_motions_step_between_starts_and_ends_and_clamp_to_the_scope() {
    let w = motion(true, 1, false, false);
    assert_eq!(
        [0, 10, 25, 40, 50, 100].map(|from| target(from, w)),
        [10, 25, 40, 50, 100, 100]
    );
    assert_eq!(target(0, motion(true, 3, false, false)), 40);
    assert_eq!(target(12, w), 25, "inside a word, w goes to the next word");
    let b = motion(false, 1, false, false);
    assert_eq!([45, 40, 25, 5].map(|from| target(from, b)), [40, 25, 10, 0]);
    assert_eq!(target(100, motion(false, 2, false, false)), 40);
    let e = motion(true, 1, false, true);
    assert_eq!(
        [0, 20, 26, 55].map(|from| target(from, e)),
        [20, 30, 30, 100]
    );
}

#[test]
fn sentence_motions_use_sentence_starts() {
    let forward = motion(true, 1, true, false);
    assert_eq!([0, 10, 40].map(|from| target(from, forward)), [10, 40, 100]);
    let back = motion(false, 1, true, false);
    assert_eq!([45, 40, 10].map(|from| target(from, back)), [40, 10, 0]);
}

#[test]
fn inner_objects_are_exact_and_pauses_hold_no_word() {
    let timeline = timeline();
    let object =
        |at: i64, object| timeline.object_range(ProjectFrame(at), BOUNDS, object, thirty());
    assert_eq!(object(12, SpeechObject::InnerWord).unwrap(), range(10, 20));
    assert_eq!(
        object(26, SpeechObject::InnerSentence).unwrap(),
        range(10, 30)
    );
    assert_eq!(
        object(22, SpeechObject::InnerSentence).unwrap(),
        range(10, 30)
    );
    let pause = object(22, SpeechObject::InnerWord).unwrap_err();
    assert_eq!(pause.code, EditErrorCode::SelectionUnavailable);
    assert!(object(70, SpeechObject::AroundSentence).is_err());
}

#[test]
fn around_objects_take_half_of_each_pause_capped_at_eighty_milliseconds() {
    let timeline = timeline();
    let around = |at: i64, object, rate| {
        timeline
            .object_range(ProjectFrame(at), BOUNDS, object, rate)
            .unwrap()
    };
    // At 30 fps the cap is two frames; the pause after word 0 is five frames.
    assert_eq!(around(12, SpeechObject::AroundWord, thirty()), range(8, 22));
    // Abutting speech contributes no handle on that side.
    assert_eq!(
        around(50, SpeechObject::AroundWord, thirty()),
        range(50, 57)
    );
    assert_eq!(
        around(45, SpeechObject::AroundWord, thirty()),
        range(38, 50)
    );
    assert_eq!(
        around(26, SpeechObject::AroundSentence, thirty()),
        range(8, 32)
    );
    // A short pause gives half of itself: five frames at 120 fps allow nine.
    let fast = FrameRate::new(120, 1).unwrap();
    assert_eq!(around(26, SpeechObject::AroundWord, fast), range(23, 35));
}

#[test]
fn replayed_sentences_are_separate_and_scopes_clip_runs() {
    let replayed = SpeechTimeline::new(vec![
        run(0, 5, 0, 0),
        run(5, 10, 1, 0),
        run(12, 17, 0, 0),
        run(17, 20, 1, 0),
    ])
    .unwrap();
    let rate = thirty();
    let bounds = (ProjectFrame(0), ProjectFrame(20));
    assert_eq!(
        replayed
            .object_range(ProjectFrame(13), bounds, SpeechObject::InnerSentence, rate)
            .unwrap(),
        range(12, 20)
    );
    let scope = (ProjectFrame(12), ProjectFrame(45));
    let timeline = timeline();
    assert_eq!(
        timeline
            .motion_target(ProjectFrame(12), scope, motion(true, 1, false, false))
            .0,
        25
    );
    assert_eq!(
        timeline
            .object_range(ProjectFrame(12), scope, SpeechObject::InnerWord, rate)
            .unwrap(),
        range(12, 20)
    );
    // At the scope end the object is the picture before the cursor.
    assert_eq!(
        timeline
            .object_range(ProjectFrame(45), scope, SpeechObject::InnerWord, rate)
            .unwrap(),
        range(40, 45)
    );
}

#[test]
fn timelines_reject_empty_overlapping_and_unsorted_runs() {
    assert!(SpeechTimeline::new(vec![run(0, 0, 0, 0)]).is_err());
    assert!(SpeechTimeline::new(vec![run(0, 10, 0, 0), run(5, 12, 1, 0)]).is_err());
    assert!(SpeechTimeline::new(vec![run(10, 12, 0, 0), run(0, 5, 1, 0)]).is_err());
    assert!(SpeechTimeline::new(Vec::new()).unwrap().runs().is_empty());
}

/// The words of [`timeline`] with pauses 0..10, 30..40 and 55..100.
fn paused() -> SpeechTimeline {
    timeline()
        .with_pauses(vec![range(0, 10), range(30, 40), range(55, 100)])
        .unwrap()
}

#[test]
fn pause_motions_go_to_pause_starts_and_report_when_there_are_none() {
    let timeline = paused();
    let target = |from: i64, forward: bool, count: u32| {
        timeline
            .pause_target(ProjectFrame(from), BOUNDS, forward, count)
            .map(|frame| frame.0)
    };
    assert_eq!(
        target(0, true, 1).unwrap(),
        30,
        "a pause at the cursor is skipped"
    );
    assert_eq!(target(12, true, 2).unwrap(), 55);
    assert_eq!(
        target(12, true, 9).unwrap(),
        55,
        "a long count stops at the last pause"
    );
    assert_eq!(target(45, false, 1).unwrap(), 30);
    assert_eq!(target(30, false, 1).unwrap(), 0);
    let none = target(60, true, 1).unwrap_err();
    assert_eq!(none.code, EditErrorCode::SelectionUnavailable);
    assert!(none.message.contains("no later pause"), "{none:?}");
    assert!(target(0, false, 1).is_err());
    // A scope clips pauses: the pause beginning before it starts at its edge.
    let scope = (ProjectFrame(35), ProjectFrame(60));
    assert_eq!(
        timeline
            .pause_target(ProjectFrame(50), scope, false, 1)
            .unwrap(),
        ProjectFrame(35)
    );
}

#[test]
fn pause_objects_select_the_pause_with_handles_of_adjoining_speech() {
    let timeline = paused();
    let object =
        |at: i64, object| timeline.object_range(ProjectFrame(at), BOUNDS, object, thirty());
    assert_eq!(object(33, SpeechObject::InnerPause).unwrap(), range(30, 40));
    // Two frames (80 ms at 30 fps) of the words on each side.
    assert_eq!(
        object(33, SpeechObject::AroundPause).unwrap(),
        range(28, 42)
    );
    // The first pause starts the scope; the last ends it.
    assert_eq!(object(0, SpeechObject::AroundPause).unwrap(), range(0, 12));
    assert_eq!(
        object(100, SpeechObject::InnerPause).unwrap(),
        range(55, 100)
    );
    let speaking = object(12, SpeechObject::InnerPause).unwrap_err();
    assert!(speaking.message.contains("not in a pause"), "{speaking:?}");
}

#[test]
fn words_and_pauses_explain_their_own_absence() {
    let words_only = timeline();
    let error = words_only
        .object_range(ProjectFrame(33), BOUNDS, SpeechObject::InnerPause, thirty())
        .unwrap_err();
    assert!(error.message.contains("pauses are not ready"), "{error:?}");
    let pauses_only = SpeechTimeline::without_words("words are not ready: no model")
        .with_pauses(vec![range(30, 40)])
        .unwrap();
    assert_eq!(
        pauses_only
            .pause_target(ProjectFrame(0), BOUNDS, true, 1)
            .unwrap(),
        ProjectFrame(30)
    );
    let error = pauses_only
        .object_range(ProjectFrame(33), BOUNDS, SpeechObject::InnerWord, thirty())
        .unwrap_err();
    assert_eq!(error.message, "words are not ready: no model");
    assert!(
        timeline()
            .with_pauses(vec![range(0, 10), range(5, 12)])
            .is_err()
    );
    assert!(timeline().with_pauses(vec![range(4, 4)]).is_err());
}
