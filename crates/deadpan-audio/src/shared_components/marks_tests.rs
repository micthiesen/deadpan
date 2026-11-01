//! Real renderer accounting around the private bounded marker algorithm.
//! Checked provider/native fixture reachability is tested separately.
use super::*;
use crate::stages::{MAX_PLAN_WORK_PER_READ, ReadWork, WorkControl};
use deadpan_core::{
    ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
};
use deadpan_plan::RenderPlan;
use std::cell::RefCell;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

fn renderer() -> StageAudio {
    let document = ProjectDocument::new(
        ProjectId::new("component-mark-boundaries").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(60, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    StageAudio::new(Arc::new(RenderPlan::compile(&document).unwrap()))
}

fn control<'a>(work: &'a RefCell<ReadWork>, cancelled: &'a AtomicBool) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work,
    }
}

fn marked(frames: usize, indices: &[usize], control: WorkControl<'_>) -> ComponentMarks {
    let mut marks = ComponentMarks::new(frames, control).unwrap();
    for &index in indices {
        marks.mark(index, control).unwrap();
    }
    marks
}

fn old_ranges(start: AudioSample, indices: &[usize]) -> Vec<Range<AudioSample>> {
    crate::stages::merged_suppression(
        indices
            .iter()
            .map(|&index| {
                let at = AudioSample(start.0 + index as i64);
                at..AudioSample(at.0 + 1)
            })
            .collect(),
    )
}

fn initialize_real_shell(renderer: &mut StageAudio, control: WorkControl<'_>) -> u64 {
    let seed = {
        let mut context = renderer.masks(control);
        let mut seed = begin_ranges::<AudioSample, _>(&mut context, 1).unwrap();
        seed.push(AudioSample(0)..AudioSample(1), &context).unwrap();
        seed.finish(&context).unwrap()
    };
    drop(seed);
    let live = renderer.owned_suppression.live_frames();
    assert!(live > 0);
    live
}

fn patterns(frames: usize) -> Vec<Vec<usize>> {
    vec![
        Vec::new(),
        (0..frames).collect(),
        vec![frames - 1],
        (0..frames).step_by(2).collect(),
        [
            61, 62, 63, 64, 65, 66, 125, 126, 127, 128, 129, 130, 189, 190, 191, 192, 193, 194,
            253, 254, 255,
        ]
        .into_iter()
        .filter(|&index| index < frames)
        .collect(),
    ]
}

#[test]
fn inline_marks_match_old_singleton_merge_at_word_boundaries() {
    for frames in [1, 63, 64, 65, 127, 128, 129, 255, 256] {
        for mut indices in patterns(frames) {
            // Bit order must follow sample labels, not insertion order.
            indices.reverse();
            for start in [AudioSample(-513), AudioSample(11)] {
                let mut renderer = renderer();
                let work = RefCell::new(ReadWork::default());
                let cancelled = AtomicBool::new(false);
                let control = control(&work, &cancelled);
                let marks = marked(frames, &indices, control);
                assert_eq!(marks.frames(), frames);
                assert_eq!(work.borrow().plan_work, WORDS + indices.len());
                let expected = old_ranges(start, &indices);
                let mask = marks.finish(start, &mut renderer.masks(control)).unwrap();
                assert_eq!(
                    mask.as_slice(),
                    expected.as_slice(),
                    "frames={frames}, start={start:?}, indices={indices:?}",
                );
                assert_eq!(mask.capacity(), indices.len());
                assert_eq!(mask.is_owned(), !indices.is_empty());
                if indices.is_empty() {
                    assert_eq!(renderer.owned_suppression.live_frames(), 0);
                    assert!(renderer.owned_suppression.observer().is_none());
                } else {
                    let observer = renderer.owned_suppression.observer().unwrap();
                    let held = observer.live_frames();
                    drop(mask);
                    assert!(observer.live_frames() < held);
                }
            }
        }
    }
}

#[test]
fn empty_inline_marks_spend_only_bounded_word_work_and_create_no_account() {
    for frames in [1, 63, 64, 65, 127, 128, 129, 255, 256] {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&work, &cancelled);
        let marks = ComponentMarks::new(frames, control).unwrap();
        assert!(marks.test_words().iter().all(|word| *word == 0));
        let mask = marks
            .finish(AudioSample(-257), &mut renderer.masks(control))
            .unwrap();
        assert!(mask.is_empty());
        assert_eq!(mask.capacity(), 0);
        assert!(!mask.is_owned());
        assert_eq!(work.borrow().plan_work, WORDS * 2);
        assert_eq!(renderer.owned_suppression.live_frames(), 0);
        assert!(renderer.owned_suppression.observer().is_none());
    }
}

#[test]
fn invalid_frame_and_mark_bounds_change_no_bit_or_account() {
    let renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    for frames in [0, MAX_OUTPUT_FRAMES as usize + 1, usize::MAX] {
        assert!(matches!(
            ComponentMarks::new(frames, control),
            Err(StageAudioError::Range)
        ));
        assert_eq!(work.borrow().plan_work, 0);
    }
    let mut marks = ComponentMarks::new(65, control).unwrap();
    let before = work.borrow().plan_work;
    for index in [65, 256, usize::MAX] {
        assert!(matches!(
            marks.mark(index, control),
            Err(StageAudioError::Range)
        ));
        assert_eq!(work.borrow().plan_work, before);
        assert!(marks.test_words().iter().all(|word| *word == 0));
    }
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    assert!(renderer.owned_suppression.observer().is_none());
}

#[test]
fn refused_and_cancelled_marks_do_not_publish_a_bit() {
    for cancelled_case in [false, true] {
        let renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&work, &cancelled);
        let mut marks = ComponentMarks::new(256, control).unwrap();
        // Exhausted prior read work is injected exactly as existing ops tests
        // do; this is a budget-state test, not measured DSP work.
        if cancelled_case {
            cancelled.store(true, Ordering::Relaxed);
        } else {
            work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ;
        }
        let before = work.borrow().plan_work;
        let result = marks.mark(255, control);
        if cancelled_case {
            assert!(matches!(result, Err(error) if error.is_cancelled()));
        } else {
            assert!(matches!(
                result,
                Err(StageAudioError::Limit("plan work per read"))
            ));
        }
        assert_eq!(work.borrow().plan_work, before);
        assert!(marks.test_words().iter().all(|word| *word == 0));
        assert_eq!(renderer.owned_suppression.live_frames(), 0);
    }
}

#[test]
fn refused_inline_initialization_creates_no_account_or_partial_debit() {
    let renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let prior = MAX_PLAN_WORK_PER_READ - (WORDS - 1);
    work.borrow_mut().plan_work = prior;
    assert!(matches!(
        ComponentMarks::new(256, control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(work.borrow().plan_work, prior);
    assert!(renderer.owned_suppression.observer().is_none());
}

#[test]
fn dense_runs_retain_raw_capacity_and_outlive_work_and_controller() {
    let (mask, observer, held, shell) = {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&work, &cancelled);
        let shell = initialize_real_shell(&mut renderer, control);
        let indices = (0..256).collect::<Vec<_>>();
        let marks = marked(256, &indices, control);
        let mask = marks
            .finish(AudioSample(-257), &mut renderer.masks(control))
            .unwrap();
        assert_eq!(mask.as_slice(), [AudioSample(-257)..AudioSample(-1)]);
        assert_eq!(mask.capacity(), 256);
        let observer = renderer.owned_suppression.observer().unwrap();
        let held = observer.live_frames();
        assert!(held > shell);
        (mask, observer, held, shell)
    };
    assert_eq!(observer.live_frames(), held);
    assert_eq!(mask.as_slice(), [AudioSample(-257)..AudioSample(-1)]);
    assert_eq!(mask.capacity(), 256);
    drop(mask);
    assert_eq!(observer.live_frames(), shell);
}

fn observed_small_finish_units() -> usize {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let shell = initialize_real_shell(&mut renderer, control);
    let marks = marked(129, &[0, 1, 63, 64, 65, 128], control);
    let before = work.borrow().plan_work;
    let result = marks
        .finish(AudioSample(-193), &mut renderer.masks(control))
        .unwrap();
    assert_eq!(
        result.as_slice(),
        [
            AudioSample(-193)..AudioSample(-191),
            AudioSample(-130)..AudioSample(-127),
            AudioSample(-65)..AudioSample(-64),
        ],
    );
    let units = work.borrow().plan_work - before;
    drop(result);
    assert_eq!(renderer.owned_suppression.live_frames(), shell);
    assert!(units > 0 && units < 128);
    units
}

#[test]
fn each_small_finish_work_prefix_stops_and_cleans_actual_payload() {
    let units = observed_small_finish_units();
    for remaining in 0..units {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&work, &cancelled);
        let shell = initialize_real_shell(&mut renderer, control);
        let marks = marked(129, &[0, 1, 63, 64, 65, 128], control);
        let injected = MAX_PLAN_WORK_PER_READ - remaining;
        assert!(injected >= work.borrow().plan_work);
        work.borrow_mut().plan_work = injected;
        let result = marks.finish(AudioSample(-193), &mut renderer.masks(control));
        assert!(matches!(
            result,
            Err(StageAudioError::Limit("plan work per read"))
        ));
        assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        assert_eq!(renderer.owned_suppression.live_frames(), shell);
    }
}
