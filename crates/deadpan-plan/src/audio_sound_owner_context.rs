//! Full current-structure history for independent sounds entering an owner.
//! Original resume bindings are intentionally not a source of sound clocks.

use super::*;
use crate::plan::audio_domain::AudioWalkSeed;

/// A checked current occurrence and its complete sound-processing allocation.
/// Transparent ancestors may hide this allocation from project delivery.
/// An enclosing Preserve remains opaque and keeps its complete selected input.
/// This handle grants neither a sound permission nor access to source bytes.
#[derive(Debug, Clone)]
pub struct AudioSoundOwnerContext<'plan> {
    pub(in crate::plan) plan: &'plan RenderPlan,
    pub(in crate::plan) owner: InstancePath,
    pub(in crate::plan) owner_node: usize,
    pub(in crate::plan) policy: Arc<AudioDomain<'plan>>,
    pub(in crate::plan) processing_node: usize,
    pub(in crate::plan) processing_instance: InstancePath,
    work: usize,
}

impl RenderPlan {
    /// Resolve independent sound history for a checked concrete treatment
    /// owner. Sound clocks follow current structure even when this Original
    /// owner's nominal clock comes from an earlier retained allocation.
    pub fn audio_sound_owner_context(
        &self,
        owner: &crate::AudioOwnerClock<'_>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSoundOwnerContext<'_>, PlanError> {
        limits.validate()?;
        if !owner.belongs_to(self)
            || owner.definition().is_some()
            || owner.kind() != crate::AudioOwnerKind::Node
        {
            return Err(PlanError::InvalidAudioSourceOccurrence(
                "sound context requires a current concrete owner",
            ));
        }
        self.audio_sound_instance_context(owner.instance(), limits)
    }

    /// Resolve current sound history from an explicit structural occurrence.
    /// This is also used after a saved sound-clock scope has proved a historical
    /// alias. The complete path and stable plays are checked by this plan;
    /// neither nominal placement nor a matching node name grants membership.
    pub fn audio_sound_instance_context(
        &self,
        instance: &InstancePath,
        limits: AudioQueryLimits,
    ) -> Result<AudioSoundOwnerContext<'_>, PlanError> {
        limits.validate()?;
        if instance.repeats.len() > MAX_DOCUMENT_DEPTH {
            return Err(PlanError::AudioQueryLimit("sound context depth"));
        }
        let node =
            *self
                .by_id
                .get(&instance.node)
                .ok_or(PlanError::InvalidAudioSourceOccurrence(
                    "sound context owner is absent",
                ))?;
        let mut budget = BuildBudget {
            maximum: limits.maximum_work,
            remaining: limits.maximum_work,
            maximum_runs: limits.maximum_spans,
            runs: 0,
        };
        let (edges, _) = resolve_edges(self, node, instance, &mut budget)?;
        let mut map = Placement::IDENTITY;
        let mut processing = None;
        for edge in &edges {
            budget.spend(1)?;
            if processing.is_none()
                && matches!(self.nodes[edge.parent].kind,
                    CompiledKind::Retime { pitch, scale, .. } if pitch.processes(scale == ExactRatio::ONE))
            {
                processing = Some((edge.parent, map, edge.repeats));
            }
            map = map.compose(edge.map)?;
        }
        let (processing_node, processing_map, repeats) =
            processing.unwrap_or((node, map, instance.repeats.len()));
        let extent = processing_map.range(node_support(self, processing_node))?;
        let rate = self.metadata.presentation_basis.frame_rate;
        let step = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let grid = AudioSampleGrid::<AudioSample>::new(
            ExactRatio::ZERO,
            step,
            AudioBoundaryRule::RoundEven,
        )?;
        let samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
        budget.spend(1 + repeats + instance.repeats.len())?;
        let processing_instance = InstancePath {
            node: self.nodes[processing_node].inspection.id.clone(),
            repeats: instance.repeats[..repeats].to_vec(),
        };
        let policy = AudioDomain {
            plan: self,
            seed: AudioWalkSeed {
                definition: None,
                bypass_binding: None,
                node: processing_node,
                transform: AudioTransform {
                    project_origin: processing_map.origin,
                    project_frames_per_local_frame: processing_map.scale,
                    project_frames_per_sample: step,
                },
                grid,
                extent,
                envelope: None,
                constraints: Vec::new(),
                editorial: Vec::new(),
                suppress_entry_editorial: Default::default(),
                repeats: processing_instance.repeats.clone(),
                retimes: Vec::new(),
                gap: None,
            },
            samples: samples.clone(),
            visible: samples,
            instance: processing_instance.clone(),
            placement: None,
        };
        Ok(AudioSoundOwnerContext {
            plan: self,
            owner: instance.clone(),
            owner_node: node,
            policy: Arc::new(policy),
            processing_node,
            processing_instance,
            work: budget.maximum - budget.remaining,
        })
    }
}

impl<'plan> AudioSoundOwnerContext<'plan> {
    /// Complete current processing domain for Original input sharing this
    /// checked sound owner. Retained Original clocks still resolve separately.
    pub fn processing_domain(&self) -> &AudioDomain<'plan> {
        &self.policy
    }
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
    pub fn owner(&self) -> &InstancePath {
        &self.owner
    }
    /// Meaningful full processing support on the current absolute sample grid.
    /// May begin before zero or end beyond project delivery.
    pub fn samples(&self) -> Range<AudioSample> {
        self.policy.root_samples()
    }
    pub fn construction_work(&self) -> usize {
        self.work
    }

    pub(super) fn clip_boundary(
        &self,
        plan: &RenderPlan,
        host: usize,
        instance: &InstancePath,
        edges: &[Edge],
    ) -> Result<usize, PlanError> {
        let matches = |node: usize, repeats: usize, expected: &InstancePath| {
            plan.nodes[node].inspection.id == expected.node
                && instance.repeats.get(..repeats) == Some(expected.repeats.as_slice())
        };
        if !self.belongs_to(plan)
            || !(matches(host, instance.repeats.len(), &self.owner)
                || edges
                    .iter()
                    .any(|edge| matches(edge.parent, edge.repeats, &self.owner)))
        {
            return Err(PlanError::InvalidAudioSourceOccurrence(
                "sound occurrence is outside its checked owner context",
            ));
        }
        if host == self.processing_node {
            return Ok(edges.len());
        }
        edges
            .iter()
            .position(|edge| matches(edge.parent, edge.repeats, &self.processing_instance))
            .ok_or(PlanError::InvalidAudioSourceOccurrence(
                "sound processing context is absent",
            ))
    }
}
