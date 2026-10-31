//! Adapter tests use existing raw event fixtures and a real native account.
//! Checked Root/provider/cache coverage belongs to separate native tests.
use super::*;
use deadpan_core::{ColorPolicy, PresentationBasis, ProjectDocument};

struct Trace;
impl Trace {
    fn new() -> Self {
        point_suppression::ops::trace::reset();
        Self
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        point_suppression::ops::trace::disable();
    }
}
fn owned_renderer() -> StageAudio {
    let document = ProjectDocument::new(
        ProjectId::new("root-mask-collector").unwrap(),
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

#[test]
fn empty_tag_survives_as_a_range_and_duplicate_events_keep_legacy_capacity() {
    for (spans, policy, emissions) in [
        (vec![span(4, 4, 1, 0, true)], Vec::new(), 1),
        (
            vec![
                span(4, 4, 1, 0, true),
                span(4, 6, 2, 0, true),
                span(6, 6, 1, 0, true),
            ],
            vec![AudioSample(4)..AudioSample(5)],
            4,
        ),
    ] {
        let queries = queries(4, 6, spans, policy);
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let mut expected_pcm = [[0.75, -0.5]; 2];
        let expected = apply(
            queries.as_borrowed(),
            AudioSample(4),
            &mut expected_pcm,
            control(&cancelled, &work),
        )
        .unwrap();
        assert_eq!(expected.suppressed.len(), emissions);
        let expected_ranges = merged_suppression(expected.suppressed);

        let mut renderer = owned_renderer();
        let mut pcm = [[0.75, -0.5]; 2];
        let PendingApplied {
            suppressed,
            exhausted,
        } = apply_owned(
            queries.as_borrowed(),
            AudioSample(4),
            &mut pcm,
            control(&cancelled, &work),
            &mut renderer.masks(control(&cancelled, &work)),
        )
        .unwrap();
        let PendingSuppression::Owned(Some(builder)) = suppressed else {
            panic!("actual emitted tags require a pending owned destination");
        };
        assert_eq!(builder.len(), emissions);
        assert_eq!(builder.capacity(), emissions);
        let pending_pointer = builder.as_slice().as_ptr();
        assert_eq!(exhausted, expected.exhausted);
        assert_eq!(
            pcm.map(|frame| frame.map(f32::to_bits)),
            expected_pcm.map(|frame| frame.map(f32::to_bits)),
        );
        let observer = renderer.owned_suppression.observer().unwrap();
        assert!(observer.live_frames() > point_suppression::shell_frames().unwrap());

        let unmerged = builder
            .finish(&renderer.masks(control(&cancelled, &work)))
            .unwrap();
        assert_eq!(unmerged.as_slice().as_ptr(), pending_pointer);
        assert_eq!(unmerged.len(), emissions);
        assert_eq!(unmerged.capacity(), emissions);
        let pointer = unmerged.as_slice().as_ptr();
        let result = unmerged
            .merge(&renderer.masks(control(&cancelled, &work)))
            .unwrap();
        assert!(result.is_owned());
        assert_eq!(result.as_slice(), expected_ranges.as_slice());
        assert_eq!(result.capacity(), expected_ranges.capacity());
        assert_eq!(result.capacity(), emissions);
        assert_eq!(
            result.as_slice().as_ptr(),
            pointer,
            "owned merge must not allocate a second Vec"
        );
        if emissions == 1 {
            assert_eq!(result.as_slice(), [AudioSample(4)..AudioSample(4)]);
        }
        let held_frames = observer.live_frames();
        drop(renderer);
        drop(work);
        assert_eq!(observer.live_frames(), held_frames);
        assert_eq!(result.as_slice(), expected_ranges.as_slice());
        drop(result);
        assert_eq!(
            observer.live_frames(),
            point_suppression::shell_frames().unwrap()
        );
    }
}

#[test]
fn actual_empty_collector_has_no_account_or_allocation_work() {
    let queries = queries(4, 6, Vec::new(), Vec::new());
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut renderer = owned_renderer();
    let mut pcm = [[0.75, -0.5]; 2];
    let _trace = Trace::new();
    let pending = apply_owned(
        queries.as_borrowed(),
        AudioSample(4),
        &mut pcm,
        control(&cancelled, &work),
        &mut renderer.masks(control(&cancelled, &work)),
    )
    .unwrap();
    let result = pending
        .suppressed
        .finish(&renderer.masks(control(&cancelled, &work)))
        .unwrap();
    assert!(result.is_empty() && !result.is_owned());
    assert_eq!(result.capacity(), 0);
    assert_eq!(pcm, [[0.75, -0.5]; 2]);
    assert!(renderer.owned_suppression.observer().is_none());
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    assert_eq!(work.borrow().plan_work, 0);
    let trace = serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
    for lane in ["plan", "cache"] {
        assert!(
            trace[lane]
                .as_object()
                .unwrap()
                .values()
                .all(|value| value.as_u64() == Some(0))
        );
    }
    assert_eq!(trace["overflow"], false);
}

#[test]
fn later_invalid_policy_preserves_zeroing_order_and_drops_pending_without_box() {
    let queries = queries(
        4,
        6,
        vec![span(4, 5, 1, 0, true)],
        vec![
            AudioSample(5)..AudioSample(6),
            AudioSample(3)..AudioSample(4),
        ],
    );
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut renderer = owned_renderer();
    let mut pcm = [[0.75, -0.5]; 2];
    let _trace = Trace::new();
    let result = apply_owned(
        queries.as_borrowed(),
        AudioSample(4),
        &mut pcm,
        control(&cancelled, &work),
        &mut renderer.masks(control(&cancelled, &work)),
    );
    assert!(matches!(result, Err(StageAudioError::Range)));
    assert_eq!(
        pcm,
        [[0.0, 0.0], [0.75, -0.5]],
        "whole policy validation precedes explicit-policy zeroing"
    );
    let observer = renderer.owned_suppression.observer().unwrap();
    assert_eq!(
        observer.live_frames(),
        point_suppression::shell_frames().unwrap()
    );
    let trace = serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
    assert_eq!(trace["plan"]["accepted_scan_units"], 1);
    assert_eq!(trace["plan"]["accepted_map_units"], 1);
    assert_eq!(trace["plan"]["accepted_vector_units"], 1);
    assert_eq!(trace["plan"]["accepted_storage_units"], 0);
    assert_eq!(trace["overflow"], false);
}

#[test]
fn root_only_override_leaves_point_policy_active() {
    assert!(!crate::stages::root_mask_request::legacy());
    assert!(!crate::stages::point_policy_request::legacy());
    crate::stages::root_mask_request::with_legacy_root_masks(|| {
        assert!(crate::stages::root_mask_request::legacy());
        assert!(!crate::stages::point_policy_request::legacy());
        crate::stages::root_mask_request::with_legacy_root_masks(|| {
            assert!(crate::stages::root_mask_request::legacy());
            assert!(!crate::stages::point_policy_request::legacy());
        });
        assert!(crate::stages::root_mask_request::legacy());
    });
    assert!(!crate::stages::root_mask_request::legacy());
    crate::stages::point_policy_request::with_legacy_policy(|| {
        assert!(crate::stages::root_mask_request::legacy());
        assert!(crate::stages::point_policy_request::legacy());
    });
    assert!(!crate::stages::root_mask_request::legacy());
    assert!(!crate::stages::point_policy_request::legacy());
}
