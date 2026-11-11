//! Signal policy owns its final allocation independently of inventory guards.
use super::*;
use crate::AudioPointDomain;

/// Borrowed policy inputs before the final mask's exclusive custody transfer.
#[derive(Debug, Clone, Copy)]
pub struct SignalPolicyView<'value> {
    pub suppressed: &'value [Range<SignalSample>],
    pub contents: &'value [AudioContent],
    pub lookup: LookupStats,
    pub work: usize,
}

/// Sealed provenance for the exact final allocation, never an inventory lease.
pub struct SignalPolicyMaskRef<'value, I, L> {
    issuer: I,
    capacity: usize,
    lease: Option<&'value L>,
}
impl<I: Copy, L> SignalPolicyMaskRef<'_, I, L> {
    pub fn issuer(&self) -> I {
        self.issuer
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn lease(&self) -> Option<&L> {
        self.lease
    }
}

/// Trusted accounting backend for one exact Signal policy allocation.
///
/// As with AudioSoundContextAdmission::merge, implementations must uphold the
/// publication contract. The source and native token constructors stay closed;
/// this cross-crate backend trait is not a globally sealed extraction barrier.
pub trait AudioSignalPolicyMaskHandoff: AudioSoundContextAdmission {
    type PreparedMask;
    type Mask;

    /// Admit and physically prepare only the destination shell/storage while
    /// the complete source query, final Vec and its exact lease remain alive.
    ///
    /// On success, the issuer and exact lease must have been revalidated after
    /// all reservations. For a transferring lease, establish exclusive ledger
    /// access with try_borrow_mut, precompute the remaining ledger and final
    /// charge totals, then end every borrow. Retain no Ref/RefMut or caller
    /// callback. A capacity0 source must have no lease or native allocation.
    /// The returned token is
    /// for this exact source and must be consumed immediately by commit.
    fn prepare_signal_mask(
        &mut self,
        source: SignalPolicyMaskRef<'_, Self::Issuer, Self::Lease>,
    ) -> std::result::Result<Self::PreparedMask, Self::Error>;

    /// One infallible, non-unwinding, allocation-free publication tail.
    ///
    /// Move this exact Vec into prepared owning storage before disarming its
    /// exact source lease. Native custody must already cover every transferred
    /// frame. Use precomputed totals, never recompute arithmetic or store a
    /// stale atomic account total. Do not call users, check cancellation,
    /// emit traces, format messages, or defer/store/reuse the token.
    fn commit_signal_mask(
        &mut self,
        prepared: Self::PreparedMask,
        ranges: Vec<Range<SignalSample>>,
        source_lease: Option<&mut Self::Lease>,
    ) -> Self::Mask;
}

#[derive(Clone, Copy)]
enum PolicyOrigin {
    Point,
    Tape,
    Signal,
}
impl PolicyOrigin {
    fn unmetered(self) -> &'static str {
        match self {
            Self::Point => "unmetered Point policy mask handoff",
            Self::Tape => "unmetered Tape policy mask handoff",
            Self::Signal => "unmetered Signal policy mask handoff",
        }
    }
    fn capacity(self) -> &'static str {
        match self {
            Self::Point => "Point policy mask capacity lease",
            Self::Tape => "Tape policy mask capacity lease",
            Self::Signal => "Signal policy mask capacity lease",
        }
    }
    fn work(self) -> &'static str {
        match self {
            Self::Point => "Point policy query work provenance mismatch",
            Self::Tape => "Tape policy query work provenance mismatch",
            Self::Signal => "Signal policy query work provenance mismatch",
        }
    }
}

struct FinalSignalMask<I, L> {
    ranges: Vec<Range<SignalSample>>,
    memory: Memory<I, L>,
}
struct SignalPolicyInventory<I, L> {
    contents: Vec<AudioContent>,
    _contents_capacity: Memory<I, L>,
}

/// Actual Signal policy metadata and its separately transferable final mask.
pub struct AdmittedSignalPolicyQuery<I, L> {
    origin: PolicyOrigin,
    mask: FinalSignalMask<I, L>,
    inventory: GuardedBound<SignalPolicyInventory<I, L>, I, L>,
    lookup: LookupStats,
    work: usize,
    canonical: usize,
    additional: usize,
    forwarded: bool,
}
impl<I: Copy + Eq, L> AdmittedSignalPolicyQuery<I, L> {
    pub fn view(&self) -> SignalPolicyView<'_> {
        SignalPolicyView {
            suppressed: &self.mask.ranges,
            contents: &self.inventory.contents,
            lookup: self.lookup,
            work: self.work,
        }
    }
    pub fn work(&self) -> usize {
        self.work
    }
    pub fn canonical_units(&self) -> usize {
        self.canonical
    }
    pub fn additional_units(&self) -> usize {
        self.additional
    }
    pub fn was_forwarded(&self) -> bool {
        self.forwarded
    }

    /// Consume this exact admitted final allocation through a trusted backend.
    ///
    /// The inventory remains alive through preparation and publication. There
    /// is deliberately no callback, check or destructor between successful
    /// prepare and commit: its exclusive-ledger preflight must remain valid.
    pub fn into_suppression<A>(self, admission: &mut A) -> std::result::Result<A::Mask, A::Error>
    where
        A: AudioSignalPolicyMaskHandoff<Issuer = I, Lease = L>,
    {
        if !A::METERED || !self.forwarded {
            return Err(PlanError::InvalidPlan(self.origin.unmetered()).into());
        }
        if self.mask.memory.issuer != admission.issuer() {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        let capacity = self.mask.ranges.capacity();
        if (capacity == 0) != self.mask.memory.lease.is_none() {
            return Err(PlanError::InvalidPlan(self.origin.capacity()).into());
        }
        let prepared = admission.prepare_signal_mask(SignalPolicyMaskRef {
            issuer: self.mask.memory.issuer,
            capacity,
            lease: self.mask.memory.lease.as_ref(),
        })?;
        let Self {
            origin: _,
            mask,
            inventory,
            lookup: _,
            work: _,
            canonical: _,
            additional: _,
            forwarded: _,
        } = self;
        let FinalSignalMask { ranges, mut memory } = mask;
        let result = admission.commit_signal_mask(prepared, ranges, memory.lease.as_mut());
        drop(memory);
        drop(inventory);
        Ok(result)
    }
}

fn ordinary_signal<A: AudioSoundContextAdmission>(
    query: AudioPolicyQuery<SignalSample>,
    admission: &A,
) -> QueryResult<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A> {
    if A::METERED {
        return Err(PlanError::InvalidPlan("Point policy accounting mode mismatch").into());
    }
    let AudioPolicyQuery {
        suppressed,
        contents,
        lookup,
        work,
    } = query;
    let issuer = admission.issuer();
    Ok(AdmittedSignalPolicyQuery {
        origin: PolicyOrigin::Point,
        mask: FinalSignalMask {
            ranges: suppressed,
            memory: Memory::empty(issuer),
        },
        inventory: GuardedBound::plain(
            SignalPolicyInventory {
                contents,
                _contents_capacity: Memory::empty(issuer),
            },
            issuer,
        ),
        lookup,
        work,
        canonical: work,
        additional: 0,
        forwarded: false,
    })
}

impl AudioPointDomain<'_> {
    /// Current and retained policy on this exact domain's PointCeil grid.
    pub fn policy_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let query = self
                .signal_ref()
                .policy(samples, limits)
                .map_err(A::Error::from)?;
            return outer::<_, A>(ordinary_signal(query, admission));
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
                self.signal_ref(),
                samples.start.0..samples.end.0,
                AudioBoundaryRule::PointCeil,
                &mut state,
                &mut budget,
                0,
                true,
            )?;
            finish_signal(state, ranges, limits, &mut budget)
        })())
    }
}

fn sift_signal_ranges<A: AudioSoundContextAdmission>(
    ranges: &mut [Range<i64>],
    mut root: usize,
    end: usize,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    loop {
        budget.additional(1)?;
        let Some(mut child) = root.checked_mul(2).and_then(|value| value.checked_add(1)) else {
            return Err(TimeError::Overflow.into());
        };
        if child >= end {
            return Ok(());
        }
        let right = child.checked_add(1).ok_or(TimeError::Overflow)?;
        if right < end {
            budget.additional(1)?;
            if ranges[child].start < ranges[right].start {
                child = right;
            }
        }
        budget.additional(1)?;
        if ranges[root].start >= ranges[child].start {
            return Ok(());
        }
        budget.additional(1)?;
        ranges.swap(root, child);
        root = child;
    }
}
fn sort_signal_ranges<A: AudioSoundContextAdmission>(
    ranges: &mut Ranges<A>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    budget.check_memory(&ranges.memory)?;
    let len = ranges.value.len();
    if len < 2 {
        return Ok(());
    }
    for root in (0..len / 2).rev() {
        budget.additional(1)?;
        sift_signal_ranges(&mut ranges.value, root, len, budget)?;
    }
    for end in (1..len).rev() {
        budget.additional(1)?;
        ranges.value.swap(0, end);
        sift_signal_ranges(&mut ranges.value, 0, end, budget)?;
    }
    budget.check_memory(&ranges.memory)
}

pub(super) fn normalize_signal_ranges<A: AudioSoundContextAdmission>(
    mut ranges: Ranges<A>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<Ranges<A>, A> {
    sort_signal_ranges(&mut ranges, budget)?;
    let mut input = Owned {
        value: ranges.value.into_iter(),
        memory: ranges.memory,
    };
    let mut merged = empty_ranges(budget);
    while !input.value.as_slice().is_empty() {
        budget.additional(1)?;
        let range = input.value.next().expect("observed policy range");
        if range.start >= range.end {
            continue;
        }
        if let Some(last) = merged.value.last_mut() {
            budget.additional(1)?;
            if range.start <= last.end {
                budget.additional(1)?;
                last.end = last.end.max(range.end);
                continue;
            }
        }
        push_range(&mut merged, range, budget)?;
    }
    drop(input);
    Ok(merged)
}

pub(super) fn ordinary_tape<A: AudioSoundContextAdmission>(
    query: AudioPolicyQuery<SignalSample>,
    admission: &A,
) -> QueryResult<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A> {
    ordinary_signal(query, admission).map(|mut value| {
        value.origin = PolicyOrigin::Tape;
        value
    })
}

pub(super) fn finish_tape<A: AudioSoundContextAdmission>(
    state: State<A>,
    ranges: Ranges<A>,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A> {
    finish_signal_origin(state, ranges, limits, budget, PolicyOrigin::Tape)
}

pub(super) fn ordinary_standalone_signal<A: AudioSoundContextAdmission>(
    query: AudioPolicyQuery<SignalSample>,
    admission: &A,
) -> QueryResult<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A> {
    if A::METERED {
        return Err(PlanError::InvalidPlan("Signal policy accounting mode mismatch").into());
    }
    ordinary_signal(query, admission).map(|mut value| {
        value.origin = PolicyOrigin::Signal;
        value
    })
}

pub(super) fn finish_standalone_signal<A: AudioSoundContextAdmission>(
    state: State<A>,
    ranges: Ranges<A>,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A> {
    finish_signal_origin(state, ranges, limits, budget, PolicyOrigin::Signal)
}

fn finish_signal<A: AudioSoundContextAdmission>(
    state: State<A>,
    ranges: Ranges<A>,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A> {
    finish_signal_origin(state, ranges, limits, budget, PolicyOrigin::Point)
}

fn finish_signal_origin<A: AudioSoundContextAdmission>(
    mut state: State<A>,
    ranges: Ranges<A>,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
    origin: PolicyOrigin,
) -> QueryResult<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A> {
    let merged = normalize_signal_ranges(ranges, budget)?;
    let count = merged.value.len();
    let mut output = Owned {
        value: Vec::new(),
        memory: budget.memory(),
    };
    // This fresh Memory reserves exactly one final typed Vec and is never
    // adopted, zipped or merged with any inventory/work-buffer memory.
    budget.reserve(&mut output.memory, array::<Range<SignalSample>>(count)?)?;
    output.value = Vec::with_capacity(count);
    assert_eq!(output.value.capacity(), count);
    let mut input = Owned {
        value: merged.value.into_iter(),
        memory: merged.memory,
    };
    while !input.value.as_slice().is_empty() {
        budget.additional(1)?;
        let range = input.value.next().expect("observed merged policy range");
        output
            .value
            .push(SignalSample(range.start)..SignalSample(range.end));
    }
    drop(input);
    budget.check_memory(&output.memory)?;
    let canonical = budget.canonical_used;
    let additional = budget.additional_used;
    let work = canonical
        .checked_add(additional)
        .ok_or(TimeError::Overflow)?;
    if limits.maximum_work.checked_sub(budget.remaining) != Some(work) {
        return Err(PlanError::InvalidPlan(origin.work()).into());
    }
    let lookup = budget.lookup;
    let mask = FinalSignalMask {
        ranges: output.value,
        memory: output.memory,
    };
    let inventory = state
        .inventory
        .take()
        .expect("completed Point policy inventory")
        .map(budget, |inventory| SignalPolicyInventory {
            contents: inventory.spans,
            _contents_capacity: inventory.capacity,
        })?;
    Ok(AdmittedSignalPolicyQuery {
        origin,
        mask,
        inventory,
        lookup,
        work,
        canonical,
        additional,
        forwarded: A::METERED,
    })
}

#[cfg(test)]
#[path = "audio_point_policy_query_tests.rs"]
mod tests;
