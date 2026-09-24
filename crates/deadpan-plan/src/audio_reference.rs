//! Policy queries against one immutable old timing layout. These clocks do not
//! decode media or authorize bypassing Preserve preparation. Every sample query
//! belongs to an explicit root or preparation clock borrowed from this plan.
use std::collections::BTreeMap;
use std::ops::Range;

use deadpan_core::{
    AudioSample, ExactRatio, FrameDuration, FrozenAudioKind, FrozenAudioLayout, InsertionBias,
    InstancePath, IterationId, MIX_SAMPLE_RATE, NodeId, PitchPolicy, ReferenceAudibility,
    RepeatInstance, RepeatLayout, RetimePurpose, TimeError,
};
use serde::Serialize;

use crate::{
    AudioBoundaryRule, AudioQueryLimits, AudioSampleGrid, LookupStats, PlanError, SilenceReason,
};

/// Index interpreted only through its owning old reference clock handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ReferenceSample(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReferenceAudioContent {
    Source,
    RoomTone,
    /// Requested maximum in the owning Hold/gap's local frames. This is policy
    /// metadata, not a scaled output duration or implemented tail expiry.
    Tail {
        maximum: FrameDuration,
    },
    Silence {
        reason: SilenceReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "instance", rename_all = "snake_case")]
pub enum ReferenceClockOwner {
    ProjectRoot,
    PreserveInput(InstancePath),
    PreserveOutput(InstancePath),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceAudioSpan {
    pub samples: Range<ReferenceSample>,
    pub allocated_samples: Range<ReferenceSample>,
    /// Exact structural extent in this old clock's frame domain.
    pub extent: Range<ExactRatio>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub grid: AudioSampleGrid<ReferenceSample>,
    pub content: ReferenceAudioContent,
}

/// Results retain the handle that gives their sample indices meaning.
#[derive(Debug, Clone)]
pub struct ReferenceAudioQuery<'plan> {
    pub clock: ReferenceAudioClock<'plan>,
    pub samples: Range<ReferenceSample>,
    pub spans: Vec<ReferenceAudioSpan>,
    pub lookup: LookupStats,
}

/// An opaque Preserve output owns one processing domain even when its input
/// contains several sources and Holds. Leaf policies remain explicit; absent
/// input and an authored silent Hold are not interchangeable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReferenceProcessingKind {
    Leaf {
        content: ReferenceAudioContent,
    },
    Preserve {
        selection: Range<ExactRatio>,
        duration: FrameDuration,
        rate: ExactRatio,
    },
}

/// A physical processing domain borrowed from one admitted timing layout.
/// Neither matching node names nor matching numeric grids establish identity.
/// Authored copy lineage can be queried separately from physical identity.
#[derive(Debug, Clone)]
pub struct ReferenceProcessingDomain<'plan> {
    clock: ReferenceAudioClock<'plan>,
    instance: InstancePath,
    gap_after: Option<IterationId>,
    kind: ReferenceProcessingKind,
    allocated_samples: Range<ReferenceSample>,
    extent: Range<ExactRatio>,
    meaningful_extent: Range<ExactRatio>,
    meaningful_samples: Range<ReferenceSample>,
    transform: Transform,
    lookup: LookupStats,
}

impl<'plan> ReferenceProcessingDomain<'plan> {
    pub fn clock(&self) -> &ReferenceAudioClock<'plan> {
        &self.clock
    }
    pub fn instance(&self) -> &InstancePath {
        &self.instance
    }
    pub fn gap_after(&self) -> Option<&IterationId> {
        self.gap_after.as_ref()
    }
    pub fn kind(&self) -> &ReferenceProcessingKind {
        &self.kind
    }
    pub fn allocated_samples(&self) -> Range<ReferenceSample> {
        self.allocated_samples.clone()
    }
    pub fn extent(&self) -> Range<ExactRatio> {
        self.extent.clone()
    }
    pub fn meaningful_extent(&self) -> Range<ExactRatio> {
        self.meaningful_extent.clone()
    }
    pub fn meaningful_samples(&self) -> Range<ReferenceSample> {
        self.meaningful_samples.clone()
    }
    pub fn lookup(&self) -> LookupStats {
        self.lookup
    }

    /// Exact local coordinate on this domain's source/Hold or intrinsic Preserve
    /// output clock. Out-of-allocation positions are continued, never clamped.
    pub fn local_at(&self, sample: ReferenceSample) -> Result<ExactRatio, PlanError> {
        Ok(self
            .clock
            .grid
            .at(sample)?
            .checked_sub(self.transform.origin)?
            .checked_div(self.transform.scale)?)
    }

    /// Identity deliberately excludes the visible Partition crop and lookup
    /// budget. A copied physical context is still a different domain.
    pub fn same_domain(&self, other: &Self) -> bool {
        std::ptr::eq(self.clock.plan, other.clock.plan)
            && self.clock.owner == other.clock.owner
            && self.instance == other.instance
            && self.gap_after == other.gap_after
            && self.meaningful_extent == other.meaningful_extent
            && self.kind == other.kind
    }

    /// Report an explicit copy relationship compatible with this frozen clock,
    /// phase, processing kind and meaningful extent. Visible Partition crops
    /// may differ. This is provenance, not proof of equal PCM: resolving live
    /// media, recipes and retained policy remains a separate admission step.
    /// Equal timing or media alone never establish lineage. Work is bounded by
    /// the admitted occurrence depth and does not enumerate Repeat plays.
    pub fn shares_copy_lineage(&self, other: &Self) -> bool {
        if !std::ptr::eq(self.clock.plan, other.clock.plan)
            || self.clock.owner != other.clock.owner
            || self.gap_after != other.gap_after
            || self.meaningful_extent != other.meaningful_extent
            || self.transform.origin != other.transform.origin
            || self.transform.scale != other.transform.scale
            || self.kind != other.kind
            || self.instance.repeats.len() != other.instance.repeats.len()
        {
            return false;
        }
        let lineages = self.clock.plan.layout.audio_lineage();
        let related = |a: &NodeId, b: &NodeId| {
            lineages
                .get(a)
                .is_some_and(|lineage| Some(lineage) == lineages.get(b))
        };
        related(&self.instance.node, &other.instance.node)
            && self
                .instance
                .repeats
                .iter()
                .zip(&other.instance.repeats)
                .all(|(a, b)| {
                    a.iteration == b.iteration && (a.node == b.node || related(&a.node, &b.node))
                })
    }

    /// Place this old domain on a new root grid without changing its rate.
    /// The caller supplies the new *meaningful* start, not a query/Partition
    /// start. Later genuine domains each need their own placement.
    pub fn place_root(
        &self,
        meaningful_start: AudioSample,
    ) -> Result<RetainedRootMap<'plan>, PlanError> {
        if self.clock.owner != ReferenceClockOwner::ProjectRoot {
            return Err(PlanError::InvalidPlan(
                "root resume requires a project-root reference domain",
            ));
        }
        Ok(RetainedRootMap {
            domain: self.clone(),
            output_anchor: meaningful_start,
            reference_at_anchor: ExactRatio::integer(self.meaningful_samples.start.0),
        })
    }
}

/// Unit-rate root-sample continuation for one physical processing domain. It
/// retains a borrowed reference identity and composes successive insertion
/// anchors. It neither selects a live target nor authorizes reading old media.
#[derive(Debug, Clone)]
pub struct RetainedRootMap<'plan> {
    domain: ReferenceProcessingDomain<'plan>,
    output_anchor: AudioSample,
    reference_at_anchor: ExactRatio,
}

impl<'plan> RetainedRootMap<'plan> {
    pub fn domain(&self) -> &ReferenceProcessingDomain<'plan> {
        &self.domain
    }
    pub fn output_anchor(&self) -> AudioSample {
        self.output_anchor
    }
    pub fn reference_at_anchor(&self) -> ExactRatio {
        self.reference_at_anchor
    }

    /// Wide exact arithmetic permits exhausted coordinates outside i64 support.
    /// Consumers still apply the retained support and explicit policy masks.
    pub fn reference_position(&self, output: AudioSample) -> Result<ExactRatio, PlanError> {
        self.reference_position_at(ExactRatio::integer(output.0))
    }

    /// Evaluate at an exact position measured in current root samples, for an
    /// explicit subsequent root-to-point-grid transfer. This does not reinterpret
    /// a SignalSample as a root sample or choose the carrier grid's origin.
    pub fn reference_position_at(&self, output: ExactRatio) -> Result<ExactRatio, PlanError> {
        Ok(self
            .reference_at_anchor
            .checked_add(output.checked_sub(ExactRatio::integer(self.output_anchor.0))?)?)
    }

    pub fn resume(&self, old_cut: AudioSample, new_anchor: AudioSample) -> Result<Self, PlanError> {
        Ok(Self {
            domain: self.domain.clone(),
            output_anchor: new_anchor,
            reference_at_anchor: self.reference_position(old_cut)?,
        })
    }
}

#[derive(Debug, Clone)]
struct SequenceEntry {
    child: NodeId,
    start: i64,
    end: i64,
}

#[derive(Debug, Clone)]
pub struct AudioReferencePlan {
    layout: FrozenAudioLayout,
    sequences: BTreeMap<NodeId, Vec<SequenceEntry>>,
    repeats: BTreeMap<NodeId, RepeatLayout>,
    frames_per_sample: ExactRatio,
}

impl AudioReferencePlan {
    pub fn compile(layout: &FrozenAudioLayout) -> Result<Self, PlanError> {
        layout.validate()?;
        let durations = layout
            .nodes()
            .iter()
            .map(|(id, node)| (id.clone(), node.duration))
            .collect();
        let mut sequences = BTreeMap::new();
        let mut repeats = BTreeMap::new();
        for (id, node) in layout.nodes() {
            match &node.kind {
                FrozenAudioKind::Sequence { children } => {
                    let mut end = 0i64;
                    let mut entries = Vec::with_capacity(children.len());
                    for child in children {
                        let start = end;
                        end = end
                            .checked_add(layout.nodes()[child].duration.frames())
                            .ok_or(TimeError::Overflow)?;
                        if end > start {
                            entries.push(SequenceEntry {
                                child: child.clone(),
                                start,
                                end,
                            });
                        }
                    }
                    sequences.insert(id.clone(), entries);
                }
                FrozenAudioKind::Repeat {
                    child,
                    iterations,
                    gap_duration,
                    ..
                } => {
                    repeats.insert(
                        id.clone(),
                        RepeatLayout::compile(
                            iterations,
                            child,
                            layout.overrides().get(id),
                            *gap_duration,
                            &durations,
                        )?,
                    );
                }
                _ => {}
            }
        }
        let rate = layout.rate();
        Ok(Self {
            layout: layout.clone(),
            sequences,
            repeats,
            frames_per_sample: ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
        })
    }

    pub fn layout(&self) -> &FrozenAudioLayout {
        &self.layout
    }

    pub fn root_clock(&self) -> ReferenceAudioClock<'_> {
        self.clock(
            self.layout.root().clone(),
            Vec::new(),
            ExactRatio::ZERO..ExactRatio::integer(self.layout.duration().frames()),
            ReferenceClockOwner::ProjectRoot,
            AudioBoundaryRule::RoundEven,
        )
    }

    /// Only a validated, nonunity Preserve occurrence owns this preparation grid.
    /// The grid origin is the selected child's exact input start, not zero.
    pub fn preserve_input_clock(
        &self,
        instance: &InstancePath,
    ) -> Result<ReferenceAudioClock<'_>, PlanError> {
        self.preserve_clock(instance, true)
    }

    pub fn preserve_output_clock(
        &self,
        instance: &InstancePath,
    ) -> Result<ReferenceAudioClock<'_>, PlanError> {
        self.preserve_clock(instance, false)
    }

    fn preserve_clock(
        &self,
        instance: &InstancePath,
        input: bool,
    ) -> Result<ReferenceAudioClock<'_>, PlanError> {
        self.layout
            .project(instance, ExactRatio::ZERO, None, 65_536)?;
        let node = &self.layout.nodes()[&instance.node];
        let FrozenAudioKind::Retime {
            child,
            mapping,
            pitch: PitchPolicy::Preserve,
            ..
        } = &node.kind
        else {
            return Err(PlanError::InvalidPlan(
                "reference clock owner is not a Preserve stage",
            ));
        };
        if mapping.duration() == node.duration {
            return Err(PlanError::InvalidPlan(
                "unity Retime does not own a preparation clock",
            ));
        }
        let (root, support, owner) = if input {
            (
                child.clone(),
                ExactRatio::integer(mapping.start().0)..ExactRatio::integer(mapping.end().0),
                ReferenceClockOwner::PreserveInput(instance.clone()),
            )
        } else {
            (
                instance.node.clone(),
                ExactRatio::ZERO..ExactRatio::integer(node.duration.frames()),
                ReferenceClockOwner::PreserveOutput(instance.clone()),
            )
        };
        Ok(self.clock(
            root,
            instance.repeats.clone(),
            support,
            owner,
            AudioBoundaryRule::PointCeil,
        ))
    }

    fn clock(
        &self,
        root: NodeId,
        repeats: Vec<RepeatInstance>,
        support: Range<ExactRatio>,
        owner: ReferenceClockOwner,
        rule: AudioBoundaryRule,
    ) -> ReferenceAudioClock<'_> {
        ReferenceAudioClock {
            plan: self,
            root,
            repeats,
            grid: AudioSampleGrid::new(support.start, self.frames_per_sample, rule)
                .expect("validated positive frame rate"),
            support,
            owner,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReferenceAudioClock<'plan> {
    plan: &'plan AudioReferencePlan,
    root: NodeId,
    repeats: Vec<RepeatInstance>,
    support: Range<ExactRatio>,
    grid: AudioSampleGrid<ReferenceSample>,
    owner: ReferenceClockOwner,
}

impl<'plan> ReferenceAudioClock<'plan> {
    pub fn owner(&self) -> &ReferenceClockOwner {
        &self.owner
    }
    pub fn grid(&self) -> AudioSampleGrid<ReferenceSample> {
        self.grid
    }
    pub fn support(&self) -> Range<ExactRatio> {
        self.support.clone()
    }
    pub fn sample_count(&self) -> Result<ReferenceSample, PlanError> {
        Ok(self.grid.boundary(self.support.end)?)
    }

    /// Resolve the raw processing owner of one allocated sample. This stops at
    /// the first nonunity Preserve; `query` continues to flatten all policies.
    /// The sample is an allocation probe, not a source/DSP sampling coordinate.
    pub fn processing_domain_at(
        &self,
        sample: ReferenceSample,
        limits: AudioQueryLimits,
    ) -> Result<ReferenceProcessingDomain<'plan>, PlanError> {
        limits.validate()?;
        if sample.0 < 0 || sample >= self.sample_count()? {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut work = Work {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        work.spend(
            self.repeats
                .len()
                .checked_mul(2)
                .ok_or(TimeError::Overflow)?,
        )?;
        let located = self.locate(sample, true, &mut work)?;
        let meaningful_extent = located.meaningful_extent.ok_or(PlanError::InvalidPlan(
            "reference processing domain has no meaningful extent",
        ))?;
        let meaningful_samples = self.grid.boundary(meaningful_extent.start)?
            ..self.grid.boundary(meaningful_extent.end)?;
        Ok(ReferenceProcessingDomain {
            clock: self.clone(),
            instance: located.instance,
            gap_after: located.gap_after,
            kind: located.kind,
            allocated_samples: located.allocated_samples,
            extent: located.extent,
            meaningful_extent,
            meaningful_samples,
            transform: located.transform,
            lookup: work.lookup,
        })
    }

    pub fn query(
        &self,
        samples: Range<ReferenceSample>,
        limits: AudioQueryLimits,
    ) -> Result<ReferenceAudioQuery<'plan>, PlanError> {
        limits.validate()?;
        if samples.start.0 < 0
            || samples.end < samples.start
            || samples.end > self.sample_count()?
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut work = Work {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        // The returned clock owns two copies of the occurrence path at most.
        work.spend(
            self.repeats
                .len()
                .checked_mul(2)
                .ok_or(TimeError::Overflow)?,
        )?;
        let mut spans = Vec::new();
        let mut cursor = samples.start;
        while cursor < samples.end {
            if spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("span count"));
            }
            let mut span = self.span(cursor, &mut work)?;
            span.samples = cursor..span.allocated_samples.end.min(samples.end);
            cursor = span.samples.end;
            spans.push(span);
        }
        Ok(ReferenceAudioQuery {
            clock: self.clone(),
            samples,
            spans,
            lookup: work.lookup,
        })
    }

    fn span(
        &self,
        sample: ReferenceSample,
        work: &mut Work,
    ) -> Result<ReferenceAudioSpan, PlanError> {
        let located = self.locate(sample, false, work)?;
        let ReferenceProcessingKind::Leaf { content } = located.kind else {
            return Err(PlanError::InvalidPlan(
                "reference policy query stopped at Preserve",
            ));
        };
        Ok(ReferenceAudioSpan {
            samples: located.allocated_samples.clone(),
            allocated_samples: located.allocated_samples,
            extent: located.extent,
            instance: located.instance,
            gap_after: located.gap_after,
            grid: self.grid,
            content,
        })
    }

    fn locate(
        &self,
        sample: ReferenceSample,
        stop_at_preserve: bool,
        work: &mut Work,
    ) -> Result<Located, PlanError> {
        let (probe, bias) = self.grid.probe(sample)?;
        let mut transform = Transform {
            origin: ExactRatio::ZERO,
            scale: ExactRatio::ONE,
        };
        let mut extent = self.support.clone();
        // An input selection is a real preparation crop. The root and an
        // intrinsic output grid only bound allocation; transparent partitions
        // can retain processing context outside those visible bounds.
        let mut meaningful_extent = (stop_at_preserve
            && matches!(self.owner, ReferenceClockOwner::PreserveInput(_)))
        .then(|| self.support.clone());
        let mut current = &self.root;
        work.spend(self.repeats.len())?;
        let mut repeats = self.repeats.clone();
        let (kind, gap_after) = loop {
            work.spend(1)?;
            work.lookup.visited_nodes += 1;
            let node = &self.plan.layout.nodes()[current];
            let node_extent =
                transform.origin..transform.at(ExactRatio::integer(node.duration.frames()))?;
            extent = intersect(extent, node_extent.clone())?;
            if stop_at_preserve
                && !matches!(
                    node.kind,
                    FrozenAudioKind::Sequence { .. }
                        | FrozenAudioKind::Repeat { .. }
                        | FrozenAudioKind::Retime {
                            purpose: RetimePurpose::Partition,
                            ..
                        }
                )
            {
                meaningful_extent = Some(match meaningful_extent {
                    Some(previous) => intersect(previous, node_extent)?,
                    None => node_extent,
                });
            }
            let local = probe
                .checked_sub(transform.origin)?
                .checked_div(transform.scale)?;
            match &node.kind {
                FrozenAudioKind::Source { placement: None } => {
                    break (
                        leaf(ReferenceAudioContent::Silence {
                            reason: SilenceReason::NoSourceAudio,
                        }),
                        None,
                    );
                }
                FrozenAudioKind::Source {
                    placement: Some(placement),
                } => {
                    let start = transform.at(placement.start)?;
                    let end = transform.at(placement.end)?;
                    let content = if sample < self.grid.boundary(start)? {
                        extent.end = minimum(extent.end, start)?;
                        if let Some(domain) = &mut meaningful_extent {
                            domain.end = minimum(domain.end, start)?;
                        }
                        ReferenceAudioContent::Silence {
                            reason: SilenceReason::OutsideSourcePlacement,
                        }
                    } else if sample >= self.grid.boundary(end)? {
                        extent.start = maximum(extent.start, end)?;
                        if let Some(domain) = &mut meaningful_extent {
                            domain.start = maximum(domain.start, end)?;
                        }
                        ReferenceAudioContent::Silence {
                            reason: SilenceReason::OutsideSourcePlacement,
                        }
                    } else {
                        extent = intersect(extent, start..end)?;
                        if let Some(domain) = meaningful_extent {
                            meaningful_extent = Some(intersect(domain, start..end)?);
                        }
                        ReferenceAudioContent::Source
                    };
                    break (leaf(content), None);
                }
                FrozenAudioKind::Hold { audio } => break (leaf(content(*audio)), None),
                FrozenAudioKind::Sequence { .. } => {
                    let entries = &self.plan.sequences[current];
                    let (mut left, mut right) = (0, entries.len());
                    while left < right {
                        work.spend(1)?;
                        work.lookup.sequence_comparisons += 1;
                        let middle = left + (right - left) / 2;
                        let comparison = local.compare_integer(entries[middle].end);
                        if comparison.is_gt()
                            || (comparison.is_eq() && bias == InsertionBias::Right)
                        {
                            left = middle + 1;
                        } else {
                            right = middle;
                        }
                    }
                    let entry = entries.get(left).ok_or(PlanError::InvalidPlan(
                        "reference sequence has no allocated child",
                    ))?;
                    transform =
                        transform.child(ExactRatio::integer(entry.start), ExactRatio::ONE)?;
                    current = &entry.child;
                }
                FrozenAudioKind::Retime {
                    mapping,
                    pitch: PitchPolicy::Preserve,
                    ..
                } if stop_at_preserve && mapping.duration() != node.duration => {
                    break (
                        ReferenceProcessingKind::Preserve {
                            selection: ExactRatio::integer(mapping.start().0)
                                ..ExactRatio::integer(mapping.end().0),
                            duration: node.duration,
                            rate: ExactRatio::new(
                                i128::from(mapping.duration().frames()),
                                i128::from(node.duration.frames()),
                            )?,
                        },
                        None,
                    );
                }
                FrozenAudioKind::Retime { child, mapping, .. } => {
                    let inverse = ExactRatio::new(
                        i128::from(node.duration.frames()),
                        i128::from(mapping.duration().frames()),
                    )?;
                    transform = transform.child(
                        ExactRatio::ZERO.checked_sub(
                            ExactRatio::integer(mapping.start().0).checked_mul(inverse)?,
                        )?,
                        inverse,
                    )?;
                    current = child;
                }
                FrozenAudioKind::Repeat { gap_audio, .. } => {
                    let location = self.plan.repeats[current]
                        .locate_bounded(local, bias, work.remaining)
                        .map_err(|error| {
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                                PlanError::AudioQueryLimit("structural work")
                            } else {
                                error.into()
                            }
                        })?;
                    work.spend(location.comparisons)?;
                    work.lookup.iteration_run_comparisons += location.comparisons;
                    if location.in_gap {
                        let start = location
                            .play
                            .start
                            .checked_add(location.play.duration.frames())
                            .ok_or(TimeError::Overflow)?;
                        transform = transform.child(ExactRatio::integer(start), ExactRatio::ONE)?;
                        let gap_extent = transform.origin
                            ..transform
                                .at(ExactRatio::integer(location.play.gap_after.frames()))?;
                        extent = intersect(extent, gap_extent.clone())?;
                        if stop_at_preserve {
                            meaningful_extent = Some(match meaningful_extent {
                                Some(previous) => intersect(previous, gap_extent)?,
                                None => gap_extent,
                            });
                        }
                        break (leaf(content(*gap_audio)), Some(location.play.iteration));
                    }
                    transform = transform
                        .child(ExactRatio::integer(location.play.start), ExactRatio::ONE)?;
                    work.spend(1)?;
                    repeats.push(RepeatInstance {
                        node: current.clone(),
                        iteration: location.play.iteration,
                    });
                    current = self
                        .plan
                        .layout
                        .nodes()
                        .get_key_value(&location.play.child)
                        .ok_or(PlanError::InvalidPlan("reference Repeat child missing"))?
                        .0;
                }
            }
        };
        let allocated_samples =
            self.grid.boundary(extent.start)?..self.grid.boundary(extent.end)?;
        if !allocated_samples.contains(&sample) {
            return Err(PlanError::InvalidPlan("reference interval did not advance"));
        }
        Ok(Located {
            allocated_samples,
            extent,
            instance: InstancePath {
                node: current.clone(),
                repeats,
            },
            gap_after,
            kind,
            meaningful_extent,
            transform,
        })
    }
}

struct Located {
    allocated_samples: Range<ReferenceSample>,
    extent: Range<ExactRatio>,
    instance: InstancePath,
    gap_after: Option<IterationId>,
    kind: ReferenceProcessingKind,
    meaningful_extent: Option<Range<ExactRatio>>,
    transform: Transform,
}

fn leaf(content: ReferenceAudioContent) -> ReferenceProcessingKind {
    ReferenceProcessingKind::Leaf { content }
}

fn content(audio: ReferenceAudibility) -> ReferenceAudioContent {
    match audio {
        ReferenceAudibility::Silence => ReferenceAudioContent::Silence {
            reason: SilenceReason::SilentHold,
        },
        ReferenceAudibility::RoomTone => ReferenceAudioContent::RoomTone,
        ReferenceAudibility::Tail { maximum } => ReferenceAudioContent::Tail { maximum },
    }
}

struct Work {
    remaining: usize,
    lookup: LookupStats,
}
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), PlanError> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or(PlanError::AudioQueryLimit("structural work"))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct Transform {
    origin: ExactRatio,
    scale: ExactRatio,
}
impl Transform {
    fn at(self, local: ExactRatio) -> Result<ExactRatio, TimeError> {
        self.origin.checked_add(local.checked_mul(self.scale)?)
    }
    fn child(self, start: ExactRatio, scale: ExactRatio) -> Result<Self, TimeError> {
        Ok(Self {
            origin: self.at(start)?,
            scale: self.scale.checked_mul(scale)?,
        })
    }
}
fn minimum(a: ExactRatio, b: ExactRatio) -> Result<ExactRatio, TimeError> {
    Ok(if a.checked_sub(b)?.compare_integer(0).is_le() {
        a
    } else {
        b
    })
}
fn maximum(a: ExactRatio, b: ExactRatio) -> Result<ExactRatio, TimeError> {
    Ok(if a.checked_sub(b)?.compare_integer(0).is_ge() {
        a
    } else {
        b
    })
}
fn intersect(a: Range<ExactRatio>, b: Range<ExactRatio>) -> Result<Range<ExactRatio>, TimeError> {
    Ok(maximum(a.start, b.start)?..minimum(a.end, b.end)?)
}
