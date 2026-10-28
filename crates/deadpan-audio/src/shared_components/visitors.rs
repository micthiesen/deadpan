//! Per-phase temporal demand visitors used at the numerical error boundaries.
//! Requests borrow the one existing phase clock; no per-member clone, provider,
//! gain evaluation or native processing is part of visitation.

use super::*;

pub(super) struct TemporalDemand<'a, 'plan> {
    pub(super) clock: &'a AudioOwnerClock<'plan>,
    pub(super) kind: Temporal,
    pub(super) sample: AudioSample,
    pub(super) permissions: Permissions,
    pub(super) voice: Option<usize>,
}

impl<'plan> TemporalDemand<'_, 'plan> {
    pub(super) fn read(
        self,
        equalize: &mut impl FnMut(
            &AudioOwnerClock<'plan>,
            Temporal,
            AudioSample,
            Permissions,
            Option<usize>,
        ) -> Result<[f64; 2], StageAudioError>,
    ) -> Result<[f64; 2], StageAudioError> {
        equalize(
            self.clock,
            self.kind,
            self.sample,
            self.permissions,
            self.voice,
        )
    }
}

pub(super) struct EqualizerPhase<'a, 'plan> {
    pub(super) owner: &'a Owner<'plan>,
    pub(super) permitted: &'a permissions::SamplePermissions,
    pub(super) at: usize,
    pub(super) sample: AudioSample,
    pub(super) stage: usize,
    pub(super) permissions: &'a Permissions,
    pub(super) clock: Option<&'a AudioOwnerClock<'plan>>,
}

/// Replace the existing EQ member walk, after gain/sum finite checks. The
/// callback may mutate that member's PCM; the visitor itself uses only policy.
/// Metadata-only consumers can leave route.pcm empty and inspect each request.
/// The inspection hook runs before every member, including inactive members.
/// Runtime supplies a noop; additional metadata walks bill their actual visits.
pub(super) fn visit_equalizer<'plan>(
    routes: &mut [Route<'plan>],
    phase: EqualizerPhase<'_, 'plan>,
    mut inspect: impl FnMut() -> Result<(), StageAudioError>,
    mut visit: impl FnMut(
        usize,
        &mut Route<'plan>,
        TemporalDemand<'_, 'plan>,
    ) -> Result<(), StageAudioError>,
) -> Result<(), StageAudioError> {
    phase.permitted.validate(routes.len())?;
    for &(id, _) in &phase.owner.members {
        inspect()?;
        let route = &mut routes[id];
        if phase.permitted.contains(id, phase.at) {
            let request = TemporalDemand {
                clock: phase
                    .clock
                    .ok_or(PlanError::InvalidPlan("shared EQ has no admitted clock"))?,
                kind: Temporal::Equalizer(phase.stage),
                sample: phase.sample,
                permissions: *phase.permissions,
                voice: Some(route.voice),
            };
            visit(id, route, request)?;
        }
    }
    Ok(())
}

pub(super) struct WetPhase<'a, 'plan> {
    pub(super) sample: AudioSample,
    pub(super) permissions: &'a Permissions,
    pub(super) clock: Option<&'a AudioOwnerClock<'plan>>,
    pub(super) voice: usize,
    pub(super) sends: deadpan_core::TailSends,
}

/// The caller preserves its route/sample eligibility and pre-return numerical
/// flush. This shared effect walk emits exactly Reverb then Delay, with the
/// original level and missing-clock failure immediately before each callback.
/// The supplied mask/clock must already come from prepared policy; unlike EQ
/// member visitation this effect-only phase does not receive a flag carrier.
/// The hook precedes both effect probes, including absent sends, so a cold
/// consumer can bill partial work without a separate traversal.
pub(super) fn visit_wet_effects<'plan>(
    phase: WetPhase<'_, 'plan>,
    mut inspect: impl FnMut() -> Result<(), StageAudioError>,
    mut visit: impl FnMut(
        deadpan_core::GainDb,
        TemporalDemand<'_, 'plan>,
    ) -> Result<(), StageAudioError>,
) -> Result<(), StageAudioError> {
    for effect in [
        deadpan_core::TailEffect::Reverb,
        deadpan_core::TailEffect::Delay,
    ] {
        inspect()?;
        if let Some(level) = phase.sends.level(effect) {
            visit(
                level,
                TemporalDemand {
                    clock: phase
                        .clock
                        .ok_or(PlanError::InvalidPlan("wet return has no admitted clock"))?,
                    kind: Temporal::Wet(effect),
                    sample: phase.sample,
                    permissions: *phase.permissions,
                    voice: Some(phase.voice),
                },
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "visitor_tests.rs"]
mod tests;
