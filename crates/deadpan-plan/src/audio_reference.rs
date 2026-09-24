//! Policy queries against one immutable old timing layout. These clocks do not
//! decode media or authorize bypassing Preserve preparation. Every sample query
//! belongs to an explicit root or preparation clock borrowed from this plan.
use std::collections::BTreeMap;
use std::ops::Range;

use deadpan_core::{
    ExactRatio, FrameDuration, FrozenAudioKind, FrozenAudioLayout, InsertionBias, InstancePath,
    IterationId, MIX_SAMPLE_RATE, NodeId, PitchPolicy, ReferenceAudibility, RepeatInstance,
    RepeatLayout, TimeError,
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
        let (probe, bias) = self.grid.probe(sample)?;
        let mut transform = Transform {
            origin: ExactRatio::ZERO,
            scale: ExactRatio::ONE,
        };
        let mut extent = self.support.clone();
        let mut current = &self.root;
        work.spend(self.repeats.len())?;
        let mut repeats = self.repeats.clone();
        let (content, gap_after) = loop {
            work.spend(1)?;
            work.lookup.visited_nodes += 1;
            let node = &self.plan.layout.nodes()[current];
            extent = intersect(
                extent,
                transform.origin..transform.at(ExactRatio::integer(node.duration.frames()))?,
            )?;
            let local = probe
                .checked_sub(transform.origin)?
                .checked_div(transform.scale)?;
            match &node.kind {
                FrozenAudioKind::Source { placement: None } => {
                    break (
                        ReferenceAudioContent::Silence {
                            reason: SilenceReason::NoSourceAudio,
                        },
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
                        ReferenceAudioContent::Silence {
                            reason: SilenceReason::OutsideSourcePlacement,
                        }
                    } else if sample >= self.grid.boundary(end)? {
                        extent.start = maximum(extent.start, end)?;
                        ReferenceAudioContent::Silence {
                            reason: SilenceReason::OutsideSourcePlacement,
                        }
                    } else {
                        extent = intersect(extent, start..end)?;
                        ReferenceAudioContent::Source
                    };
                    break (content, None);
                }
                FrozenAudioKind::Hold { audio } => break (content(*audio), None),
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
                        extent = intersect(
                            extent,
                            transform.origin
                                ..transform
                                    .at(ExactRatio::integer(location.play.gap_after.frames()))?,
                        )?;
                        break (content(*gap_audio), Some(location.play.iteration));
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
        Ok(ReferenceAudioSpan {
            samples: allocated_samples.clone(),
            allocated_samples,
            extent,
            instance: InstancePath {
                node: current.clone(),
                repeats,
            },
            gap_after,
            grid: self.grid,
            content,
        })
    }
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

#[derive(Clone, Copy)]
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
