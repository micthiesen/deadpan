//! Resolve persisted owned selectors into checked live preparation projections.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use deadpan_core::{ExactRatio, InstancePath, PreserveInputProvider};

use super::{AudioDefinitionSelector, AudioStage, RenderPlan};
use crate::{AudioSignalTape, AudioSignalTapeRun, AudioStageProjection, PlanError};

impl RenderPlan {
    /// The complete intrinsic output, before splice allocation. This resolves
    /// current recipes and nested intrinsic inputs from the saved owner clock.
    /// Ordinary root output still requires its separate retained placement.
    pub fn owned_preserve_projection(
        &self,
        selector: AudioDefinitionSelector,
    ) -> Result<Arc<AudioStageProjection<'_>>, PlanError> {
        let definition = self.audio_definition(selector.clone())?;
        if definition.gap.is_some() {
            return Err(PlanError::InvalidPlan(
                "a gap recipe is not an owned Preserve stage",
            ));
        }
        let instance = InstancePath {
            node: definition.root().clone(),
            repeats: Vec::new(),
        };
        Builder {
            plan: self,
            definition: selector,
            memo: BTreeMap::new(),
            active: BTreeSet::new(),
            runs: 0,
        }
        .compile(instance)
    }
}

struct Builder<'plan> {
    plan: &'plan RenderPlan,
    definition: AudioDefinitionSelector,
    memo: BTreeMap<InstancePath, Arc<AudioStageProjection<'plan>>>,
    active: BTreeSet<InstancePath>,
    runs: usize,
}

impl<'plan> Builder<'plan> {
    fn compile(
        &mut self,
        instance: InstancePath,
    ) -> Result<Arc<AudioStageProjection<'plan>>, PlanError> {
        if let Some(projection) = self.memo.get(&instance) {
            return Ok(Arc::clone(projection));
        }
        if self.memo.len() + self.active.len() >= crate::audio_projection::MAX_PROJECTED_STAGES
            || self.active.len() >= crate::audio_projection::MAX_PROJECTION_DEPTH
        {
            return Err(PlanError::AudioQueryLimit(
                "owned Preserve projection graph",
            ));
        }
        if !self.active.insert(instance.clone()) {
            return Err(PlanError::InvalidPlan("cyclic owned Preserve input"));
        }
        let node = *self
            .plan
            .by_id
            .get(&instance.node)
            .ok_or(PlanError::InvalidPlan("owned Preserve stage is missing"))?;
        let stage = AudioStage::for_owned_input(
            self.plan,
            node,
            instance.repeats.clone(),
            self.definition.clone(),
        )?;
        let input = self.plan.nodes[node]
            .preserve_input
            .as_ref()
            .ok_or(PlanError::InvalidPlan("owned Preserve input is absent"))?;
        self.runs = self
            .runs
            .checked_add(input.input.runs().len())
            .ok_or(PlanError::AudioQueryLimit("owned input runs"))?;
        if self.runs > deadpan_core::MAX_DOCUMENT_PRESERVE_INPUT_RUNS {
            return Err(PlanError::AudioQueryLimit("owned input runs"));
        }
        let mut runs = Vec::with_capacity(input.input.runs().len());
        let mut policy = Vec::with_capacity(input.input.runs().len());
        for run in input.input.runs() {
            let destination = run.destination.start..run.destination.end;
            let source = run.source.start..run.source.end;
            let output = run.destination.start.checked_div(input.rate)?
                ..run.destination.end.checked_div(input.rate)?;
            match &run.provider {
                PreserveInputProvider::Current { target } => {
                    let signal = stage.owned_descendant_signal(target)?;
                    runs.push(AudioSignalTapeRun::new(
                        destination,
                        source.clone(),
                        signal.clone(),
                    ));
                    policy.push(AudioSignalTapeRun::new(output, source, signal));
                }
                PreserveInputProvider::Intrinsic { target } => {
                    let mut nested = target.clone();
                    nested
                        .repeats
                        .splice(0..0, instance.repeats.iter().cloned());
                    let projection = self.compile(nested)?;
                    runs.push(AudioSignalTapeRun::intrinsic(
                        destination,
                        source.clone(),
                        Arc::clone(&projection),
                    ));
                    policy.push(AudioSignalTapeRun::intrinsic(output, source, projection));
                }
            }
        }
        let output_end = ExactRatio::integer(stage.descriptor().duration.frames());
        let tape = AudioSignalTape::new(self.plan, ExactRatio::ZERO..input.input.extent(), runs)?;
        let policy = AudioSignalTape::new(self.plan, ExactRatio::ZERO..output_end, policy)?;
        let projection = AudioStageProjection::new_owned(stage, tape, policy)?;
        self.active.remove(&instance);
        self.memo.insert(instance, Arc::clone(&projection));
        Ok(projection)
    }
}
