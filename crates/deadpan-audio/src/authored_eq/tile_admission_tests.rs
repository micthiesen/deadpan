//! OLD semantic matrix with explicit, independently checked new resource fees.
use super::*;
use std::alloc::Layout;

// The actual frozen Node fields, with only its old tile container restored.
// This is a shallow layout witness, never an estimate of BTreeMap node bytes.
#[allow(dead_code)]
struct OldNodeLayout<'plan, 'work> {
    parent: usize,
    context: Context<'plan>,
    boundary: deadpan_plan::AdmittedAudioTreatmentBoundary<
        'plan,
        sound_context_admission::Issuer<'work>,
        sound_context_admission::Lease<'work>,
    >,
    filter: Option<crate::Equalizer>,
    tiles: BTreeMap<i64, Arc<WideBlock>>,
}

fn arc_frames() -> u64 {
    // Independent Arc shell calculation from the pinned two-counter layout.
    let (layout, _) = Layout::new::<[usize; 2]>()
        .extend(Layout::new::<WideBlock>())
        .unwrap();
    u64::try_from(layout.pad_to_align().size())
        .unwrap()
        .div_ceil(8)
}

#[test]
fn pending_arc_admits_exact_shell_once_and_clones_share_its_lease() {
    let plan = super::eq_admission_fixture::fixture();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = super::eq_admission_fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(plan);
    let value = WideBlock {
        samples: Vec::new(),
        suppressed: AudioSuppressionMask::default(),
        dependencies: Dependencies::new(),
        relative_depth: 0,
    };
    let pending = tile_block::PendingArc::new(&mut renderer, value, control).unwrap();
    assert_eq!(pending.frames(), arc_frames());
    assert_eq!(work.borrow().sound_context_frames, arc_frames());
    let borrowed = pending.value.as_ref().unwrap();
    let clone = Arc::clone(borrowed);
    assert_eq!(Arc::strong_count(borrowed), 2);
    assert_eq!(work.borrow().sound_context_frames, arc_frames());
    drop(clone);
    drop(pending);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn pending_arc_drops_value_before_lease_and_refusal_returns_no_live_charge() {
    struct Probe<'a> {
        ledger: &'a RefCell<ReadWork>,
        seen: &'a std::cell::Cell<Option<u64>>,
    }
    impl Drop for Probe<'_> {
        fn drop(&mut self) {
            self.seen
                .set(Some(self.ledger.borrow().sound_context_frames));
        }
    }
    let plan = super::eq_admission_fixture::fixture();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = super::eq_admission_fixture::control(&cancelled, &work);
    let seen = std::cell::Cell::new(None);
    let mut renderer = StageAudio::new(plan);
    renderer.limits.maximum_resident_frames = 0;
    assert!(matches!(
        tile_block::PendingArc::new(
            &mut renderer,
            Probe {
                ledger: &work,
                seen: &seen
            },
            control
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(seen.get(), Some(0), "refused input is physically dropped");
    assert_eq!(work.borrow().sound_context_frames, 0);
    renderer.limits = StageLimits::default();
    seen.set(None);
    let pending = tile_block::PendingArc::new(
        &mut renderer,
        Probe {
            ledger: &work,
            seen: &seen,
        },
        control,
    )
    .unwrap();
    let charge = pending.frames();
    assert!(charge > 0);
    drop(pending);
    assert_eq!(
        seen.get(),
        Some(charge),
        "value destructor observes its still-live lease"
    );
    assert_eq!(work.borrow().sound_context_frames, 0);
}

fn graph<'plan, 'work>(plan: &'plan RenderPlan) -> Graph<'plan, 'plan, 'work> {
    Graph {
        plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        skip: &[],
    }
}

fn add<'plan, 'work>(
    renderer: &mut StageAudio,
    graph: &mut Graph<'plan, '_, 'work>,
    owner: &AudioOwnerClock<'plan>,
    control: WorkControl<'work>,
) -> usize {
    renderer
        .eq_node(
            graph,
            Program {
                context: &Context::Root,
                stop: None,
                parent: usize::MAX,
                depth: 0,
            },
            owner,
            AudioSample(128),
            super::eq_admission_fixture::stage(owner),
            control,
        )
        .unwrap()
}

#[test]
fn actual_graph_late_publication_work_refusal_restores_filter_and_retries() {
    let plan = super::eq_admission_fixture::fixture();
    let owner = super::eq_admission_fixture::owner(&plan);
    let cancelled = AtomicBool::new(false);
    // First observe the real complete cold call under ample headroom. For this
    // empty index its only new operation is the final new-entry write.
    let measurement = RefCell::new(ReadWork::default());
    let control = super::eq_admission_fixture::control(&cancelled, &measurement);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut measured_graph = graph(&plan);
    let id = add(&mut renderer, &mut measured_graph, &owner, control);
    let before = measurement.borrow().plan_work;
    let (result, fee) = tile_index::with_fee_trace(|| {
        renderer.eq_tile_with_index_work::<true>(
            &mut super::eq_admission_fixture::NoSources(0),
            &mut measured_graph,
            id,
            0,
            control,
            1,
        )
    });
    let block = result.unwrap();
    assert_eq!(fee, 1);
    assert_eq!(Arc::strong_count(&block), 2);
    let required = measurement.borrow().plan_work.checked_sub(before).unwrap();
    let prepared = measurement.borrow().prepared_frames;
    drop(block);
    let resident = measured_graph.resident;
    drop(measured_graph);
    renderer.active_frames -= resident;
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(measurement.borrow().sound_context_frames, 0);

    let work = RefCell::new(ReadWork::default());
    let retry_work = RefCell::new(ReadWork::default());
    let control = super::eq_admission_fixture::control(&cancelled, &work);
    let retry_control = super::eq_admission_fixture::control(&cancelled, &retry_work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut actual_graph = graph(&plan);
    let id = add(&mut renderer, &mut actual_graph, &owner, control);
    let active = renderer.active_frames;
    let resident = actual_graph.resident;
    // An explicit initial resource input leaves every old charge available,
    // and refuses precisely the final one-unit new-entry publication fee.
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ.checked_sub(required).unwrap() + 1;
    let initial_work = work.borrow().plan_work;
    let mut provider = super::eq_admission_fixture::NoSources(0);
    let (refusal, accepted) = tile_index::with_fee_trace(|| {
        renderer.eq_tile_with_index_work::<true>(
            &mut provider,
            &mut actual_graph,
            id,
            0,
            control,
            1,
        )
    });
    assert!(matches!(
        refusal,
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(accepted, 0);
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    assert!(
        work.borrow().plan_work > initial_work,
        "accepted old work is retained"
    );
    assert_eq!(work.borrow().prepared_frames, prepared);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.active_frames, active);
    assert_eq!(actual_graph.resident, resident);
    assert!(actual_graph.nodes[id].tiles.is_empty());
    assert_eq!(actual_graph.nodes[id].tiles.capacity(), 0);
    assert!(actual_graph.nodes[id].filter.is_some());
    assert_eq!(provider.0, 0);
    // Retry gets a fresh explicit request budget; the exhausted budget and its
    // accepted work remain untouched. The existing Graph/filter are reused.
    let retried = renderer
        .eq_tile_with_index_work::<true>(&mut provider, &mut actual_graph, id, 0, retry_control, 1)
        .unwrap();
    assert_eq!(Arc::strong_count(&retried), 2);
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    drop(retried);
    let resident = actual_graph.resident;
    drop(actual_graph);
    renderer.active_frames -= resident;
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(retry_work.borrow().sound_context_frames, 0);
}
