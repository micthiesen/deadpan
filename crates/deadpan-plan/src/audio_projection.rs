//! Immutable plan-local views of a Preserve stage's intrinsic input and output policy.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use deadpan_core::{ExactRatio, FrameDuration};
use serde::{Serialize, Serializer, ser::SerializeStruct};

use crate::{AudioSignalMix, AudioSignalTape, AudioStage, PlanError, RenderPlan, SignalSample};

/// Opaque call-local identity for memoizing one immutable projection.
#[derive(Debug, Clone)]
pub struct AudioProjectionIdentity(Arc<ProjectionIdentityToken>);

#[derive(Debug)]
struct ProjectionIdentityToken {
    _private: u8,
}

impl PartialEq for AudioProjectionIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for AudioProjectionIdentity {}

impl AudioProjectionIdentity {
    pub(crate) fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
}

/// A checked intrinsic projection of one live Preserve stage.
///
/// The input and output-policy views use independent exact frame domains. The
/// input is normalized to zero and spans `duration * authored_rate`; output
/// policy is normalized to zero and spans `duration`. A mixed input is summed
/// before this one Preserve stage, never stretched independently per voice.
pub struct AudioStageProjection<'plan> {
    stage: AudioStage<'plan>,
    input: ProjectionInput<'plan>,
    output_policy: AudioSignalTape<'plan>,
    duration: FrameDuration,
    identity: AudioProjectionIdentity,
}

enum ProjectionInput<'plan> {
    Tape(AudioSignalTape<'plan>),
    Mix(AudioSignalMix<'plan>),
}

impl ProjectionInput<'_> {
    fn belongs_to(&self, plan: &RenderPlan) -> bool {
        match self {
            Self::Tape(tape) => tape.belongs_to(plan),
            Self::Mix(mix) => mix.belongs_to(plan),
        }
    }

    fn support(&self) -> Range<ExactRatio> {
        match self {
            Self::Tape(tape) => tape.support(),
            Self::Mix(mix) => mix.support(),
        }
    }

    fn sample_count(&self) -> Result<SignalSample, PlanError> {
        match self {
            Self::Tape(tape) => tape.sample_count(),
            Self::Mix(mix) => mix.sample_count(),
        }
    }

    fn validate_projection_scope(
        &self,
        owner: &AudioStage<'_>,
        budget: &mut ProjectionValidationBudget,
        depth: usize,
    ) -> Result<usize, PlanError> {
        match self {
            Self::Tape(tape) => tape.validate_projection_scope(owner, budget, depth),
            Self::Mix(mix) => mix.validate_projection_scope(owner, budget, depth),
        }
    }
}

impl fmt::Debug for AudioStageProjection<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioStageProjection")
            .field("stage", self.stage.descriptor())
            .field("duration", &self.duration)
            .finish()
    }
}

impl PartialEq for AudioStageProjection<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for AudioStageProjection<'_> {}

impl<'plan> AudioStageProjection<'plan> {
    pub fn new(
        stage: AudioStage<'plan>,
        input_tape: AudioSignalTape<'plan>,
        output_policy: AudioSignalTape<'plan>,
        duration: FrameDuration,
    ) -> Result<Arc<Self>, PlanError> {
        Self::build(
            stage,
            ProjectionInput::Tape(input_tape),
            output_policy,
            duration,
        )
    }

    pub fn new_mixed_input(
        stage: AudioStage<'plan>,
        input_mix: AudioSignalMix<'plan>,
        output_policy: AudioSignalTape<'plan>,
        duration: FrameDuration,
    ) -> Result<Arc<Self>, PlanError> {
        Self::build(
            stage,
            ProjectionInput::Mix(input_mix),
            output_policy,
            duration,
        )
    }

    fn build(
        stage: AudioStage<'plan>,
        input: ProjectionInput<'plan>,
        output_policy: AudioSignalTape<'plan>,
        duration: FrameDuration,
    ) -> Result<Arc<Self>, PlanError> {
        if duration == FrameDuration::ZERO {
            return Err(PlanError::InvalidPlan(
                "projected Preserve duration must be positive",
            ));
        }
        if duration.frames() > stage.descriptor().duration.frames() {
            return Err(PlanError::InvalidPlan(
                "projected Preserve duration exceeds its live stage",
            ));
        }
        let plan = stage.plan();
        if !input.belongs_to(plan) || !output_policy.belongs_to(plan) {
            return Err(PlanError::InvalidPlan(
                "projected Preserve tapes belong to another plan",
            ));
        }
        let input_frames =
            ExactRatio::integer(duration.frames()).checked_mul(stage.descriptor().rate)?;
        let expected_input = ExactRatio::ZERO..input_frames;
        let expected_output = ExactRatio::ZERO..ExactRatio::integer(duration.frames());
        if input.support() != expected_input {
            return Err(PlanError::InvalidPlan(
                "projected Preserve input does not match its intrinsic rate and duration",
            ));
        }
        if output_policy.support() != expected_output {
            return Err(PlanError::InvalidPlan(
                "projected Preserve output policy has the wrong intrinsic extent",
            ));
        }
        let projection = Arc::new(Self {
            stage,
            input,
            output_policy,
            duration,
            identity: AudioProjectionIdentity(Arc::new(ProjectionIdentityToken { _private: 0 })),
        });
        let mut budget = ProjectionValidationBudget::default();
        projection.validate_graph(&mut budget, 0)?;
        Ok(projection)
    }

    pub fn stage(&self) -> &AudioStage<'plan> {
        &self.stage
    }

    pub fn input_tape(&self) -> Option<&AudioSignalTape<'plan>> {
        match &self.input {
            ProjectionInput::Tape(tape) => Some(tape),
            ProjectionInput::Mix(_) => None,
        }
    }

    pub fn input_mix(&self) -> Option<&AudioSignalMix<'plan>> {
        match &self.input {
            ProjectionInput::Mix(mix) => Some(mix),
            ProjectionInput::Tape(_) => None,
        }
    }

    pub fn input_support(&self) -> Range<ExactRatio> {
        self.input.support()
    }

    pub fn input_sample_count(&self) -> Result<SignalSample, PlanError> {
        self.input.sample_count()
    }

    pub fn output_policy(&self) -> &AudioSignalTape<'plan> {
        &self.output_policy
    }

    pub fn output_frames(&self) -> i64 {
        self.duration.frames()
    }

    pub fn output_duration(&self) -> FrameDuration {
        self.duration
    }

    pub fn identity(&self) -> AudioProjectionIdentity {
        self.identity.clone()
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        self.stage.belongs_to(plan)
    }

    pub(crate) fn identity_key(&self) -> usize {
        self.identity.key()
    }

    pub(crate) fn validate_graph(
        &self,
        budget: &mut ProjectionValidationBudget,
        depth: usize,
    ) -> Result<usize, PlanError> {
        if depth > MAX_PROJECTION_DEPTH {
            return Err(PlanError::AudioQueryLimit("projected Preserve depth"));
        }
        let key = self.identity_key();
        if let Some(relative_depth) = budget.validated.get(&key).copied() {
            if depth
                .checked_add(relative_depth)
                .is_none_or(|value| value > MAX_PROJECTION_DEPTH)
            {
                return Err(PlanError::AudioQueryLimit("projected Preserve depth"));
            }
            return Ok(relative_depth);
        }
        budget.unique = budget
            .unique
            .checked_add(1)
            .ok_or(PlanError::AudioQueryLimit("projected Preserve count"))?;
        if budget.unique > MAX_PROJECTED_STAGES {
            return Err(PlanError::AudioQueryLimit("projected Preserve count"));
        }
        let mut child_depth = 0;
        child_depth = child_depth.max(self.input.validate_projection_scope(
            &self.stage,
            budget,
            depth + 1,
        )?);
        child_depth = child_depth.max(self.output_policy.validate_projection_scope(
            &self.stage,
            budget,
            depth + 1,
        )?);
        let relative_depth = 1 + child_depth;
        if depth
            .checked_add(relative_depth)
            .is_none_or(|value| value > MAX_PROJECTION_DEPTH)
        {
            return Err(PlanError::AudioQueryLimit("projected Preserve depth"));
        }
        budget.validated.insert(key, relative_depth);
        Ok(relative_depth)
    }
}

const MAX_PROJECTED_STAGES: usize = 64;
const MAX_PROJECTION_EDGES: usize = 65_536;
const MAX_PROJECTION_DEPTH: usize = 64;

#[derive(Debug, Default)]
pub(crate) struct ProjectionValidationBudget {
    unique: usize,
    edges: usize,
    validated: HashMap<usize, usize>,
}

impl ProjectionValidationBudget {
    pub(crate) fn edge(&mut self) -> Result<(), PlanError> {
        self.edges = self
            .edges
            .checked_add(1)
            .ok_or(PlanError::AudioQueryLimit("projected Preserve edges"))?;
        if self.edges > MAX_PROJECTION_EDGES {
            return Err(PlanError::AudioQueryLimit("projected Preserve edges"));
        }
        Ok(())
    }
}

impl Serialize for AudioStageProjection<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AudioStageProjection", 2)?;
        state.serialize_field("stage", self.stage.descriptor())?;
        state.serialize_field("duration", &self.duration)?;
        state.end()
    }
}
