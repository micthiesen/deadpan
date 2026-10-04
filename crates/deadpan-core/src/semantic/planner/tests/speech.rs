use super::*;
use crate::{SpeechObject, SpeechRun};
use std::cell::Cell;

fn run(start: i64, end: i64, word: u32, sentence: u32) -> SpeechRun {
    SpeechRun {
        range: range(start, end),
        word,
        sentence,
    }
}
fn words(starts: &[(i64, i64)]) -> Arc<SpeechTimeline> {
    Arc::new(
        SpeechTimeline::new(
            starts
                .iter()
                .enumerate()
                .map(|(index, (start, end))| run(*start, *end, index as u32, index as u32 / 2))
                .collect(),
        )
        .unwrap(),
    )
}
fn count(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap()
}
fn plan_speech(
    document: &ProjectDocument,
    context: SemanticContext,
    instructions: Vec<SemanticInstruction>,
    speech: impl FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_speech(
        document,
        &context,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 7,
        },
        revision("outer"),
        allocate,
        no_original,
        speech,
        |_, _| Err(crate::pause_unavailable()),
    )
}
fn word_motion(forward: bool, value: u32, end: bool) -> SemanticInstruction {
    SemanticInstruction::MoveWords {
        forward,
        count: count(value),
        end,
    }
}

#[test]
fn word_and_sentence_motions_follow_supplied_speech() {
    let document = fixture(100);
    let speech = words(&[(10, 20), (25, 30), (40, 50), (50, 55)]);
    let moved = |from: i64, instruction: SemanticInstruction| {
        plan_speech(&document, context("root", from), vec![instruction], |_| {
            Ok(Arc::clone(&speech))
        })
        .unwrap()
        .context
        .cursor
        .0
    };
    assert_eq!(moved(0, word_motion(true, 1, false)), 10);
    assert_eq!(moved(0, word_motion(true, 3, false)), 40);
    assert_eq!(moved(45, word_motion(false, 1, false)), 40);
    assert_eq!(moved(12, word_motion(true, 1, true)), 20);
    let sentence = |forward| SemanticInstruction::MoveSentences {
        forward,
        count: count(1),
    };
    assert_eq!(moved(12, sentence(true)), 40);
    assert_eq!(moved(45, sentence(false)), 40);
    assert_eq!(moved(90, word_motion(true, 1, false)), 100);
}

#[test]
fn word_objects_cut_exact_ranges_and_speech_follows_the_staged_edit() {
    let document = fixture(100);
    let calls = Cell::new(0);
    let planned = plan_speech(
        &document,
        context("root", 12),
        vec![
            SemanticInstruction::Cut {
                selector: SemanticSelector::Speech {
                    object: SpeechObject::InnerWord,
                },
                register: name('a'),
            },
            word_motion(true, 1, false),
        ],
        |staged| {
            calls.set(calls.get() + 1);
            // The host projects the transcript through each staged document.
            Ok(if staged.duration().unwrap().frames() == 100 {
                words(&[(10, 20), (25, 30)])
            } else {
                words(&[(15, 20)])
            })
        },
    )
    .unwrap();
    assert_eq!(planned.trace[0].resolved_range, Some(range(10, 20)));
    assert_eq!(planned.document.duration().unwrap().frames(), 90);
    assert_eq!(planned.context.cursor, ProjectFrame(15));
    assert_eq!(calls.get(), 2);

    let deleted = plan_speech(
        &document,
        context("root", 12),
        vec![SemanticInstruction::Cut {
            selector: SemanticSelector::Motion {
                motion: SemanticMotion::Words {
                    forward: true,
                    count: count(1),
                    end: false,
                },
            },
            register: name('a'),
        }],
        |_| Ok(words(&[(10, 20), (25, 30)])),
    )
    .unwrap();
    assert_eq!(deleted.trace[0].resolved_range, Some(range(12, 25)));
}

#[test]
fn around_words_select_visual_time_with_handles() {
    let document = fixture(100);
    let planned = plan_speech(
        &document,
        context("root", 12),
        vec![SemanticInstruction::SelectSpeech {
            object: SpeechObject::AroundWord,
        }],
        |_| Ok(words(&[(10, 20), (25, 30)])),
    )
    .unwrap();
    assert_eq!(
        planned.context.visual_selection,
        Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(8),
            head: ProjectFrame(22),
            extending: true,
        })
    );
    assert_eq!(planned.context.cursor, ProjectFrame(22));
    assert_eq!(planned.trace[0].resolved_range, Some(range(8, 22)));
}

#[test]
fn speech_is_requested_only_when_needed_and_explains_its_absence() {
    let document = fixture(20);
    plan_speech(&document, context("root", 0), vec![motion(true, 2)], |_| {
        panic!("frame motions never project speech")
    })
    .unwrap();
    let error = plan(
        &document,
        context("root", 0),
        vec![word_motion(true, 1, false)],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("transcript"), "{error:?}");
    assert!(SemanticProgram::new(vec![word_motion(false, 1, true)]).is_err());
}

#[test]
fn pause_motions_and_objects_resolve_without_words() {
    let document = fixture(100);
    let speech = Arc::new(
        SpeechTimeline::without_words("words are not ready: no transcript")
            .with_pauses(vec![range(20, 30), range(60, 70)])
            .unwrap(),
    );
    let planned = plan_speech(
        &document,
        context("root", 0),
        vec![
            SemanticInstruction::MovePauses {
                forward: true,
                count: count(2),
            },
            SemanticInstruction::Cut {
                selector: SemanticSelector::Speech {
                    object: SpeechObject::InnerPause,
                },
                register: name('a'),
            },
        ],
        |_| Ok(Arc::clone(&speech)),
    )
    .unwrap();
    assert_eq!(planned.trace[0].resolved_range, None);
    assert_eq!(planned.trace[1].resolved_range, Some(range(60, 70)));
    assert_eq!(planned.document.duration().unwrap().frames(), 90);

    let error = plan_speech(
        &document,
        context("root", 0),
        vec![word_motion(true, 1, false)],
        |_| Ok(Arc::clone(&speech)),
    )
    .unwrap_err();
    assert_eq!(error.message, "words are not ready: no transcript");
}
