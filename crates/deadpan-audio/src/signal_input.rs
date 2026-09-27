//! Closed preparation inputs. Mixed inputs retain independent voice policy and
//! are evaluated through the scoped mix reader before enclosing DSP.

use std::ops::Range;

use deadpan_plan::{
    AudioPolicyQuery, AudioQueryLimits, AudioSignal, AudioSignalMix, AudioSignalQuery,
    AudioSignalTape, AudioStageProjection, PlanError, SignalSample,
};

#[derive(Clone, Copy)]
pub(super) enum SignalInput<'signal, 'plan> {
    Signal(&'signal AudioSignal<'plan>),
    Tape(&'signal AudioSignalTape<'plan>),
    PreserveTape(&'signal AudioSignalTape<'plan>),
    Mix(&'signal AudioSignalMix<'plan>),
}

impl<'signal, 'plan> From<&'signal AudioSignalMix<'plan>> for SignalInput<'signal, 'plan> {
    fn from(value: &'signal AudioSignalMix<'plan>) -> Self {
        Self::Mix(value)
    }
}

impl<'signal, 'plan> TryFrom<&'signal AudioStageProjection<'plan>> for SignalInput<'signal, 'plan> {
    type Error = PlanError;

    fn try_from(value: &'signal AudioStageProjection<'plan>) -> Result<Self, Self::Error> {
        match (value.input_tape(), value.input_mix()) {
            (Some(tape), None) => Ok(Self::Tape(tape)),
            (None, Some(mix)) => Ok(Self::Mix(mix)),
            _ => Err(PlanError::InvalidPlan("invalid intrinsic input kind")),
        }
    }
}

impl<'signal, 'plan> From<&'signal AudioSignal<'plan>> for SignalInput<'signal, 'plan> {
    fn from(value: &'signal AudioSignal<'plan>) -> Self {
        Self::Signal(value)
    }
}

impl<'signal, 'plan> From<&'signal AudioSignalTape<'plan>> for SignalInput<'signal, 'plan> {
    fn from(value: &'signal AudioSignalTape<'plan>) -> Self {
        Self::Tape(value)
    }
}

impl<'plan> SignalInput<'_, 'plan> {
    pub(super) fn query(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        match self {
            Self::Signal(signal) => signal.query(samples, limits),
            Self::Tape(tape) | Self::PreserveTape(tape) => tape.query(samples, limits),
            Self::Mix(_) => Err(PlanError::InvalidPlan("mix requires scoped voice queries")),
        }
    }

    pub(super) fn policy(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
        match self {
            Self::Signal(signal) => signal.policy(samples, limits),
            Self::Tape(tape) => tape.policy(samples, limits),
            Self::PreserveTape(tape) => tape.policy_after_preserve(samples, limits),
            Self::Mix(_) => Err(PlanError::InvalidPlan("mix requires scoped voice policy")),
        }
    }
}
