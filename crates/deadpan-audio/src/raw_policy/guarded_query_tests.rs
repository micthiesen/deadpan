//! Actual admitted Root metadata overlaps a separately funded native collector.
//! These isolate destination residency after source preparation, not a full read.
use super::*;
use deadpan_core::{
    BeatNode, ColorPolicy, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, PresentationBasis,
    ProjectDocument,
};

struct NativeTrace;
impl NativeTrace {
    fn new() -> Self {
        point_suppression::ops::trace::reset();
        Self
    }

    fn snapshot(&self) -> serde_json::Value {
        serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap()
    }
}
impl Drop for NativeTrace {
    fn drop(&mut self) {
        point_suppression::ops::trace::disable();
    }
}

fn checked_silent_root() -> Arc<RenderPlan> {
    let document = ProjectDocument::new(
        ProjectId::new("guarded-root-mask-overlap").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "Root",
        vec![NodeId::new("quiet").unwrap()],
    ))
    .unwrap();
    wire["nodes"]["quiet"] = serde_json::to_value(BeatNode::hold(
        "Silence",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(64).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    let checked = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = Arc::new(RenderPlan::compile(&checked).unwrap());
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(64));
    plan
}

fn emitted_capacity(queries: RootReadQueriesRef<'_, '_>) -> usize {
    assert_eq!(
        queries.policy.suppressed.as_slice(),
        [AudioSample(0)..AudioSample(64)]
    );
    let silent_tags = queries
        .flattened
        .spans
        .iter()
        .filter(|span| is_silent_hold(&span.content))
        .count();
    assert_eq!(silent_tags, 1);
    silent_tags + queries.policy.suppressed.len()
}

fn native_frames(capacity: usize) -> (u64, u64) {
    (
        point_suppression::shell_frames().unwrap(),
        point_suppression::audio_payload_frames(capacity).unwrap(),
    )
}

fn isolated_destination_ceiling(
    renderer: &StageAudio,
    work: &RefCell<ReadWork>,
    source_frames: u64,
    capacity: usize,
) -> u64 {
    let cached = renderer.cached_resident_frames();
    let active = renderer.active_frames;
    let projected = work.borrow().projected_resident_frames;
    let owned = renderer.owned_suppression.live_frames();
    assert_eq!(cached, 0);
    assert_eq!(active, 0);
    assert_eq!(projected, 0);
    assert_eq!(owned, 0);
    assert_eq!(work.borrow().sound_context_frames, source_frames);
    let (shell, payload) = native_frames(capacity);
    cached + active + projected + source_frames + owned + shell + payload
}

#[test]
fn checked_guarded_root_policy_retains_source_and_exact_destination_through_merge() {
    let plan = checked_silent_root();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let request = renderer
        .prepare_original_root(&plan, AudioSample(0), 64, control, false)
        .unwrap();
    request.validate_issuer(control).unwrap();
    let source_frames = work.borrow().sound_context_frames;
    assert!(source_frames > 0);
    let queries = request.queries();
    let source_pointer = queries.policy.suppressed.as_ptr();
    let source_capacity = queries.policy.suppressed.capacity();
    assert!(source_capacity > 0);
    let capacity = emitted_capacity(queries);
    let (shell, payload) = native_frames(capacity);
    let exact = isolated_destination_ceiling(&renderer, &work, source_frames, capacity);
    assert!(exact < u64::from(renderer.limits.maximum_resident_frames));
    renderer.limits.maximum_resident_frames = u32::try_from(exact).unwrap();

    let trace = NativeTrace::new();
    let mut samples = [[1.0, -0.5]; 64];
    let PendingApplied {
        suppressed,
        exhausted,
    } = apply_owned(
        queries,
        AudioSample(0),
        &mut samples,
        control,
        &mut renderer.masks(control),
    )
    .unwrap();
    assert!(exhausted.is_empty());
    assert_eq!(samples, [[0.0; 2]; 64]);
    let PendingSuppression::Owned(Some(builder)) = suppressed else {
        panic!("actual checked SilentHold must allocate its native destination");
    };
    assert_eq!(builder.len(), capacity);
    assert_eq!(builder.capacity(), capacity);
    let destination_pointer = builder.as_slice().as_ptr();
    assert_ne!(source_pointer, destination_pointer);
    assert_eq!(request.queries().policy.suppressed.as_ptr(), source_pointer);
    assert_eq!(
        request.queries().policy.suppressed.capacity(),
        source_capacity
    );
    assert_eq!(work.borrow().sound_context_frames, source_frames);
    let observer = renderer.owned_suppression.observer().unwrap();
    assert_eq!(observer.live_frames(), shell + payload);
    assert_eq!(
        source_frames + observer.live_frames(),
        u64::from(renderer.limits.maximum_resident_frames)
    );

    let unmerged = builder.finish(&renderer.masks(control)).unwrap();
    assert_eq!(unmerged.as_slice().as_ptr(), destination_pointer);
    assert_eq!(unmerged.len(), capacity);
    assert_eq!(unmerged.capacity(), capacity);
    assert_eq!(work.borrow().sound_context_frames, source_frames);
    assert_eq!(observer.live_frames(), shell + payload);
    let result = unmerged.merge(&renderer.masks(control)).unwrap();
    assert_eq!(result.as_slice(), [AudioSample(0)..AudioSample(64)]);
    assert_eq!(result.as_slice().as_ptr(), destination_pointer);
    assert_eq!(result.capacity(), capacity);
    assert_eq!(work.borrow().sound_context_frames, source_frames);
    assert_eq!(request.queries().policy.suppressed.as_ptr(), source_pointer);
    assert_eq!(observer.live_frames(), shell + payload);

    let accepted = trace.snapshot();
    assert_eq!(accepted["overflow"], false);
    assert_eq!(accepted["plan"]["initialized_account_shells"], 1);
    assert_eq!(accepted["plan"]["initialized_account_frames"], shell);
    assert_eq!(accepted["plan"]["accepted_payload_reservations"], 1);
    assert_eq!(accepted["plan"]["accepted_payload_frames"], payload);
    assert_eq!(accepted["plan"]["accepted_vector_units"], 1);
    assert_eq!(accepted["plan"]["accepted_storage_units"], 1);

    drop(request);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(observer.live_frames(), shell + payload);
    drop(renderer);
    drop(work);
    assert_eq!(result.as_slice(), [AudioSample(0)..AudioSample(64)]);
    assert_eq!(result.as_slice().as_ptr(), destination_pointer);
    assert_eq!(observer.live_frames(), shell + payload);
    drop(result);
    assert_eq!(observer.live_frames(), shell);
}

#[test]
fn checked_query_isolated_destination_ceiling_and_one_below_are_exact() {
    for below in [false, true] {
        let plan = checked_silent_root();
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let request = renderer
            .prepare_original_root(&plan, AudioSample(0), 64, control, false)
            .unwrap();
        let source_frames = work.borrow().sound_context_frames;
        assert!(source_frames > 0);
        let capacity = emitted_capacity(request.queries());
        let (shell, payload) = native_frames(capacity);
        assert!(payload > 1);
        let exact = isolated_destination_ceiling(&renderer, &work, source_frames, capacity);
        assert!(exact < u64::from(renderer.limits.maximum_resident_frames));
        renderer.limits.maximum_resident_frames = u32::try_from(exact - u64::from(below)).unwrap();

        let trace = NativeTrace::new();
        let mut samples = [[1.0, -0.5]; 64];
        let actual = apply_owned(
            request.queries(),
            AudioSample(0),
            &mut samples,
            control,
            &mut renderer.masks(control),
        );
        let observer = renderer.owned_suppression.observer().unwrap();
        if below {
            assert!(matches!(
                actual,
                Err(StageAudioError::Limit("resident PCM or stage cache"))
            ));
            assert_eq!(observer.live_frames(), shell);
            let accepted = trace.snapshot();
            assert_eq!(accepted["plan"]["initialized_account_shells"], 1);
            assert_eq!(accepted["plan"]["accepted_payload_reservations"], 0);
            assert_eq!(accepted["plan"]["accepted_vector_units"], 0);
            assert_eq!(accepted["plan"]["accepted_storage_units"], 0);
        } else {
            let PendingApplied {
                suppressed,
                exhausted,
            } = actual.unwrap();
            assert!(exhausted.is_empty());
            assert_eq!(observer.live_frames(), shell + payload);
            let result = suppressed.finish(&renderer.masks(control)).unwrap();
            assert_eq!(result.capacity(), capacity);
            assert_eq!(result.as_slice(), [AudioSample(0)..AudioSample(64)]);
            assert_eq!(work.borrow().sound_context_frames, source_frames);
            assert_eq!(observer.live_frames(), shell + payload);
            drop(result);
            assert_eq!(observer.live_frames(), shell);
        }
        assert_eq!(work.borrow().sound_context_frames, source_frames);
        assert_eq!(renderer.cached_resident_frames(), 0);
        assert_eq!(renderer.active_frames, 0);
        drop(request);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(observer.live_frames(), shell);
    }
}

#[test]
fn pending_cancellation_and_expiry_dispose_payload_before_releasing_guarded_source() {
    for cancel in [true, false] {
        let plan = checked_silent_root();
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let request = renderer
            .prepare_original_root(&plan, AudioSample(0), 64, control, false)
            .unwrap();
        let source_frames = work.borrow().sound_context_frames;
        assert!(source_frames > 0);
        let capacity = emitted_capacity(request.queries());
        let source_pointer = request.queries().policy.suppressed.as_ptr();
        let (shell, payload) = native_frames(capacity);
        let trace = NativeTrace::new();
        let mut samples = [[1.0, -0.5]; 64];
        let PendingApplied {
            suppressed,
            exhausted,
        } = apply_owned(
            request.queries(),
            AudioSample(0),
            &mut samples,
            control,
            &mut renderer.masks(control),
        )
        .unwrap();
        assert!(exhausted.is_empty());
        let PendingSuppression::Owned(Some(builder)) = &suppressed else {
            panic!("cancellation boundary must retain an actual pending destination");
        };
        assert_eq!(builder.len(), capacity);
        assert_eq!(builder.capacity(), capacity);
        assert_ne!(builder.as_slice().as_ptr(), source_pointer);
        let observer = renderer.owned_suppression.observer().unwrap();
        assert_eq!(observer.live_frames(), shell + payload);
        assert_eq!(work.borrow().sound_context_frames, source_frames);
        assert_eq!(trace.snapshot()["plan"]["accepted_storage_units"], 0);

        let refused = if cancel {
            cancelled.store(true, Ordering::Relaxed);
            suppressed.finish(&renderer.masks(control))
        } else {
            let expired = WorkControl {
                deadline: Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
                ..control
            };
            suppressed.finish(&renderer.masks(expired))
        };
        if cancel {
            assert!(matches!(refused, Err(error) if error.is_cancelled()));
        } else {
            assert!(matches!(refused, Err(StageAudioError::Timeout)));
        }
        assert_eq!(observer.live_frames(), shell);
        assert_eq!(trace.snapshot()["plan"]["accepted_storage_units"], 0);
        assert_eq!(work.borrow().sound_context_frames, source_frames);
        assert_eq!(request.queries().policy.suppressed.as_ptr(), source_pointer);
        drop(request);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(observer.live_frames(), shell);
    }
}
