//! Public Signal policy retains the provider-aware query and exact final mask.
use super::*;

impl AudioSignal<'_> {
    /// Current Signal policy on the public PointCeil grid, with construction admission.
    pub fn policy_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        self.policy_admitted_inner(samples, limits, admission, true)
    }

    /// Policy after Preserve, without source-placement endpoint gates.
    pub fn policy_after_preserve_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        self.policy_admitted_inner(samples, limits, admission, false)
    }

    fn policy_admitted_inner<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
        source_endpoints: bool,
    ) -> std::result::Result<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let query = if source_endpoints {
                self.policy(samples, limits)
            } else {
                self.policy_after_preserve(samples, limits)
            }
            .map_err(A::Error::from)?;
            return outer::<_, A>(point::ordinary_standalone_signal(query, admission));
        }
        limits.validate()?;
        let mut local = LocalBudget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        outer::<_, A>((|| {
            let mut state = State::new(limits.maximum_spans, &budget);
            let ranges = signal_policy(
                self,
                samples.start.0..samples.end.0,
                AudioBoundaryRule::PointCeil,
                &mut state,
                &mut budget,
                0,
                source_endpoints,
            )?;
            point::finish_standalone_signal(state, ranges, limits, &mut budget)
        })())
    }
}
