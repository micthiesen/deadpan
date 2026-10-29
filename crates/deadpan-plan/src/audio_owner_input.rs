//! Complete raw input contexts for post-time/pitch owner processing.

use deadpan_core::{AudioSample, ExactRatio, TimeError};

use super::audio::{AudioWalkSpan, Budget, EditorialCapture};
use super::{
    AudioBound, AudioBoundDomain, AudioDomain, AudioFadeQuery, AudioOwnerClock, AudioOwnerKind,
    AudioProcessingSpan, AudioQueryLimits, AudioSignalContent, LookupStats, RenderPlan,
};
use crate::PlanError;

#[path = "audio_owner_input/candidates.rs"]
mod candidates;
pub use candidates::{
    AdmittedAudioOwnerInputCandidates, AdmittedAudioOwnerInputCapture, AudioOwnerInputCandidates,
    AudioOwnerInputCapture,
};

/// Whether the raw context is this owner or a processor surrounding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOwnerInputScope {
    Owner,
    /// Preserve or a retained binding owns the complete raw processor. Its
    /// output may also contain other descendant owners. Never treat all of it
    /// as the requested owner's contribution or move an effect into its input.
    EnclosingProcessor,
}

/// A checked raw processing context, before authored treatments and independent
/// sounds. This is an input to effect planning, not a complete voice/mix graph.
/// The owner's clock is an observation at the resolving sample; query the
/// context's owners again when crossing a child, play or retained clock boundary.
/// A retained binding has a separate reference history: `retained_at` exposes
/// its complete operand and exact map beyond the current delivery interval.
#[derive(Debug, Clone)]
pub struct AudioOwnerInput<'plan> {
    owner: AudioOwnerClock<'plan>,
    observed_at: AudioSample,
    context: AudioDomain<'plan>,
    scope: AudioOwnerInputScope,
    work: usize,
}

impl<'plan> AudioOwnerInput<'plan> {
    pub fn owner(&self) -> &AudioOwnerClock<'plan> {
        &self.owner
    }
    /// The root sample at which this owner and its processing context were
    /// checked together. In particular, retained lookup must use this sample,
    /// not an arbitrary extrapolation of the owner's nominal clock.
    pub fn observed_at(&self) -> AudioSample {
        self.observed_at
    }
    /// Signed root-grid delivery context. It may span a whole Sequence or
    /// Repeat, and may include hidden samples outside current root allocation.
    /// `scope` identifies whether an enclosing processor must remain intact.
    /// This interval is not the complete reference history of a resumed binding;
    /// use `retained_at` to resolve that separate input instead of zero padding.
    pub fn context(&self) -> &AudioDomain<'plan> {
        &self.context
    }
    pub fn scope(&self) -> AudioOwnerInputScope {
        self.scope
    }
    pub fn work(&self) -> usize {
        self.work
    }

    /// Compatibility lookup for consumers that do not account preparation
    /// work. Controlled preparation should use retained_probe_at instead.
    pub fn retained_at(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<Option<AudioOwnerRetainedInput<'plan>>, PlanError> {
        Ok(self.retained_probe_at(at, limits)?.into_input())
    }

    /// The first opaque retained input at this context sample and the work of
    /// the same lookup, including a successful lookup that finds no input.
    /// Do not descend through an enclosing Preserve to find one: that would
    /// move post-time/pitch effects into its input. No PCM is opened here.
    pub fn retained_probe_at(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerRetainedProbe<'plan>, PlanError> {
        limits.validate()?;
        if !self.context.root_samples().contains(&at) {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut budget = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut editorial = EditorialCapture::default();
        let walked = self.context.plan.audio_walk_editorial(
            at,
            &mut budget,
            Some(&self.context.seed),
            (true, true),
            &mut editorial,
        );
        #[cfg(test)]
        super::audio_owners::tests::legacy_snapshot_v1::capture_baseline::probe_walk(
            self.context.plan,
            at,
            &walked,
            &editorial,
            (limits.maximum_work, budget.remaining),
        );
        let walked = walked?;
        let input = match walked {
            AudioWalkSpan::Stage(span) if matches!(span.content, AudioSignalContent::Bound(_)) => {
                let AudioSignalContent::Bound(bound) = &span.content else {
                    unreachable!()
                };
                let domain = bound.raw_domain()?;
                Some(AudioOwnerRetainedInput {
                    plan: self.context.plan,
                    processing: span,
                    editorial,
                    domain,
                    work: limits.maximum_work - budget.remaining,
                })
            }
            _ => None,
        };
        #[cfg(test)]
        super::audio_owners::tests::legacy_snapshot_v1::capture_baseline::probe_result(
            self.context.plan,
            input.as_ref(),
            limits.maximum_work - budget.remaining,
        );
        Ok(AudioOwnerRetainedProbe {
            input,
            work: limits.maximum_work - budget.remaining,
        })
    }
}

/// Result of one checked retained-input lookup. An absent operand still has
/// actual traversal work. Owns the same optional input without cloning it.
#[derive(Debug)]
pub struct AudioOwnerRetainedProbe<'plan> {
    input: Option<AudioOwnerRetainedInput<'plan>>,
    work: usize,
}

impl<'plan> AudioOwnerRetainedProbe<'plan> {
    pub fn input(&self) -> Option<&AudioOwnerRetainedInput<'plan>> {
        self.input.as_ref()
    }

    pub fn work(&self) -> usize {
        self.work
    }

    pub fn into_input(self) -> Option<AudioOwnerRetainedInput<'plan>> {
        self.input
    }
}

/// A retained processor's full raw operand and its exact map from consumer
/// sample labels. Reference coordinates stay on their RootRoundEven or PointCeil
/// grid; they are never fabricated by inverting a nominal automation clock.
#[derive(Debug, Clone)]
pub struct AudioOwnerRetainedInput<'plan> {
    pub(super) plan: &'plan RenderPlan,
    processing: AudioProcessingSpan<'plan>,
    editorial: EditorialCapture,
    domain: AudioBoundDomain<'plan>,
    work: usize,
}

impl<'plan> AudioOwnerRetainedInput<'plan> {
    #[cfg(test)]
    pub(in crate::plan) fn capture_baseline_snapshot(
        &self,
        expected_plan: &RenderPlan,
    ) -> serde_json::Value {
        use super::audio_owners::tests::legacy_snapshot_v1::capture_baseline as snapshot;
        assert!(self.belongs_to(expected_plan));
        assert!(std::ptr::eq(self.plan, expected_plan));
        let Self {
            plan: _,
            processing,
            editorial,
            domain,
            work,
        } = self;
        serde_json::json!({
            "belongs_to_expected_plan":true,
            "processing":snapshot::processing(expected_plan, processing),
            "editorial":snapshot::editorial(expected_plan, editorial),
            "raw_domain":snapshot::bound_domain(expected_plan, domain),
            "work":work,
        })
    }

    pub(super) fn clock_origin(&self) -> super::AudioOwnerClockOrigin {
        super::AudioOwnerClockOrigin::Retained {
            definition: self.processing.definition.clone(),
            instance: self.processing.instance.clone(),
            kind: if self.processing.gap_after.is_some() {
                AudioOwnerKind::DefaultGap
            } else {
                AudioOwnerKind::Node
            },
            gap_after: self.processing.gap_after.clone(),
        }
    }
    /// Checked immutable operand. Its offset origin is the original allocated
    /// start, retained by `reference_at`; serialized metadata cannot replace it.
    pub fn bound(&self) -> &AudioBound<'plan> {
        let AudioSignalContent::Bound(bound) = &self.processing.content else {
            unreachable!()
        };
        bound
    }
    pub fn domain(&self) -> &AudioBoundDomain<'plan> {
        &self.domain
    }
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        self.bound().belongs_to(plan)
    }
    pub fn reference_at(&self, sample: AudioSample) -> Result<ExactRatio, PlanError> {
        self.bound().reference_at_wide_offset(
            i128::from(sample.0) - i128::from(self.processing.allocated_samples.start.0),
        )
    }
    pub fn reference_samples_per_output_sample(&self) -> ExactRatio {
        self.bound().reference_samples_per_output_sample()
    }
    pub fn work(&self) -> usize {
        self.work
    }

    /// Creative output fades on the original consumer grid, including retained
    /// history outside current delivery. This does not grant output allocation
    /// or a silence/tail permission. Empty history remains a raw-reader policy.
    pub fn fades(
        &self,
        samples: std::ops::Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioFadeQuery, PlanError> {
        limits.validate()?;
        if samples.end < samples.start {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut budget = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut spans = Vec::new();
        let mut cursor = samples.start;
        while cursor < samples.end {
            if spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("retained fade span count"));
            }
            budget.spend(
                1 + self.bound().work()
                    + self.processing.retimes.len()
                    + self.processing.instance.repeats.len()
                    + self.editorial.editorial.len()
                    + self.editorial.constraints.len()
                    + self.editorial.repeats.len(),
            )?;
            let span = super::audio_fades::retained_bound_fade(
                self.processing.clone(),
                cursor,
                samples.end,
                self.editorial.clone(),
                &mut budget,
            )?;
            if span.samples.start != cursor
                || span.samples.end <= cursor
                || span.samples.end > samples.end
            {
                return Err(PlanError::InvalidPlan(
                    "retained fade interval did not advance",
                ));
            }
            cursor = span.samples.end;
            spans.push(span);
        }
        Ok(AudioFadeQuery {
            spans,
            lookup: budget.lookup,
            work: limits.maximum_work - budget.remaining,
        })
    }
}

impl RenderPlan {
    /// Resolve complete raw context for an owner observed at this root sample.
    /// Retain the first enclosing Preserve/binding instead of reconstructing its
    /// output from nominal automation coordinates. Full owner contribution,
    /// ordered treatments and per-voice silence/tail gates are separate work.
    pub fn audio_owner_input(
        &self,
        owner: &AudioOwnerClock<'_>,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInput<'_>, PlanError> {
        limits.validate()?;
        if !owner.belongs_to(self) {
            return Err(PlanError::InvalidPlan("foreign audio input owner"));
        }
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        let query = self.audio_gain_owners(at..end, limits)?;
        resolve_input(self, owner, at, limits, query, None)
    }
}

impl<'plan> AudioDomain<'plan> {
    /// Resolve a child or enclosing processor from this checked context even
    /// when its sample is hidden outside current project delivery. Membership
    /// and capture use the same seed and exact clock, never a root lookup.
    pub fn owner_input(
        &self,
        owner: &AudioOwnerClock<'_>,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInput<'plan>, PlanError> {
        limits.validate()?;
        if !owner.belongs_to(self.plan) {
            return Err(PlanError::InvalidPlan("foreign audio input owner"));
        }
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        let query = self.gain_owners(at..end, limits)?;
        resolve_input(self.plan, owner, at, limits, query, Some(&self.seed))
    }
}

fn resolve_input<'plan>(
    plan: &'plan RenderPlan,
    owner: &AudioOwnerClock<'_>,
    at: AudioSample,
    limits: AudioQueryLimits,
    query: super::AudioOwnerQuery<'plan>,
    context: Option<&super::audio_domain::AudioWalkSeed>,
) -> Result<AudioOwnerInput<'plan>, PlanError> {
    let observed = query
        .spans()
        .first()
        .and_then(|span| {
            span.owners()
                .iter()
                .find(|candidate| candidate.same_at(owner, at))
        })
        .ok_or(PlanError::InvalidPlan(
            "audio input owner is not current at sample",
        ))?;
    let mut budget = Budget {
        remaining: limits.maximum_work - query.work(),
        lookup: LookupStats::default(),
    };
    capture_observed(plan, observed, at, context, query.work(), &mut budget)
}

/// Capture one member of the actual checked owner query. Both the ordinary
/// membership API and sealed one-sample candidates use this exact walk.
fn capture_observed<'plan>(
    plan: &'plan RenderPlan,
    observed: &AudioOwnerClock<'plan>,
    at: AudioSample,
    context: Option<&super::audio_domain::AudioWalkSeed>,
    query_work: usize,
    budget: &mut Budget,
) -> Result<AudioOwnerInput<'plan>, PlanError> {
    use super::audio_owners::admission::{self, Unmetered};
    let mut policy = Unmetered;
    let mut admitted = admission::Budget::new(budget, &mut policy);
    admission::outer::<_, Unmetered>(
        capture_observed_admitted(plan, observed, at, context, query_work, &mut admitted)
            .map(admission::Owned::unmetered),
    )
}

fn capture_observed_admitted<'plan, A: crate::AudioSoundContextAdmission>(
    plan: &'plan RenderPlan,
    observed: &AudioOwnerClock<'plan>,
    at: AudioSample,
    context: Option<&super::audio_domain::AudioWalkSeed>,
    query_work: usize,
    budget: &mut super::audio_owners::admission::Budget<'_, A>,
) -> super::audio_owners::admission::Result<
    super::audio_owners::admission::Owned<AudioOwnerInput<'plan>, A::Issuer, A::Lease>,
    A,
> {
    #[cfg(test)]
    use super::audio_owners::admission::Error;
    use super::audio_owners::admission::Owned;
    let before = budget.remaining;
    let mut captured = Owned {
        value: None,
        memory: budget.memory(),
    };
    if A::METERED {
        let walked = plan.audio_walk_inner_admitted(
            at,
            budget,
            context,
            true,
            true,
            (Some(&mut captured), None, Some(observed)),
        );
        #[cfg(test)]
        match &walked {
            Ok(value) => {
                super::audio_owners::tests::legacy_snapshot_v1::capture_baseline::capture_ref(
                    plan,
                    at,
                    observed,
                    captured.value.as_ref(),
                    Ok(value.span()),
                    (before, budget.remaining),
                )
            }
            Err(Error::Plan(error)) => {
                super::audio_owners::tests::legacy_snapshot_v1::capture_baseline::capture_ref(
                    plan,
                    at,
                    observed,
                    captured.value.as_ref(),
                    Err(error),
                    (before, budget.remaining),
                )
            }
            Err(Error::Policy(_)) => {
                super::audio_owners::tests::legacy_snapshot_v1::capture_baseline::policy_failure(
                    plan,
                    at,
                    captured.value.as_ref(),
                    (before, budget.remaining),
                )
            }
        }
        // The COMPLETE actual span and all workspace/Core/Box guards end at
        // the legacy discard, before the observed owner's unpaid old clone.
        drop(walked?);
    } else {
        plan.audio_walk_owner_input(at, budget, observed, &mut captured.value, context)?;
        budget.canonical_used = budget
            .canonical_used
            .checked_add(before - budget.remaining)
            .ok_or(deadpan_core::TimeError::Overflow)?;
    }
    captured.try_map(budget, |seed, memory, budget| {
        let seed = seed.ok_or(PlanError::InvalidPlan("owner input was not captured"))?;
        let kind = if seed.walk.gap.is_some() {
            AudioOwnerKind::DefaultGap
        } else {
            AudioOwnerKind::Node
        };
        let scope = if seed.instance == *observed.instance()
            && kind == observed.kind()
            && seed.walk.gap.as_ref().and_then(|gap| gap.after.as_ref()) == observed.gap_after()
        {
            AudioOwnerInputScope::Owner
        } else {
            AudioOwnerInputScope::EnclosingProcessor
        };
        Ok(AudioOwnerInput {
            owner: budget.owner(memory, observed)?,
            observed_at: at,
            context: AudioDomain {
                plan,
                seed: seed.walk,
                samples: seed.samples,
                visible: seed.visible,
                instance: seed.instance,
                placement: None,
            },
            scope,
            work: query_work
                .checked_add(before - budget.remaining)
                .ok_or(deadpan_core::TimeError::Overflow)?,
        })
    })
}

#[cfg(test)]
mod capture_baseline_branding_tests {
    use super::*;

    #[test]
    fn actual_bound_and_retained_snapshots_reject_identical_foreign_plan() {
        use super::super::audio_owners::tests::legacy_snapshot_v1::capture_baseline as snapshot;
        let frozen: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/owner-query-baseline-v1/snapshot.json"
        ))
        .unwrap();
        let fixture = frozen["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["fixture"] == "nonunity_shift_fractional_point_ceil_source44100")
            .unwrap();
        let doc =
            deadpan_core::ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let actual_plan = RenderPlan::compile(&doc).unwrap();
        let foreign_plan = RenderPlan::compile(&doc).unwrap();
        let at = AudioSample(0);
        let owners = actual_plan
            .audio_gain_owners(at..AudioSample(1), Default::default())
            .unwrap();
        let owner = owners.spans()[0]
            .owners()
            .iter()
            .find(|v| v.instance().node.as_str() == "voice")
            .unwrap();
        let input = actual_plan
            .audio_owner_input(owner, at, Default::default())
            .unwrap();
        let probe = input.retained_probe_at(at, Default::default()).unwrap();
        let retained = probe.input().expect("real frozen fixture must reach Bound");
        assert!(retained.belongs_to(&actual_plan));
        assert!(!retained.belongs_to(&foreign_plan));
        assert!(matches!(
            retained.processing.content,
            AudioSignalContent::Bound(_)
        ));
        let good_processing = snapshot::processing(&actual_plan, &retained.processing);
        assert_eq!(good_processing["content"]["tag"], "Bound");
        assert_eq!(
            retained.capture_baseline_snapshot(&actual_plan)["belongs_to_expected_plan"],
            true
        );

        // Exercise the actual content(plan, Bound) caller using this exact
        // processing span, without a clone, replacement Box or second walk.
        let foreign_content = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            snapshot::processing(&foreign_plan, &retained.processing)
        }));
        assert!(foreign_content.is_err());

        let foreign_retained = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            retained.capture_baseline_snapshot(&foreign_plan)
        }));
        assert!(foreign_retained.is_err());
    }
}
