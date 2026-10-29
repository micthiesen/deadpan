//! Frozen numerical observations plus actual request/Graph admission witnesses.
use super::eq_admission_fixture as fixture;
use super::legacy_node_baseline as baseline;
use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use serde_json::{Value, json};
use std::alloc::Layout;
use std::sync::atomic::Ordering;

const BASE: u64 = 17;
const GOLDEN: &str = include_str!("../../tests/fixtures/authored-node-baseline-v1/snapshot.json");

fn golden() -> Value {
    serde_json::from_str(GOLDEN).unwrap()
}

fn request<'a, 'plan>(
    incoming: &'a Context<'plan>,
    owner: &'a AudioOwnerClock<'plan>,
    at: AudioSample,
    stage: usize,
) -> node_description::Request<'a, 'plan> {
    node_description::Request {
        incoming,
        owner,
        at,
        stage,
    }
}

fn boundary<I: Copy + Eq, L>(
    plan: &RenderPlan,
    value: &deadpan_plan::AdmittedAudioTreatmentBoundary<'_, I, L>,
    owner: &AudioOwnerClock<'_>,
    at: AudioSample,
) -> Value {
    json!({
        "before_stage": value.before_stage(),
        "owner": baseline::clock(plan, value.owner(), at),
        "matches_source_owner": value.matches(owner),
        "matches_source_clock_at": value.matches_clock_at(owner, at),
    })
}

#[test]
fn false_descriptions_preserve_frozen_fields_errors_and_canonical_work() {
    let old = golden();
    let actual = baseline::cases(|plan, incoming, owner, at, stage, _, owner_limit| {
        if owner_limit == Some(0) {
            return Value::Null; // The renderer's count check precedes Description.
        }
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let result = node_description::describe(
            plan,
            request(incoming, owner, at, stage),
            fixture::control(&cancelled, &work),
        );
        let output = match result {
            Ok(value) => json!({
                "context": baseline::context(plan, &value.context, at),
                "boundary": boundary(plan, &value.boundary, owner, at),
            }),
            Err(error) => json!({"error": format!("{error:?}")}),
        };
        json!({"result":output, "canonical":work.borrow().plan_work})
    });
    assert_eq!(actual.len(), 13);
    for (actual, expected) in actual.iter().zip(old["cases"].as_array().unwrap()) {
        assert_eq!(actual["name"], expected["name"]);
        if actual["observation"].is_null() {
            continue;
        }
        let old = &expected["observation"];
        let observed = &actual["observation"];
        let canonical = old["controller_plan_work"].as_u64().unwrap();
        if old["result"].get("error").is_some() {
            assert_eq!(observed["result"], old["result"], "{}", actual["name"]);
            assert_eq!(
                observed["canonical"],
                json!(canonical),
                "{}",
                actual["name"]
            );
        } else {
            assert_eq!(
                observed["result"]["context"], old["result"]["context"],
                "{}",
                actual["name"]
            );
            assert_eq!(
                observed["result"]["boundary"], old["result"]["boundary"],
                "{}",
                actual["name"]
            );
            assert_eq!(
                observed["canonical"],
                json!(
                    canonical
                        .checked_sub(deadpan_dsp::FINITE_FILTER_TAPS as u64)
                        .unwrap()
                ),
                "{}",
                actual["name"]
            );
        }
    }
}

#[test]
fn actual_nodes_preserve_all_thirteen_frozen_observations_and_cache_hits() {
    let old = golden();
    let actual = baseline::cases(baseline::measured);
    let retained_receipts = independent_retained_receipts();
    assert_eq!(actual.len(), 13);
    for (actual, expected) in actual.iter().zip(old["cases"].as_array().unwrap()) {
        assert_eq!(actual["name"], expected["name"]);
        let mut observed = actual["observation"].clone();
        let expected = &expected["observation"];
        if actual["name"] == "live_owner_actual_retained_probe_selection" {
            // Only these three actual bound/probe receipts change under true
            // accounting. Prove each against an independent direct capture and
            // probe before comparing every other frozen field literally.
            for (path, canonical, metered) in &retained_receipts {
                assert_eq!(expected.pointer(path).unwrap(), canonical, "{path}");
                assert_eq!(observed.pointer(path).unwrap(), metered, "{path}");
                let old = canonical.as_u64().unwrap();
                let total = metered.as_u64().unwrap();
                let additional = total.checked_sub(old).unwrap();
                assert_eq!(old.checked_add(additional).unwrap(), total);
                *observed.pointer_mut(path).unwrap() = canonical.clone();
            }
        }
        for field in [
            "actual_eq_node_calls",
            "incoming_context",
            "source_owner",
            "observation",
            "requested_stage",
            "requested_parent",
            "native_filter_taps",
            "result",
            "cache_lookup",
            "after_graph_drop",
        ] {
            assert_eq!(
                observed.get(field),
                expected.get(field),
                "{}: {field}",
                actual["name"]
            );
        }
        assert_eq!(observed["accounting"]["request_metadata"], json!(0));
        assert_eq!(
            observed["accounting"]["node_count"],
            expected["accounting"]["node_count"]
        );
        if expected["result"].get("error").is_none() {
            assert!(
                observed["accounting"]["graph_resident"].as_u64().unwrap()
                    > expected["accounting"]["graph_resident"].as_u64().unwrap()
            );
            assert_eq!(
                observed["accounting"]["active_frames"],
                observed["accounting"]["graph_resident"]
            );
        }
    }
}

/// The returned fields are obtained from the actual legacy and admitted probe
/// graphs, not inferred from the node under test or globally ignored by name.
fn independent_retained_receipts() -> Vec<(&'static str, Value, Value)> {
    let plan = baseline::frozen("hidden_resumed_retained_domain_vs_live_root", "group");
    let owner = baseline::observed(&plan, "group", 1, false);
    let at = AudioSample(1);
    let legacy_input = plan
        .audio_owner_input(&owner, at, Default::default())
        .unwrap();
    let legacy_probe = legacy_input
        .retained_probe_at(at, Default::default())
        .unwrap();
    let legacy = legacy_probe.input().unwrap();
    let legacy_bound = serde_json::to_value(legacy.bound()).unwrap();

    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
    let input = plan
        .audio_owner_input_admitted(&owner, at, control.query_limits().unwrap(), &mut admission)
        .unwrap();
    let input_canonical = input
        .query_canonical_units()
        .checked_add(input.capture_canonical_units())
        .unwrap();
    let input_additional = input
        .query_additional_units()
        .checked_add(input.capture_additional_units())
        .unwrap();
    assert_eq!(input_canonical, legacy_input.work());
    assert_eq!(
        input.work(),
        input_canonical.checked_add(input_additional).unwrap()
    );
    let probe = input
        .input()
        .unwrap()
        .retained_probe_at_admitted(at, control.query_limits().unwrap(), &mut admission)
        .unwrap();
    assert_eq!(probe.canonical_units(), legacy_probe.work());
    assert_eq!(
        probe.work(),
        probe
            .canonical_units()
            .checked_add(probe.additional_units())
            .unwrap()
    );
    assert_eq!(
        work.borrow().plan_work,
        input.work().checked_add(probe.work()).unwrap()
    );
    let metered = probe.input().unwrap();
    let metered_bound = serde_json::to_value(metered.bound()).unwrap();
    let receipts = vec![
        (
            "/result/context/bound/resolved/lattice/work",
            legacy_bound["resolved"]["lattice"]["work"].clone(),
            metered_bound["resolved"]["lattice"]["work"].clone(),
        ),
        (
            "/result/context/bound/resolved/work",
            legacy_bound["resolved"]["work"].clone(),
            metered_bound["resolved"]["work"].clone(),
        ),
        (
            "/result/context/retained_lookup_work",
            json!(legacy.work()),
            json!(metered.work()),
        ),
    ];
    let paid = work.borrow().plan_work;
    drop(probe);
    drop(input);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(work.borrow().plan_work, paid);
    receipts
}

#[test]
fn actual_authored_pcm_bits_and_full_output_metadata_match_frozen_native_renders() {
    let old = golden();
    let root = fixture::fixture();
    let repeat = baseline::frozen("stable_repeat_plays_default_gap_and_overrides", "voice");
    for (index, (plan, at, frames)) in [(&root, 128, 8), (&root, 508, 8), (&repeat, 0, 16)]
        .into_iter()
        .enumerate()
    {
        let actual = baseline::render(plan, at, frames);
        let expected = &old["tone_renders"][index]["value"];
        assert_eq!(actual["output"], expected["output"]);
        assert_eq!(actual["provider_reads"], expected["provider_reads"]);
        assert_eq!(actual["accounting"]["request_metadata"], json!(0));
        assert_eq!(actual["accounting"]["active_frames"], json!(0));
    }
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
fn capacity_frames<T>(capacity: usize) -> u64 {
    u64::try_from(Layout::array::<T>(capacity).unwrap().size())
        .unwrap()
        .div_ceil(8)
}
fn release(renderer: &mut StageAudio, graph: Graph<'_, '_, '_>) {
    let retained = graph.resident;
    drop(graph);
    renderer.active_frames -= retained;
}
fn add<'plan, 'work>(
    renderer: &mut StageAudio,
    graph: &mut Graph<'plan, 'plan, 'work>,
    owner: &AudioOwnerClock<'plan>,
    parent: usize,
    control: WorkControl<'work>,
) -> Result<usize, StageAudioError> {
    renderer.eq_node(
        graph,
        Program {
            context: &Context::Root,
            stop: None,
            parent,
            depth: 0,
        },
        owner,
        AudioSample(0),
        fixture::stage(owner),
        control,
    )
}

pub(super) fn metadata_units(plan: &Arc<RenderPlan>) -> u64 {
    let owner = fixture::owner(plan);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let description = {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        node_description::describe_admitted(
            plan,
            request(
                &Context::Root,
                &owner,
                AudioSample(128),
                fixture::stage(&owner),
            ),
            control,
            &mut admission,
        )
        .unwrap()
    };
    let boundary = description
        .boundary
        .lease()
        .map_or(0, sound_context_admission::Lease::frames);
    let original = description.original.as_ref().map_or(0, |value| {
        value
            .leases()
            .into_iter()
            .flatten()
            .map(sound_context_admission::Lease::frames)
            .sum::<u64>()
    });
    let metadata = boundary.checked_add(original).unwrap();
    assert_eq!(metadata, work.borrow().sound_context_frames);
    drop(description);
    assert_eq!(work.borrow().sound_context_frames, 0);
    metadata
        .checked_add(capacity_frames::<Node<'_, '_>>(4))
        .unwrap()
}

#[test]
fn graph_transfers_metadata_capacity_and_native_once_then_uses_spare_and_growth() {
    let plan = baseline::frozen("stable_repeat_plays_default_gap_and_overrides", "voice");
    let owner = baseline::observed(&plan, "voice", 0, false);
    assert!(!owner.instance().repeats.is_empty());
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    renderer.active_frames = BASE;
    let mut graph = graph(&plan);
    let mut per_node = None;
    for parent in 0..5 {
        let before_resident = graph.resident;
        let old_capacity = graph.node_capacity_frames;
        assert_eq!(
            add(&mut renderer, &mut graph, &owner, parent, control).unwrap(),
            parent
        );
        let expected_capacity = if parent < 4 { 4 } else { 8 };
        assert_eq!(graph.nodes.capacity(), expected_capacity);
        assert_eq!(
            graph.node_capacity_frames,
            capacity_frames::<Node<'_, '_>>(expected_capacity)
        );
        let metadata_and_native =
            graph.resident + old_capacity - before_resident - graph.node_capacity_frames;
        if let Some(expected) = per_node {
            assert_eq!(metadata_and_native, expected);
        } else {
            assert!(metadata_and_native > fixture::units());
            per_node = Some(metadata_and_native);
        }
        let node = &graph.nodes[parent];
        assert_eq!(node.boundary.lease().unwrap().frames(), 0);
        assert!(node.filter.is_some() && node.tiles.is_empty());
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(renderer.active_frames, BASE + graph.resident);
        let before = work.borrow().plan_work;
        let resident = graph.resident;
        cancelled.store(true, Ordering::Relaxed);
        assert_eq!(
            add(&mut renderer, &mut graph, &owner, parent, control).unwrap(),
            parent,
            "cache hit precedes cancellation and fresh metadata"
        );
        cancelled.store(false, Ordering::Relaxed);
        assert_eq!(work.borrow().plan_work, before);
        assert_eq!(graph.resident, resident);
    }
    release(&mut renderer, graph);
    assert_eq!(renderer.active_frames, BASE);
}

#[test]
fn replacement_capacity_overlap_and_late_move_refusals_preserve_the_published_graph() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let cancelled = AtomicBool::new(false);
    // Independently measure the fifth-node complete fee and its existing four moves.
    let probe_work = RefCell::new(ReadWork::default());
    let probe_control = fixture::control(&cancelled, &probe_work);
    let mut probe = StageAudio::new(Arc::clone(&plan));
    let mut measured = graph(&plan);
    for parent in 0..4 {
        add(&mut probe, &mut measured, &owner, parent, probe_control).unwrap();
    }
    let before = probe_work.borrow().plan_work;
    add(&mut probe, &mut measured, &owner, 4, probe_control).unwrap();
    let fifth_fee = probe_work.borrow().plan_work - before;
    release(&mut probe, measured);
    for refuse_capacity in [true, false] {
        let work = RefCell::new(ReadWork::default());
        let control = fixture::control(&cancelled, &work);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        renderer.active_frames = BASE;
        let mut graph = graph(&plan);
        for parent in 0..4 {
            add(&mut renderer, &mut graph, &owner, parent, control).unwrap();
        }
        let resident = graph.resident;
        let capacity = graph.node_capacity_frames;
        let active = renderer.active_frames;
        if refuse_capacity {
            let metadata = (resident - capacity - 4 * fixture::units()) / 4;
            let overlap = active + metadata + capacity_frames::<Node<'_, '_>>(8);
            renderer.limits.maximum_resident_frames = u32::try_from(overlap - 1).unwrap();
        } else {
            work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - fifth_fee + 1;
        }
        assert!(matches!(
            add(&mut renderer, &mut graph, &owner, 4, control),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.nodes.capacity(), 4);
        assert_eq!(graph.node_capacity_frames, capacity);
        assert_eq!(graph.resident, resident);
        assert_eq!(renderer.active_frames, active);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert!(
            graph
                .nodes
                .iter()
                .all(|node| node.filter.is_some() && node.tiles.is_empty())
        );
        renderer.limits.maximum_resident_frames = StageLimits::default().maximum_resident_frames;
        work.borrow_mut().plan_work = 0;
        add(&mut renderer, &mut graph, &owner, 4, control).unwrap();
        assert_eq!(graph.nodes.len(), 5);
        release(&mut renderer, graph);
        assert_eq!(renderer.active_frames, BASE);
    }
}

struct Trace<'controller, 'work> {
    inner: sound_context_admission::Admission<'controller, 'work>,
    control: WorkControl<'work>,
    cancelled: &'work AtomicBool,
    reservations: usize,
    spends: usize,
    fail_reservation: Option<usize>,
    cancel_spend: Option<usize>,
    accepted: usize,
}
impl<'controller, 'work> Trace<'controller, 'work> {
    fn new(
        renderer: &'controller mut StageAudio,
        control: WorkControl<'work>,
        cancelled: &'work AtomicBool,
    ) -> Self {
        Self {
            inner: sound_context_admission::Admission::new(renderer, control),
            control,
            cancelled,
            reservations: 0,
            spends: 0,
            fail_reservation: None,
            cancel_spend: None,
            accepted: 0,
        }
    }
}
impl<'work> AudioSoundContextAdmission for Trace<'_, 'work> {
    type Error = StageAudioError;
    type Issuer = sound_context_admission::Issuer<'work>;
    type Lease = sound_context_admission::Lease<'work>;
    fn issuer(&self) -> Self::Issuer {
        self.inner.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), Self::Error> {
        self.spends += 1;
        if self.cancel_spend == Some(self.spends) {
            self.cancelled.store(true, Ordering::Relaxed);
        }
        self.inner.spend(units)?;
        self.accepted = self.accepted.checked_add(units).unwrap();
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, Self::Error> {
        self.reservations += 1;
        self.control.check()?;
        if self.fail_reservation == Some(self.reservations) {
            return Err(StageAudioError::Limit("Authored allocation sentinel"));
        }
        self.inner.reserve(layout)
    }
    fn merge(&mut self, target: &mut Self::Lease, source: &mut Self::Lease) {
        self.inner.merge(target, source);
    }
}

#[test]
fn every_actual_description_reservation_and_spend_cancels_or_refuses_without_leaking() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let stage = fixture::stage(&owner);
    let cancelled = AtomicBool::new(false);
    let success_work = RefCell::new(ReadWork::default());
    let success_control = fixture::control(&cancelled, &success_work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let (reservations, spends) = {
        let mut trace = Trace::new(&mut renderer, success_control, &cancelled);
        let description = node_description::describe_admitted(
            &plan,
            request(&Context::Root, &owner, AudioSample(128), stage),
            success_control,
            &mut trace,
        )
        .unwrap();
        let reservations = trace.reservations;
        let spends = trace.spends;
        assert!(reservations > 0 && spends > 0);
        assert_eq!(trace.accepted, success_work.borrow().plan_work);
        assert!(success_work.borrow().sound_context_frames > 0);
        drop(description);
        assert_eq!(success_work.borrow().sound_context_frames, 0);
        assert_eq!(
            trace.accepted,
            success_work.borrow().plan_work,
            "accepted work is never refunded"
        );
        (reservations, spends)
    };
    for (reservation, spend) in (1..=reservations)
        .map(|n| (Some(n), None))
        .chain((1..=spends).map(|n| (None, Some(n))))
    {
        cancelled.store(false, Ordering::Relaxed);
        let work = RefCell::new(ReadWork::default());
        let control = fixture::control(&cancelled, &work);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        renderer.active_frames = BASE;
        {
            let mut trace = Trace::new(&mut renderer, control, &cancelled);
            trace.fail_reservation = reservation;
            trace.cancel_spend = spend;
            let result = node_description::describe_admitted(
                &plan,
                request(&Context::Root, &owner, AudioSample(128), stage),
                control,
                &mut trace,
            );
            assert!(result.is_err(), "reserve {reservation:?}, spend {spend:?}");
            drop(result);
            assert_eq!(work.borrow().sound_context_frames, 0);
            assert_eq!(
                work.borrow().plan_work,
                trace.accepted,
                "no accepted prefix refunded"
            );
        }
        assert_eq!(renderer.active_frames, BASE);
    }
}

#[test]
fn root_total_reconciles_independent_canonical_receipts_and_additional_fees() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let stage = fixture::stage(&owner);
    let at = AudioSample(128);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
    let input = plan
        .audio_owner_input_admitted(&owner, at, control.query_limits().unwrap(), &mut admission)
        .unwrap();
    let canonical_input = input
        .query_canonical_units()
        .checked_add(input.capture_canonical_units())
        .unwrap();
    let additional_input = input
        .query_additional_units()
        .checked_add(input.capture_additional_units())
        .unwrap();
    assert_eq!(
        input.work(),
        canonical_input.checked_add(additional_input).unwrap()
    );
    let probe = input
        .input()
        .unwrap()
        .retained_probe_at_admitted(at, control.query_limits().unwrap(), &mut admission)
        .unwrap();
    assert!(probe.input().is_none(), "actual positive None fallback");
    let canonical = canonical_input
        .checked_add(probe.canonical_units())
        .unwrap();
    let additional = additional_input
        .checked_add(probe.additional_units())
        .unwrap();
    assert_eq!(
        work.borrow().plan_work,
        canonical.checked_add(additional).unwrap()
    );
    drop(probe);
    let before_sharing = work.borrow().plan_work;
    let shared = input
        .input()
        .unwrap()
        .context()
        .clone_shared_admitted(control.query_limits().unwrap(), &mut admission)
        .unwrap();
    let sharing_fee = work.borrow().plan_work.checked_sub(before_sharing).unwrap();
    assert!(sharing_fee > 0);
    let boundary = owner
        .treatment_boundary_admitted(stage, control.query_limits().unwrap(), &mut admission)
        .unwrap();
    assert_eq!(boundary.canonical_units(), 0);
    let additional = additional
        .checked_add(sharing_fee)
        .unwrap()
        .checked_add(boundary.additional_units())
        .unwrap();
    assert_eq!(
        work.borrow().plan_work,
        canonical.checked_add(additional).unwrap()
    );
    drop(boundary);
    drop(shared);
    drop(input);
    assert_eq!(work.borrow().sound_context_frames, 0);
    let frozen = golden();
    let old = &frozen["cases"][0]["observation"];
    assert_eq!(
        old["controller_plan_work"].as_u64().unwrap() as usize,
        canonical
            .checked_add(deadpan_dsp::FINITE_FILTER_TAPS)
            .unwrap()
    );
    let actual = baseline::measured(&plan, &Context::Root, &owner, at, stage, usize::MAX, None);
    assert_eq!(
        actual["controller_plan_work"],
        json!(
            canonical
                .checked_add(additional)
                .unwrap()
                .checked_add(deadpan_dsp::FINITE_FILTER_TAPS)
                .unwrap()
        )
    );
}

#[test]
fn temporary_boundary_and_tile_pcm_refusals_keep_filter_and_graph_for_retry() {
    for refuse_boundary in [true, false] {
        let plan = baseline::frozen("stable_repeat_plays_default_gap_and_overrides", "voice");
        let owner = baseline::observed(&plan, "voice", 0, false);
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = fixture::control(&cancelled, &work);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        renderer.active_frames = BASE;
        let mut graph = graph(&plan);
        let id = add(&mut renderer, &mut graph, &owner, usize::MAX, control).unwrap();
        let copy = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            graph.nodes[id]
                .boundary
                .owner()
                .treatment_boundary_admitted(
                    fixture::stage(&owner),
                    control.query_limits().unwrap(),
                    &mut admission,
                )
                .unwrap()
        };
        let copy_frames = copy.lease().unwrap().frames();
        assert!(copy_frames > 0);
        drop(copy);
        let active = renderer.active_frames;
        let resident = graph.resident;
        let pointer = graph.nodes[id].filter.as_ref().map(std::ptr::from_ref);
        let pcm = u64::try_from((INPUT + OUTPUT * 2) * 2).unwrap();
        renderer.limits.maximum_resident_frames = u32::try_from(if refuse_boundary {
            active
        } else {
            active + copy_frames + pcm - 1
        })
        .unwrap();
        let mut provider = fixture::NoSources(0);
        assert!(matches!(
            renderer.eq_tile(&mut provider, &mut graph, id, 0, control, 1),
            Err(StageAudioError::Limit("resident PCM or stage cache"))
        ));
        assert_eq!(provider.0, 0);
        assert_eq!(
            graph.nodes[id].filter.as_ref().map(std::ptr::from_ref),
            pointer
        );
        assert!(graph.nodes[id].tiles.is_empty());
        assert_eq!(graph.resident, resident);
        assert_eq!(renderer.active_frames, active);
        assert_eq!(work.borrow().sound_context_frames, 0);
        release(&mut renderer, graph);
        assert_eq!(renderer.active_frames, BASE);
    }
}
#[test]
fn actual_incoming_retained_has_only_boundary_and_native_work_and_no_new_context_carrier() {
    let mut observed = 0;
    let _ = baseline::cases(|plan, incoming, owner, at, stage, parent, owner_limit| {
        let Context::Retained(expected) = incoming else {
            return Value::Null;
        };
        assert!(owner_limit.is_none());
        observed += 1;
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = fixture::control(&cancelled, &work);
        let mut renderer = StageAudio::new(Arc::clone(plan));
        let (boundary_fee, boundary_frames) = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            let boundary = owner
                .treatment_boundary_admitted(stage, control.query_limits().unwrap(), &mut admission)
                .unwrap();
            let boundary_fee = boundary.additional_units();
            let boundary_frames = boundary.lease().unwrap().frames();
            assert_eq!(boundary.canonical_units(), 0);
            assert_eq!(work.borrow().plan_work, boundary_fee);
            drop(boundary);
            assert_eq!(work.borrow().sound_context_frames, 0);
            (boundary_fee, boundary_frames)
        };

        let description_work = RefCell::new(ReadWork::default());
        let description_control = fixture::control(&cancelled, &description_work);
        let description = {
            let mut admission =
                sound_context_admission::Admission::new(&mut renderer, description_control);
            node_description::describe_admitted(
                plan,
                request(incoming, owner, at, stage),
                description_control,
                &mut admission,
            )
            .unwrap()
        };
        assert!(
            description.original.is_none(),
            "incoming context already owns its allocation"
        );
        let Context::Retained(actual) = &description.context else {
            panic!("actual retained consumer was replaced");
        };
        assert!(actual.ptr_eq(expected));
        assert_eq!(
            description_work.borrow().plan_work,
            boundary_fee,
            "no direct membership, capture, probe, retained membership or context copy"
        );
        assert_eq!(
            description_work.borrow().sound_context_frames,
            boundary_frames
        );
        drop(description);
        assert_eq!(description_work.borrow().sound_context_frames, 0);
        assert_eq!(description_work.borrow().plan_work, boundary_fee);

        let node = baseline::measured(plan, incoming, owner, at, stage, parent, None);
        assert_eq!(
            node["controller_plan_work"],
            json!(
                boundary_fee
                    .checked_add(deadpan_dsp::FINITE_FILTER_TAPS)
                    .unwrap()
            )
        );
        assert_eq!(
            node["accounting"]["graph_resident"],
            json!(
                fixture::units()
                    .checked_add(boundary_frames)
                    .unwrap()
                    .checked_add(capacity_frames::<Node<'_, '_>>(4))
                    .unwrap()
            )
        );
        Value::Null
    });
    assert_eq!(
        observed, 1,
        "the frozen oracle has exactly one actual incoming retained case"
    );
}
