//! Ordered additive preparation voices on one exact, plan-local point grid.
//!
//! This is a borrowed evaluation view, not an authored attachment or a master
//! bus. Each voice keeps its own complete content and policy queries. Gates
//! suppress only their named voices and never remove source dependencies.

use std::ops::Range;

use deadpan_core::{ExactRatio, ProjectId, RevisionId};

use crate::{
    AudioBoundaryRule, AudioContent, AudioPolicyQuery, AudioQueryLimits, AudioSampleGrid,
    AudioSignalContent, AudioSignalQuery, AudioSignalTape, LookupStats, PlanError, RenderPlan,
    SignalSample,
};

const MAX_MIX_VOICES: usize = 64;
const MAX_MIX_GATES: usize = 4096;
const MAX_GATE_REFERENCES: usize = 65_536;

/// An exact half-open suppression interval in the mix's frame clock. Voice
/// indices refer to the immutable order supplied to `AudioSignalMix::new`.
#[derive(Debug, Clone)]
pub struct AudioMixGate {
    range: Range<ExactRatio>,
    voices: Vec<usize>,
}

impl AudioMixGate {
    /// Validation is performed atomically with the owning mix, whose clock and
    /// voice count give this otherwise unbound instruction its meaning.
    pub fn new(range: Range<ExactRatio>, voices: Vec<usize>) -> Self {
        Self { range, voices }
    }

    pub fn range(&self) -> Range<ExactRatio> {
        self.range.clone()
    }

    pub fn voices(&self) -> &[usize] {
        &self.voices
    }
}

/// Content still partitions the requested interval separately for each voice.
/// `policy` retains source exhaustion and explicit silence for that voice;
/// `gates` adds only the selected mix gates on the same PointCeil grid.
#[derive(Debug)]
pub struct AudioMixVoiceQuery<'plan> {
    pub index: usize,
    pub signal: AudioSignalQuery<'plan>,
    pub policy: AudioPolicyQuery<SignalSample>,
    pub gates: Vec<Range<SignalSample>>,
}

#[derive(Debug)]
pub struct AudioMixQuery<'plan> {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub samples: Range<SignalSample>,
    pub voices: Vec<AudioMixVoiceQuery<'plan>>,
    /// Only intervals explicitly silent or suppressed in every voice. A silent
    /// or exhausted voice cannot erase another voice. This does not detect
    /// numeric cancellation or reinterpret opaque processed output as silence.
    pub suppressed: Vec<Range<SignalSample>>,
    pub lookup: LookupStats,
    pub work: usize,
}

#[derive(Debug, Clone)]
pub struct AudioSignalMix<'plan> {
    plan: &'plan RenderPlan,
    support: Range<ExactRatio>,
    grid: AudioSampleGrid<SignalSample>,
    count: SignalSample,
    voices: Vec<AudioSignalTape<'plan>>,
    gates: Vec<AudioMixGate>,
    voice_gates: Vec<Vec<Range<SignalSample>>>,
}

impl<'plan> AudioSignalMix<'plan> {
    pub fn new(
        plan: &'plan RenderPlan,
        voices: Vec<AudioSignalTape<'plan>>,
        gates: Vec<AudioMixGate>,
    ) -> Result<Self, PlanError> {
        if voices.is_empty() || voices.len() > MAX_MIX_VOICES {
            return Err(PlanError::AudioQueryLimit("audio mix voice count"));
        }
        if gates.len() > MAX_MIX_GATES {
            return Err(PlanError::AudioQueryLimit("audio mix gate count"));
        }
        let support = voices[0].support();
        let grid = voices[0].grid();
        if grid.boundary_rule() != AudioBoundaryRule::PointCeil
            || grid.frame_origin() != support.start
            || grid.boundary(support.start)? != SignalSample(0)
        {
            return Err(PlanError::InvalidPlan(
                "audio mix requires a normalized point grid",
            ));
        }
        let count = grid.boundary(support.end)?;
        for voice in &voices {
            if !voice.belongs_to(plan) {
                return Err(PlanError::InvalidPlan(
                    "audio mix voice belongs to another plan",
                ));
            }
            if voice.support() != support || voice.grid() != grid || !voice.covers_support() {
                return Err(PlanError::InvalidPlan(
                    "audio mix voices must share complete support and grid",
                ));
            }
            if voice.sample_count()? != count {
                return Err(PlanError::InvalidPlan(
                    "audio mix voice sample extent differs",
                ));
            }
        }
        let mut references = 0usize;
        let mut voice_gates = vec![Vec::new(); voices.len()];
        for gate in &gates {
            references = references
                .checked_add(gate.voices.len())
                .filter(|value| *value <= MAX_GATE_REFERENCES)
                .ok_or(PlanError::AudioQueryLimit("audio mix gate references"))?;
            if gate.voices.is_empty()
                || gate
                    .range
                    .end
                    .checked_sub(gate.range.start)?
                    .compare_integer(0)
                    .is_le()
                || gate
                    .range
                    .start
                    .checked_sub(support.start)?
                    .compare_integer(0)
                    .is_lt()
                || gate
                    .range
                    .end
                    .checked_sub(support.end)?
                    .compare_integer(0)
                    .is_gt()
            {
                return Err(PlanError::InvalidPlan(
                    "audio mix gate must select voices inside support",
                ));
            }
            let samples = grid.boundary(gate.range.start)?..grid.boundary(gate.range.end)?;
            let mut selected = [false; MAX_MIX_VOICES];
            for &index in &gate.voices {
                if index >= voices.len() || selected[index] {
                    return Err(PlanError::InvalidPlan(
                        "audio mix gate voice is absent or duplicated",
                    ));
                }
                selected[index] = true;
                if samples.start < samples.end {
                    voice_gates[index].push(samples.clone());
                }
            }
        }
        for ranges in &mut voice_gates {
            ranges.sort_unstable_by_key(|range| range.start);
            *ranges = merge_sorted(std::mem::take(ranges));
        }
        Ok(Self {
            plan,
            support,
            grid,
            count,
            voices,
            gates,
            voice_gates,
        })
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }

    pub fn support(&self) -> Range<ExactRatio> {
        self.support.clone()
    }

    pub fn grid(&self) -> AudioSampleGrid<SignalSample> {
        self.grid
    }

    pub fn sample_count(&self) -> Result<SignalSample, PlanError> {
        Ok(self.count)
    }

    pub fn voices(&self) -> &[AudioSignalTape<'plan>] {
        &self.voices
    }

    pub fn gates(&self) -> &[AudioMixGate] {
        &self.gates
    }

    /// One allowance covers all voice content, policy, gate lookup and interval
    /// composition. `maximum_spans` bounds total returned content spans; policy
    /// contents and masks additionally consume the shared work allowance.
    pub fn query(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioMixQuery<'plan>, PlanError> {
        limits.validate()?;
        if samples.start.0 < 0 || samples.end < samples.start || samples.end > self.count {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut budget = MixQueryBudget {
            limits,
            work: 0,
            spans: 0,
        };
        let mut voices = Vec::with_capacity(self.voices.len());
        let mut suppressed = vec![samples.clone()];
        if samples.is_empty() {
            suppressed.clear();
        }
        let mut lookup = LookupStats::default();
        for (index, tape) in self.voices.iter().enumerate() {
            budget.charge(1)?;
            let signal = tape.query(samples.clone(), budget.remaining()?)?;
            budget.charge(signal.work)?;
            budget.spans = budget
                .spans
                .checked_add(signal.spans.len())
                .filter(|value| *value <= limits.maximum_spans)
                .ok_or(PlanError::AudioQueryLimit("audio mix span count"))?;
            add_lookup(&mut lookup, signal.lookup)?;
            // Policy does not spend the content-partition span count again.
            let policy = tape.policy(samples.clone(), budget.policy_limits()?)?;
            budget.charge(policy.work)?;
            add_lookup(&mut lookup, policy.lookup)?;
            let gates = clipped_gates(&self.voice_gates[index], &samples, &mut budget)?;
            let effective = union(&policy.suppressed, &gates, &mut budget)?;
            // Policy masks describe authored suppression, not every zero input.
            // Absent audio and time before a Source placement are also known
            // silent here. Opaque stages may retain decay, so never flatten them
            // to infer zero output from their underlying source endpoints.
            let mut silent = Vec::new();
            for span in &signal.spans {
                budget.charge(1)?;
                if matches!(
                    span.content,
                    AudioSignalContent::Leaf(AudioContent::Silence { .. })
                ) {
                    silent.push(span.samples.clone());
                }
            }
            let effective = union(&effective, &silent, &mut budget)?;
            suppressed = intersection(&suppressed, &effective, &mut budget)?;
            voices.push(AudioMixVoiceQuery {
                index,
                signal,
                policy,
                gates,
            });
        }
        Ok(AudioMixQuery {
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            samples,
            voices,
            suppressed,
            lookup,
            work: budget.work,
        })
    }

    pub(crate) fn validate_projection_scope(
        &self,
        owner: &crate::AudioStage<'_>,
        budget: &mut crate::audio_projection::ProjectionValidationBudget,
        depth: usize,
    ) -> Result<usize, PlanError> {
        let mut relative_depth = 0;
        for voice in &self.voices {
            budget.edge()?;
            relative_depth =
                relative_depth.max(voice.validate_projection_scope(owner, budget, depth)?);
        }
        Ok(relative_depth)
    }
}

struct MixQueryBudget {
    limits: AudioQueryLimits,
    work: usize,
    spans: usize,
}

impl MixQueryBudget {
    fn charge(&mut self, amount: usize) -> Result<(), PlanError> {
        self.work = self
            .work
            .checked_add(amount)
            .filter(|value| *value <= self.limits.maximum_work)
            .ok_or(PlanError::AudioQueryLimit("audio mix work"))?;
        Ok(())
    }

    fn policy_limits(&self) -> Result<AudioQueryLimits, PlanError> {
        let maximum_work = self
            .limits
            .maximum_work
            .checked_sub(self.work)
            .filter(|value| *value > 0)
            .ok_or(PlanError::AudioQueryLimit("audio mix work"))?;
        Ok(AudioQueryLimits {
            maximum_work,
            ..self.limits
        })
    }

    fn remaining(&self) -> Result<AudioQueryLimits, PlanError> {
        let maximum_spans = self
            .limits
            .maximum_spans
            .checked_sub(self.spans)
            .filter(|value| *value > 0)
            .ok_or(PlanError::AudioQueryLimit("audio mix span count"))?;
        Ok(AudioQueryLimits {
            maximum_spans,
            ..self.policy_limits()?
        })
    }
}

fn clipped_gates(
    ranges: &[Range<SignalSample>],
    requested: &Range<SignalSample>,
    budget: &mut MixQueryBudget,
) -> Result<Vec<Range<SignalSample>>, PlanError> {
    let mut left = 0;
    let mut right = ranges.len();
    while left < right {
        budget.charge(1)?;
        let middle = left + (right - left) / 2;
        if ranges[middle].end <= requested.start {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    let mut output = Vec::new();
    for range in &ranges[left..] {
        budget.charge(1)?;
        if range.start >= requested.end {
            break;
        }
        let clipped = range.start.max(requested.start)..range.end.min(requested.end);
        if clipped.start < clipped.end {
            output.push(clipped);
        }
    }
    Ok(output)
}

fn merge_sorted(ranges: Vec<Range<SignalSample>>) -> Vec<Range<SignalSample>> {
    let mut output: Vec<Range<SignalSample>> = Vec::new();
    for range in ranges {
        if let Some(last) = output.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            output.push(range);
        }
    }
    output
}

fn union(
    left: &[Range<SignalSample>],
    right: &[Range<SignalSample>],
    budget: &mut MixQueryBudget,
) -> Result<Vec<Range<SignalSample>>, PlanError> {
    let (mut a, mut b) = (0, 0);
    let mut ordered = Vec::new();
    while a < left.len() || b < right.len() {
        budget.charge(1)?;
        if b == right.len() || (a < left.len() && left[a].start <= right[b].start) {
            ordered.push(left[a].clone());
            a += 1;
        } else {
            ordered.push(right[b].clone());
            b += 1;
        }
    }
    Ok(merge_sorted(ordered))
}

fn intersection(
    left: &[Range<SignalSample>],
    right: &[Range<SignalSample>],
    budget: &mut MixQueryBudget,
) -> Result<Vec<Range<SignalSample>>, PlanError> {
    let (mut a, mut b) = (0, 0);
    let mut output = Vec::new();
    while a < left.len() && b < right.len() {
        budget.charge(1)?;
        let overlap = left[a].start.max(right[b].start)..left[a].end.min(right[b].end);
        if overlap.start < overlap.end {
            output.push(overlap);
        }
        if left[a].end <= right[b].end {
            a += 1;
        } else {
            b += 1;
        }
    }
    Ok(output)
}

fn add_lookup(total: &mut LookupStats, next: LookupStats) -> Result<(), PlanError> {
    let add = |a: usize, b: usize| {
        a.checked_add(b)
            .ok_or(PlanError::AudioQueryLimit("audio mix lookup count"))
    };
    total.visited_nodes = add(total.visited_nodes, next.visited_nodes)?;
    total.sequence_comparisons = add(total.sequence_comparisons, next.sequence_comparisons)?;
    total.iteration_run_comparisons = add(
        total.iteration_run_comparisons,
        next.iteration_run_comparisons,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioSignalTapeRun;
    use deadpan_core::{
        BeatNode, ColorPolicy, FrameDuration, FrameRate, HoldAudio, HoldRecipe, HoldVideo, NodeId,
        PresentationBasis, ProjectDocument,
    };
    use std::collections::BTreeMap;

    #[test]
    fn remapped_policy_views_cannot_impersonate_a_complete_mix_clock() {
        let root = NodeId::new("root").unwrap();
        let hold = NodeId::new("hold").unwrap();
        let document = ProjectDocument::new(
            ProjectId::new("mix-clock").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(48_000, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            root.clone(),
        )
        .unwrap();
        let mut wire = serde_json::to_value(document).unwrap();
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (root, BeatNode::sequence("Root", vec![hold.clone()])),
            (
                hold,
                BeatNode::hold(
                    "Hold",
                    HoldRecipe {
                        duration: FrameDuration::new(1).unwrap(),
                        video: HoldVideo::Background,
                        picture_context: None,
                        audio: HoldAudio::Silence,
                    },
                ),
            ),
        ]))
        .unwrap();
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let plan = RenderPlan::compile(&document).unwrap();
        let support = ExactRatio::ZERO..ExactRatio::ONE;
        let tape = AudioSignalTape::new(
            &plan,
            support.clone(),
            vec![AudioSignalTapeRun::new(
                support.clone(),
                support.clone(),
                plan.audio_signal(),
            )],
        )
        .unwrap();
        let half = ExactRatio::new(1, 2).unwrap();
        for (route, origin, rule) in [
            (support.clone(), half, AudioBoundaryRule::PointCeil),
            (
                support.clone(),
                ExactRatio::ZERO,
                AudioBoundaryRule::RoundEven,
            ),
            (
                ExactRatio::ZERO..half,
                ExactRatio::ZERO,
                AudioBoundaryRule::PointCeil,
            ),
        ] {
            let remapped = tape
                .remap_policy_window(route.clone(), route, origin, ExactRatio::ONE, rule)
                .unwrap();
            assert_eq!(remapped.support(), support);
            assert!(AudioSignalMix::new(&plan, vec![tape.clone(), remapped], Vec::new()).is_err());
        }
    }
}
