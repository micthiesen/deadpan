use super::*;
use crate::plan::audio_bound::BindingTarget;
use crate::plan::audio_owners::admission::Error;
use crate::plan::audio_signal::AudioStage;
use crate::plan::{AudioDefinitionSelector, LookupStats, RenderPlan};
use deadpan_core::*;
use std::alloc::Layout;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Debug)]
enum Failure {
    Plan(PlanError),
    Memory,
    Cancelled,
}
impl From<PlanError> for Failure {
    fn from(value: PlanError) -> Self {
        Self::Plan(value)
    }
}

#[derive(Default)]
struct Ledger {
    active: usize,
    peak: usize,
    work: usize,
    limit: usize,
    reserve_calls: usize,
    cancel_at: Option<usize>,
    change_issuer: bool,
    issuer: u64,
}
struct Lease {
    state: Rc<RefCell<Ledger>>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.state.borrow_mut().active -= self.bytes;
    }
}
struct Policy(Rc<RefCell<Ledger>>);
impl Policy {
    fn new(limit: usize) -> Self {
        Self(Rc::new(RefCell::new(Ledger {
            limit,
            issuer: 7,
            ..Ledger::default()
        })))
    }
}
impl AudioSoundContextAdmission for Policy {
    type Error = Failure;
    type Issuer = u64;
    type Lease = Lease;
    fn issuer(&self) -> u64 {
        self.0.borrow().issuer
    }
    fn spend(&mut self, work: usize) -> Result<(), Failure> {
        self.0.borrow_mut().work += work;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, Failure> {
        let mut state = self.0.borrow_mut();
        state.reserve_calls += 1;
        if state.cancel_at == Some(state.reserve_calls) {
            return Err(Failure::Cancelled);
        }
        let next = state
            .active
            .checked_add(layout.size())
            .ok_or(Failure::Memory)?;
        if next > state.limit {
            return Err(Failure::Memory);
        }
        state.active = next;
        state.peak = state.peak.max(next);
        if state.change_issuer {
            state.issuer += 1;
        }
        Ok(Lease {
            state: Rc::clone(&self.0),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        assert!(Rc::ptr_eq(&target.state, &source.state));
        target.bytes += source.bytes;
        source.bytes = 0;
    }
}

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn local(work: usize) -> Budget {
    Budget {
        remaining: work,
        lookup: LookupStats::default(),
    }
}
fn repeat() -> RepeatInstance {
    RepeatInstance {
        node: id("repeat"),
        iteration: IterationId {
            allocation: revision("plays"),
            ordinal: 7,
        },
    }
}
fn owner<'a>(
    node: &'a NodeId,
    repeats: &'a [RepeatInstance],
    gap: &'a IterationId,
) -> BoundaryOwner<'a> {
    BoundaryOwner {
        node,
        repeats,
        gap_after: Some(gap),
        policies: AudioEdgePolicies {
            node_start: AudioEdgePolicy::Hard,
            ..AudioEdgePolicies::default()
        },
        placement_support: false,
    }
}

fn fill_extent(
    extent: &mut AdmittedExtent<u64, Lease>,
    budget: &mut QueryBudget<'_, Policy>,
) -> QueryResult<(), Policy> {
    let outer = id("outer");
    let inner = id("inner");
    let repeats = [repeat()];
    let gap = IterationId {
        allocation: revision("gaps"),
        ordinal: 8,
    };
    let a = owner(&outer, &repeats, &gap);
    let b = owner(&inner, &repeats, &gap);
    extent.clip_start(
        ExactRatio::integer(2),
        a,
        AudioBoundaryKind::NodeStart,
        budget,
    )?;
    extent.clip_start(
        ExactRatio::integer(2),
        b,
        AudioBoundaryKind::NodeStart,
        budget,
    )?;
    assert_eq!(
        extent
            .boundaries
            .start
            .iter()
            .map(|v| v.instance.node.as_str())
            .collect::<Vec<_>>(),
        ["outer", "inner"]
    );
    assert_eq!(extent.boundaries.start[0].policy, AudioEdgePolicy::Hard);
    assert_eq!(extent.boundaries.start[1].gap_after.as_ref(), Some(&gap));
    let start_capacity = extent.boundaries.start.capacity();
    extent.clip_start(
        ExactRatio::integer(1),
        a,
        AudioBoundaryKind::NodeStart,
        budget,
    )?;
    extent.clip_start(
        ExactRatio::integer(3),
        b,
        AudioBoundaryKind::NodeStart,
        budget,
    )?;
    assert_eq!(extent.boundaries.start.capacity(), start_capacity);
    extent.clip_end(
        ExactRatio::integer(8),
        a,
        AudioBoundaryKind::NodeEnd,
        budget,
    )?;
    extent.clip_end(
        ExactRatio::integer(8),
        b,
        AudioBoundaryKind::NodeEnd,
        budget,
    )?;
    let end_capacity = extent.boundaries.end.capacity();
    extent.clip_end(
        ExactRatio::integer(9),
        a,
        AudioBoundaryKind::NodeEnd,
        budget,
    )?;
    extent.clip_end(
        ExactRatio::integer(7),
        a,
        AudioBoundaryKind::NodeEnd,
        budget,
    )?;
    assert_eq!(extent.boundaries.end.capacity(), end_capacity);
    Ok(())
}

#[test]
fn capture_boundaries_keep_literal_coincidence_order_capacity_and_paid_work() {
    let mut policy = Policy::new(usize::MAX);
    let mut work = local(10_000);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    let mut extent = AdmittedExtent::new(ExactRatio::ZERO..ExactRatio::integer(10), 7);
    fill_extent(&mut extent, &mut budget).unwrap();
    assert_eq!(budget.canonical_used, 12);
    assert!(budget.additional_used > 0);
    let actual = extent.finish(&mut budget).unwrap();
    assert_eq!(actual.range, ExactRatio::integer(3)..ExactRatio::integer(7));
    assert_eq!(actual.boundaries.start.len(), 1);
    assert_eq!(actual.boundaries.start[0].instance.node, id("inner"));
    assert_eq!(actual.boundaries.end.len(), 1);
    assert_eq!(actual.boundaries.end[0].instance.node, id("outer"));
    assert_eq!(actual.boundaries.end[0].policy, AudioEdgePolicy::Automatic);
    assert!(policy.0.borrow().active > 0);
    drop(actual);
    assert_eq!(policy.0.borrow().active, 0);

    let repeats = [repeat()];
    let gap = IterationId {
        allocation: revision("gaps"),
        ordinal: 8,
    };
    let node = id("old");
    let owner = owner(&node, &repeats, &gap);
    let mut old = AudioExtent::new(ExactRatio::ZERO..ExactRatio::integer(10));
    let mut old_budget = local(4);
    old.clip_start(
        ExactRatio::integer(2),
        owner,
        AudioBoundaryKind::NodeStart,
        &mut old_budget,
    )
    .unwrap();
    old.clip_end(
        ExactRatio::integer(8),
        owner,
        AudioBoundaryKind::NodeEnd,
        &mut old_budget,
    )
    .unwrap();
    assert_eq!(old_budget.remaining, 0);
    assert_eq!(old.range, ExactRatio::integer(2)..ExactRatio::integer(8));
}

#[test]
fn capture_boundary_actual_peak_and_work_boundaries_release_every_lane() {
    let mut measured = Policy::new(usize::MAX);
    let mut work = local(10_000);
    let mut budget = QueryBudget::new(&mut work, &mut measured);
    let mut extent = AdmittedExtent::new(ExactRatio::ZERO..ExactRatio::integer(10), 7);
    fill_extent(&mut extent, &mut budget).unwrap();
    let needed_work = budget.canonical_used + budget.additional_used;
    drop(extent);
    let peak = measured.0.borrow().peak;
    assert!(peak > 0);
    assert_eq!(measured.0.borrow().active, 0);

    for (limit, expected_success) in [(peak, true), (peak - 1, false)] {
        let mut policy = Policy::new(limit);
        let mut work = local(needed_work);
        let mut budget = QueryBudget::new(&mut work, &mut policy);
        let mut extent = AdmittedExtent::new(ExactRatio::ZERO..ExactRatio::integer(10), 7);
        let result = fill_extent(&mut extent, &mut budget);
        assert_eq!(result.is_ok(), expected_success);
        if !expected_success {
            assert!(matches!(result, Err(Error::Policy(Failure::Memory))));
        }
        drop(extent);
        assert_eq!(policy.0.borrow().active, 0);
    }
    let mut policy = Policy::new(usize::MAX);
    let mut work = local(needed_work - 1);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    let mut extent = AdmittedExtent::new(ExactRatio::ZERO..ExactRatio::integer(10), 7);
    assert!(matches!(
        fill_extent(&mut extent, &mut budget),
        Err(Error::Plan(PlanError::AudioQueryLimit("structural work")))
    ));
    drop(extent);
    assert_eq!(policy.0.borrow().active, 0);
}

#[test]
fn capture_boundary_ignores_nonintersections_before_work_and_allocation() {
    let mut policy = Policy::new(0);
    let mut work = local(0);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    let node = id("unused");
    let gap = IterationId {
        allocation: revision("gaps"),
        ordinal: 0,
    };
    let mut extent = AdmittedExtent::new(ExactRatio::integer(2)..ExactRatio::integer(8), 7);
    extent
        .clip_start(
            ExactRatio::ONE,
            owner(&node, &[], &gap),
            AudioBoundaryKind::NodeStart,
            &mut budget,
        )
        .unwrap();
    extent
        .clip_end(
            ExactRatio::integer(9),
            owner(&node, &[], &gap),
            AudioBoundaryKind::NodeEnd,
            &mut budget,
        )
        .unwrap();
    assert_eq!(budget.canonical_used, 0);
    assert_eq!(budget.additional_used, 0);
    assert!(extent.boundaries.start.is_empty() && extent.boundaries.end.is_empty());
    assert_eq!(policy.0.borrow().reserve_calls, 0);
}

#[test]
fn capture_boundary_refused_origin_does_not_publish_and_wrong_issuer_cleans_up() {
    let node = id("owner");
    let gap = IterationId {
        allocation: revision("gaps"),
        ordinal: 0,
    };
    for change_issuer in [false, true] {
        let mut policy = Policy::new(usize::MAX);
        {
            let mut state = policy.0.borrow_mut();
            state.change_issuer = change_issuer;
            if !change_issuer {
                state.cancel_at = Some(2);
            }
        }
        let mut work = local(100);
        let mut budget = QueryBudget::new(&mut work, &mut policy);
        let mut extent = AdmittedExtent::new(ExactRatio::ZERO..ExactRatio::integer(10), 7);
        let result = extent.clip_start(
            ExactRatio::ONE,
            owner(&node, &[], &gap),
            AudioBoundaryKind::NodeStart,
            &mut budget,
        );
        if change_issuer {
            assert!(matches!(
                result,
                Err(Error::Plan(PlanError::AudioMetadataIssuerMismatch))
            ));
        } else {
            assert!(matches!(result, Err(Error::Policy(Failure::Cancelled))));
        }
        assert_eq!(extent.range.start, ExactRatio::ZERO);
        assert!(extent.boundaries.start.is_empty());
        drop(extent);
        assert_eq!(policy.0.borrow().active, 0);
    }
}

fn stage_plan(pitch: PitchPolicy) -> RenderPlan {
    stage_plan_duration(pitch, 16)
}

fn stage_plan_duration(pitch: PitchPolicy, duration: i64) -> RenderPlan {
    let frames = |n| FrameDuration::new(n).unwrap();
    let leaf = BeatNode::hold(
        "Leaf",
        HoldRecipe {
            duration: frames(8),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    );
    let mut stage = leaf.clone();
    stage.kind = NodeKind::Retime {
        child: id("leaf"),
        duration: frames(duration),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(8)).unwrap(),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    let base = ProjectDocument::new(
        ProjectId::new("constructors").unwrap(),
        revision("current"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let nodes = BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("stage")])),
        (id("stage"), stage),
        (id("leaf"), leaf),
    ]);
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    RenderPlan::compile(&document).unwrap()
}

fn stage_index(plan: &RenderPlan) -> usize {
    plan.nodes
        .iter()
        .position(|v| v.inspection.id == id("stage"))
        .unwrap()
}

#[test]
fn capture_stage_literal_descriptor_matches_legacy_with_only_outer_canonical_copy() {
    let plan = stage_plan(PitchPolicy::Preserve);
    let node = stage_index(&plan);
    let repeats = [repeat()];
    let definition = AudioDefinitionSelector::Node { node: id("stage") };
    let old = AudioStage::for_node(
        &plan,
        node,
        &repeats,
        Some(&definition),
        Some(BindingTarget::Node(node)),
    )
    .unwrap();
    let mut policy = Policy::new(usize::MAX);
    let mut work = local(1_000);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    budget.canonical(repeats.len() + 1).unwrap();
    let admitted = AudioStage::for_node_admitted(
        &plan,
        node,
        &repeats,
        Some(&definition),
        Some(BindingTarget::Node(node)),
        &mut budget,
    )
    .unwrap();
    assert_eq!(admitted.value, old);
    assert_eq!(budget.canonical_used, 2);
    assert!(budget.additional_used > 0);
    let descriptor = admitted.descriptor();
    assert_eq!(descriptor.project_id.as_str(), "constructors");
    assert_eq!(descriptor.revision_id.as_str(), "current");
    assert_eq!(descriptor.instance.node, id("stage"));
    assert_eq!(descriptor.instance.repeats.as_slice(), repeats.as_slice());
    assert_eq!(descriptor.child, id("leaf"));
    assert_eq!(
        descriptor.selection,
        ExactRatio::ZERO..ExactRatio::integer(8)
    );
    assert_eq!(descriptor.rate, ExactRatio::new(1, 2).unwrap());
    assert!(descriptor.bypass_binding);
    drop(admitted);
    assert_eq!(policy.0.borrow().active, 0);
}

#[test]
fn capture_stage_actual_peak_and_cancel_failure_have_no_returned_value_or_charge() {
    let plan = stage_plan(PitchPolicy::Preserve);
    let node = stage_index(&plan);
    let repeats = [repeat()];
    let mut measured = Policy::new(usize::MAX);
    let mut work = local(1_000);
    let mut budget = QueryBudget::new(&mut work, &mut measured);
    let stage =
        AudioStage::for_node_admitted(&plan, node, &repeats, None, None, &mut budget).unwrap();
    let required_work = budget.additional_used;
    assert_eq!(budget.canonical_used, 0);
    drop(stage);
    let peak = measured.0.borrow().peak;
    for (limit, success) in [(peak, true), (peak - 1, false)] {
        let mut policy = Policy::new(limit);
        let mut work = local(required_work);
        let mut budget = QueryBudget::new(&mut work, &mut policy);
        let result = AudioStage::for_node_admitted(&plan, node, &repeats, None, None, &mut budget);
        assert_eq!(result.is_ok(), success);
        if !success {
            assert!(matches!(result, Err(Error::Policy(Failure::Memory))));
        } else {
            drop(result);
        }
        assert_eq!(policy.0.borrow().active, 0);
    }
    let mut policy = Policy::new(usize::MAX);
    let mut work = local(required_work - 1);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    assert!(matches!(
        AudioStage::for_node_admitted(&plan, node, &repeats, None, None, &mut budget),
        Err(Error::Plan(PlanError::AudioQueryLimit("structural work")))
    ));
    assert_eq!(policy.0.borrow().active, 0);

    let mut policy = Policy::new(usize::MAX);
    policy.0.borrow_mut().cancel_at = Some(3);
    let mut work = local(1_000);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    assert!(matches!(
        AudioStage::for_node_admitted(&plan, node, &repeats, None, None, &mut budget),
        Err(Error::Policy(Failure::Cancelled))
    ));
    assert_eq!(policy.0.borrow().active, 0);
}

#[test]
fn capture_stage_preserves_semantic_failure_before_allocation_and_late_arithmetic() {
    let plan = stage_plan(PitchPolicy::Preserve);
    let leaf = plan
        .nodes
        .iter()
        .position(|v| v.inspection.id == id("leaf"))
        .unwrap();
    let mut policy = Policy::new(0);
    let mut work = local(0);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    assert!(matches!(
        outer::<_, Policy>(AudioStage::for_node_admitted(
            &plan,
            leaf,
            &[],
            None,
            None,
            &mut budget,
        )),
        Err(Failure::Plan(PlanError::InvalidPlan(
            "audio stage is not a retime"
        )))
    ));
    assert_eq!(policy.0.borrow().reserve_calls, 0);

    // Exercise the private defensive arithmetic path with an impossible
    // compiled selection. No public descriptor or validation bypass is added.
    let mut plan = stage_plan(PitchPolicy::Preserve);
    let node = stage_index(&plan);
    if let crate::plan::CompiledKind::Retime { start, .. } = &mut plan.nodes[node].kind {
        *start = ExactRatio::new(i128::MAX, 1).unwrap();
    }
    assert!(matches!(
        AudioStage::for_node(&plan, node, &[], None, None),
        Err(PlanError::Time(TimeError::Overflow))
    ));
    let mut policy = Policy::new(usize::MAX);
    let mut work = local(1_000);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    assert!(matches!(
        AudioStage::for_node_admitted(&plan, node, &[], None, None, &mut budget),
        Err(Error::Plan(PlanError::Time(TimeError::Overflow)))
    ));
    assert!(policy.0.borrow().reserve_calls > 0);
    assert_eq!(policy.0.borrow().active, 0);
}

#[test]
fn capture_stage_keeps_unity_shift_processing_and_follow_speed_refusal() {
    let pitch = PitchPolicy::Shift { semitones: 2 };
    let plan = stage_plan_duration(pitch, 8);
    let foreign = stage_plan_duration(pitch, 8);
    let node = stage_index(&plan);
    let old = AudioStage::for_node(&plan, node, &[], None, None).unwrap();
    let mut policy = Policy::new(usize::MAX);
    let mut work = local(1_000);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    let stage = AudioStage::for_node_admitted(&plan, node, &[], None, None, &mut budget).unwrap();
    assert_eq!(stage.value, old);
    assert_eq!(stage.descriptor().rate, ExactRatio::ONE);
    assert_eq!(stage.descriptor().pitch, pitch);
    assert!(stage.belongs_to(&plan));
    assert!(!stage.belongs_to(&foreign));
    drop(stage);
    assert_eq!(policy.0.borrow().active, 0);

    let plan = stage_plan(PitchPolicy::FollowSpeed);
    let mut policy = Policy::new(0);
    let mut work = local(0);
    let mut budget = QueryBudget::new(&mut work, &mut policy);
    assert!(matches!(
        AudioStage::for_node_admitted(&plan, stage_index(&plan), &[], None, None, &mut budget,),
        Err(Error::Plan(PlanError::InvalidPlan(
            "audio stage is not nonunity Preserve"
        )))
    ));
    assert_eq!(policy.0.borrow().reserve_calls, 0);
}
