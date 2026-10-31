//! Actual Root structural traversal shared by ordinary and admitted spans.
use super::*;
use crate::plan::audio::capture::storage;
use crate::plan::audio_bound::query::GuardedBound;
use crate::plan::audio_owners::admission::Memory;

type Content<'plan, I, L> = GuardedBound<AudioSignalContent<'plan>, I, L>;

struct FinishFields {
    definition: Option<AudioDefinitionSelector>,
    samples: Range<SignalSample>,
    allocated_samples: Range<SignalSample>,
    signal_extent: Range<ExactRatio>,
    instance: InstancePath,
    gap_after: Option<IterationId>,
    transform: SignalTransform,
    grid: AudioSampleGrid<SignalSample>,
    sampling: AudioSampleMap<SignalSample>,
    retimes: Vec<AudioRetimeStage>,
}

fn stage_content<'plan, A: AudioSoundContextAdmission>(
    stage: Owned<AudioStage<'plan>, A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<Content<'plan, A::Issuer, A::Lease>, A> {
    let Owned { value, memory } = stage;
    Content::plain(AudioSignalContent::Stage(value), budget.issuer).adopt_memory(memory, budget)
}

fn finish<'plan, A: AudioSoundContextAdmission>(
    content: Content<'plan, A::Issuer, A::Lease>,
    fields: Owned<FinishFields, A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<GuardedBound<AudioSignalSpan<'plan>, A::Issuer, A::Lease>, A> {
    struct Mapping<C, F, I, L> {
        content: C,
        fields: F,
        memory: Memory<I, L>,
    }
    let mut pending = Mapping {
        content,
        fields: fields.value,
        memory: fields.memory,
    };
    budget.check_memory(&pending.memory)?;
    // Keep all Plan fields ahead of their charge while Core validates/maps.
    let mut result = pending.content.map(budget, |content| {
        let FinishFields {
            definition,
            samples,
            allocated_samples,
            signal_extent,
            instance,
            gap_after,
            transform,
            grid,
            sampling,
            retimes,
        } = pending.fields;
        AudioSignalSpan {
            definition,
            samples,
            allocated_samples,
            signal_extent,
            instance,
            gap_after,
            transform,
            grid,
            sampling,
            retimes,
            content,
        }
    })?;
    result.merge_memory(&mut pending.memory, budget)?;
    budget.check_memory(&result._memory)?;
    Ok(result)
}

impl<'plan> AudioSignal<'plan> {
    pub(in crate::plan) fn span_admitted<A: AudioSoundContextAdmission>(
        &self,
        sample: SignalSample,
        grid: AudioSampleGrid<SignalSample>,
        stop_at_preserve: bool,
        stop_at_bindings: bool,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<GuardedBound<AudioSignalSpan<'plan>, A::Issuer, A::Lease>, A> {
        let mut transform = self.transform()?;
        let (probe, bias) = grid.probe(sample)?;
        let mut extent = self.support.clone();
        let mut sampling_extent = self.constrain_support.then(|| {
            self.sampling_support
                .clone()
                .unwrap_or_else(|| self.support.clone())
        });
        let mut current = self.root;
        let mut memory = budget.memory();
        let mut repeats_capacity = budget.memory();
        let mut repeats = budget.repeats(&mut memory, &mut repeats_capacity, &self.repeats)?;
        let mut retimes_capacity = budget.memory();
        let mut retimes = Vec::new();
        let mut gap = budget.gap(&mut memory, &self.gap)?;
        let (content, gap_after) = loop {
            budget.canonical(1)?;
            budget.lookup.visited_nodes += 1;
            let node = &self.plan.nodes[current];
            if let Some(gap) = gap.take() {
                let CompiledKind::Repeat {
                    gap_audio: Some(audio),
                    ..
                } = &node.kind
                else {
                    return Err(
                        PlanError::InvalidPlan("audio signal gap definition is missing").into(),
                    );
                };
                let gap_extent = transform.signal_origin
                    ..transform.signal_from_local(ExactRatio::integer(gap.duration.frames()))?;
                extent = intersect(extent, gap_extent.clone())?;
                sampling_extent = Some(match sampling_extent {
                    Some(previous) => intersect(previous, gap_extent)?,
                    None => gap_extent,
                });
                if stop_at_bindings
                    && self.bypass_binding != Some(BindingTarget::Gap(current))
                    && let Some(bound) = self.plan.bound_at_query(
                        current,
                        &repeats,
                        self.definition.as_ref(),
                        crate::plan::audio_bound::BoundPlacement {
                            transform,
                            grid,
                            allocated_start: grid.boundary(extent.start)?,
                            support: sampling_extent.as_ref(),
                            constraints: &[],
                            gap: Some(&gap),
                        },
                        budget,
                    )?
                {
                    break (
                        bound
                            .boxed(budget)?
                            .map(budget, AudioSignalContent::Bound)?,
                        gap.after,
                    );
                }
                break (
                    Content::plain(
                        AudioSignalContent::Leaf(storage::hold(
                            audio,
                            gap.duration,
                            &mut memory,
                            budget,
                        )?),
                        budget.issuer,
                    ),
                    gap.after
                        .as_ref()
                        .map(|value| budget.iteration(&mut memory, value))
                        .transpose()?,
                );
            }
            if node.preserve_input.is_some() {
                return Err(PlanError::InvalidPlan(
                    "owned Preserve output requires its splice placement; use the intrinsic projection for preparation",
                ).into());
            }
            let node_extent = transform.signal_origin..transform.signal_from_local(node.extent)?;
            extent = intersect(extent, node_extent.clone())?;
            if !matches!(
                node.kind,
                CompiledKind::Retime {
                    purpose: RetimePurpose::Partition,
                    ..
                } | CompiledKind::Sequence { .. }
                    | CompiledKind::Repeat { .. }
            ) {
                sampling_extent = Some(match sampling_extent {
                    Some(previous) => intersect(previous, node_extent)?,
                    None => node_extent,
                });
            }
            if stop_at_bindings
                && self.bypass_binding != Some(BindingTarget::Node(current))
                && let Some(bound) = self.plan.bound_at_query(
                    current,
                    &repeats,
                    self.definition.as_ref(),
                    crate::plan::audio_bound::BoundPlacement {
                        transform,
                        grid,
                        allocated_start: grid.boundary(extent.start)?,
                        support: sampling_extent.as_ref(),
                        constraints: &[],
                        gap: None,
                    },
                    budget,
                )?
            {
                break (
                    bound
                        .boxed(budget)?
                        .map(budget, AudioSignalContent::Bound)?,
                    None,
                );
            }
            let local = transform.local_at_signal_frame(probe)?;
            match &node.kind {
                CompiledKind::Source { audio: None, .. } => {
                    break (
                        Content::plain(
                            AudioSignalContent::Leaf(AudioContent::Silence {
                                reason: SilenceReason::NoSourceAudio,
                            }),
                            budget.issuer,
                        ),
                        None,
                    );
                }
                CompiledKind::Source {
                    audio: Some(audio), ..
                } => {
                    if audio.selection.start == audio.selection.end {
                        break (
                            Content::plain(
                                AudioSignalContent::Leaf(AudioContent::Silence {
                                    reason: SilenceReason::OutsideSourceSelection,
                                }),
                                budget.issuer,
                            ),
                            None,
                        );
                    }
                    let start = transform.signal_from_local(audio.selection.start)?;
                    let end = transform.signal_from_local(audio.selection.end)?;
                    let content = if sample < grid.boundary(start)? {
                        extent.end = minimum(extent.end, start)?;
                        AudioContent::Silence {
                            reason: audio.outside_reason(),
                        }
                    } else if sample >= grid.boundary(end)? {
                        extent.start = maximum(extent.start, end)?;
                        AudioContent::Silence {
                            reason: audio.outside_reason(),
                        }
                    } else {
                        extent = intersect(extent, start..end)?;
                        let support = intersect(
                            sampling_extent
                                .clone()
                                .ok_or(PlanError::InvalidPlan("source has no sampling domain"))?,
                            start..end,
                        )?;
                        AudioContent::Source {
                            source: budget.source(&mut memory, &audio.source)?,
                            start: audio.start,
                            duration: audio.duration,
                            support: SourceSamplingSupport::from_local(
                                &audio.source,
                                audio.start,
                                audio.duration,
                                transform.local_at_signal_frame(support.start)?
                                    ..transform.local_at_signal_frame(support.end)?,
                            )?,
                        }
                    };
                    break (
                        Content::plain(AudioSignalContent::Leaf(content), budget.issuer),
                        None,
                    );
                }
                CompiledKind::Hold { audio, .. } => {
                    break (
                        Content::plain(
                            AudioSignalContent::Leaf(storage::hold(
                                audio,
                                node.inspection.duration,
                                &mut memory,
                                budget,
                            )?),
                            budget.issuer,
                        ),
                        None,
                    );
                }
                CompiledKind::Sequence { entries } => {
                    let mut left = 0;
                    let mut right = entries.len();
                    while left < right {
                        budget.canonical(1)?;
                        budget.lookup.sequence_comparisons += 1;
                        let middle = left + (right - left) / 2;
                        let comparison = local.compare(entries[middle].end);
                        let preceding = comparison.is_gt()
                            || (comparison.is_eq() && bias == InsertionBias::Right);
                        if preceding {
                            left = middle + 1;
                        } else {
                            right = middle;
                        }
                    }
                    let entry = entries
                        .get(left)
                        .ok_or(PlanError::InvalidPlan("audio signal sequence has no child"))?;
                    transform = transform.child(entry.start, ExactRatio::ONE)?;
                    current = entry.child;
                }
                CompiledKind::Retime {
                    child,
                    start,
                    scale,
                    pitch,
                    purpose,
                } => {
                    if stop_at_preserve && pitch.processes(*scale == ExactRatio::ONE) {
                        break (
                            stage_content(
                                AudioStage::for_node_admitted(
                                    self.plan,
                                    current,
                                    &repeats,
                                    self.definition.as_ref(),
                                    self.bypass_binding,
                                    budget,
                                )?,
                                budget,
                            )?,
                            None,
                        );
                    }
                    if *purpose != RetimePurpose::Partition {
                        let needed = retimes.len() + 1;
                        budget.grow(&mut retimes, &mut retimes_capacity, needed)?;
                        retimes.push(AudioRetimeStage {
                            node: budget.node(&mut memory, &node.inspection.id)?,
                            child_start: *start,
                            child_frames_per_local_frame: *scale,
                            pitch: *pitch,
                        });
                    }
                    let inverse = ExactRatio::ONE.checked_div(*scale)?;
                    transform = transform.child(
                        ExactRatio::ZERO.checked_sub(start.checked_mul(inverse)?)?,
                        inverse,
                    )?;
                    current = *child;
                }
                CompiledKind::Repeat {
                    layout,
                    gap_duration,
                    ..
                } => {
                    let location = layout
                        .locate_bounded(local, bias, budget.remaining)
                        .map_err(|error| {
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                                PlanError::AudioQueryLimit("structural work")
                            } else {
                                error.into()
                            }
                        })?;
                    budget.canonical(location.comparisons)?;
                    budget.lookup.iteration_run_comparisons += location.comparisons;
                    budget.own_iteration(&mut memory, &location.play.iteration)?;
                    if location.in_gap {
                        let start = location.play.start.checked_add(location.play.duration)?;
                        transform = transform.child(start, ExactRatio::ONE)?;
                        if let Some(child) = location.play.gap_child {
                            let needed = repeats.len() + 1;
                            budget.grow(&mut repeats, &mut repeats_capacity, needed)?;
                            repeats.push(RepeatInstance {
                                node: budget.node(&mut memory, &node.inspection.id)?,
                                iteration: location.play.iteration,
                            });
                            current = self.plan.by_id[&child];
                            continue;
                        }
                        gap = Some(DomainGap {
                            after: Some(location.play.iteration),
                            duration: *gap_duration,
                        });
                        continue;
                    }
                    transform = transform.child(location.play.start, ExactRatio::ONE)?;
                    let needed = repeats.len() + 1;
                    budget.grow(&mut repeats, &mut repeats_capacity, needed)?;
                    repeats.push(RepeatInstance {
                        node: budget.node(&mut memory, &node.inspection.id)?,
                        iteration: location.play.iteration,
                    });
                    current = self.plan.by_id[&location.play.child];
                }
            }
        };
        let allocated_samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
        if !allocated_samples.contains(&sample) {
            return Err(PlanError::InvalidPlan("audio signal interval did not advance").into());
        }
        let sampling = AudioSampleMap::new(
            allocated_samples.start,
            transform.local_at(allocated_samples.start)?,
            transform
                .signal_frames_per_sample
                .checked_div(transform.signal_frames_per_local_frame)?,
        )?;
        budget.check_memory(&memory)?;
        budget.check_memory(&repeats_capacity)?;
        budget.check_memory(&retimes_capacity)?;
        budget.merge_checked(&mut memory, &mut repeats_capacity)?;
        budget.check_memory(&memory)?;
        budget.merge_checked(&mut memory, &mut retimes_capacity)?;
        let fields = FinishFields {
            definition: budget.definition(&mut memory, &self.definition)?,
            samples: allocated_samples.clone(),
            allocated_samples,
            signal_extent: extent,
            instance: InstancePath {
                node: budget.node(&mut memory, &self.plan.nodes[current].inspection.id)?,
                repeats,
            },
            gap_after,
            transform,
            grid,
            sampling,
            retimes,
        };
        finish(
            content,
            Owned {
                value: fields,
                memory,
            },
            budget,
        )
    }
}
