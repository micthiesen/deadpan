//! Exact request-local Original PCM before authored owner treatments. These
//! blocks include time/pitch, silence policy and creative edges. Payloads live
//! in the controller's evictable cache; only bounded identity handles live here.

use super::*;
use std::hash::{Hash, Hasher};

const CAPACITY: usize = 64;
// Five fixed key fields and the bucket reduction. Admission precedes hashing.
const HASH_WORK: usize = 6;
// Bound the fixed bucket/LRU unlink or insertion, free-list and count updates.
const LINK_WORK: usize = 16;

pub(in crate::stages) struct Cached {
    block: Arc<ReadBlock>,
    pub(in crate::stages) resident: u64,
}

pub(super) enum Read {
    Cached(Arc<ReadBlock>),
    Fresh(ReadBlock),
}

impl std::ops::Deref for Read {
    type Target = ReadBlock;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Cached(block) => block,
            Self::Fresh(block) => block,
        }
    }
}

struct Entry<'plan> {
    // Retain the identity allocation so a dropped Arc address cannot alias a
    // later domain. Root identity is scoped by Graph's immutable plan.
    context: Context<'plan>,
    samples: Range<AudioSample>,
    depth: usize,
    key: u64,
    lookup: LookupKey,
    hash_previous: Option<usize>,
    hash_next: Option<usize>,
    previous: Option<usize>,
    next: Option<usize>,
}

// The Arc address is only an exact-key accelerator. Entries retain the Arc,
// and every hit still checks the full Context and range below.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct LookupKey {
    kind: u8,
    context: usize,
    start: i64,
    end: i64,
    depth: usize,
}

impl LookupKey {
    fn bucket(self) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        (hasher.finish() % CAPACITY as u64) as usize
    }

    fn new(context: &Context<'_>, samples: &Range<AudioSample>, depth: usize) -> Self {
        let (kind, identity) = match context {
            Context::Root => (0, 0),
            Context::Domain(value) => (1, Arc::as_ptr(value) as usize),
            Context::Retained(value) => (2, value.as_ptr() as usize),
        };
        Self {
            kind,
            context: identity,
            start: samples.start.0,
            end: samples.end.0,
            depth,
        }
    }
}

pub(super) struct Cache<'plan> {
    scope: Option<Arc<()>>,
    entries: [Option<Entry<'plan>>; CAPACITY],
    // Fixed bucket heads point at stable slots. Intrusive hash links let a
    // known-slot eviction unlink directly without scanning or shifting keys.
    // They are independent of the LRU links, which retain their old order.
    index: [Option<usize>; CAPACITY],
    free_slots: [usize; CAPACITY],
    free_len: usize,
    len: usize,
    most_recent: Option<usize>,
    least_recent: Option<usize>,
}

impl Default for Cache<'_> {
    fn default() -> Self {
        Self {
            scope: None,
            entries: std::array::from_fn(|_| None),
            index: [None; CAPACITY],
            free_slots: std::array::from_fn(|index| CAPACITY - 1 - index),
            free_len: CAPACITY,
            len: 0,
            most_recent: None,
            least_recent: None,
        }
    }
}

impl<'plan> Cache<'plan> {
    pub(super) fn resident_frames() -> Result<u64, StageAudioError> {
        // Pinned Rust 1.97.1 Arc<()> has two heap counters. Conservatively
        // charge that shell per Graph; Self already covers the Arc handles.
        let scope_shell = std::mem::size_of::<usize>()
            .checked_mul(2)
            .ok_or(StageAudioError::Range)?;
        let bytes = std::mem::size_of::<Self>()
            .checked_add(scope_shell)
            .ok_or(StageAudioError::Range)?;
        Ok(u64::try_from(bytes)
            .map_err(|_| StageAudioError::Range)?
            .div_ceil(8))
    }

    pub(super) fn clear(&mut self, controller: &mut StageAudio) {
        for slot in 0..CAPACITY {
            if let Some(entry) = self.entries[slot].take() {
                controller.remove_original_input(&entry.key);
            }
            self.index[slot] = None;
        }
        self.len = 0;
        self.free_slots = std::array::from_fn(|index| CAPACITY - 1 - index);
        self.free_len = CAPACITY;
        self.most_recent = None;
        self.least_recent = None;
    }

    fn find(
        &self,
        key: LookupKey,
        control: WorkControl<'_>,
    ) -> Result<Option<usize>, StageAudioError> {
        control.spend_plan_work(HASH_WORK)?;
        let mut candidate = self.index[key.bucket()];
        for _ in 0..CAPACITY {
            let Some(slot) = candidate else {
                return Ok(None);
            };
            control.spend_plan_work(1)?;
            let entry = self.entries[slot].as_ref().expect("occupied index");
            if entry.lookup == key {
                return Ok(Some(slot));
            }
            candidate = entry.hash_next;
        }
        if candidate.is_some() {
            return Err(PlanError::InvalidPlan("Original input index cycle").into());
        }
        Ok(None)
    }

    fn touch(&mut self, slot: usize) {
        if self.most_recent == Some(slot) {
            return;
        }
        let (previous, next) = {
            let entry = self.entries[slot].as_ref().expect("occupied cache slot");
            (entry.previous, entry.next)
        };
        if let Some(previous) = previous {
            self.entries[previous].as_mut().unwrap().next = next;
        }
        if let Some(next) = next {
            self.entries[next].as_mut().unwrap().previous = previous;
        }
        if self.least_recent == Some(slot) {
            self.least_recent = previous;
        }
        let old_head = self.most_recent;
        {
            let entry = self.entries[slot].as_mut().expect("occupied cache slot");
            entry.previous = None;
            entry.next = old_head;
        }
        if let Some(old_head) = old_head {
            self.entries[old_head].as_mut().unwrap().previous = Some(slot);
        }
        self.most_recent = Some(slot);
        if self.least_recent.is_none() {
            self.least_recent = Some(slot);
        }
    }

    fn remove(
        &mut self,
        slot: usize,
        control: WorkControl<'_>,
    ) -> Result<Entry<'plan>, StageAudioError> {
        // Charge every fixed hash/link operation before mutating either index.
        control.spend_plan_work(HASH_WORK + LINK_WORK)?;
        let bucket = self.entries[slot]
            .as_ref()
            .expect("occupied cache slot")
            .lookup
            .bucket();
        let entry = self.entries[slot].take().expect("occupied cache slot");
        if let Some(previous) = entry.hash_previous {
            self.entries[previous].as_mut().unwrap().hash_next = entry.hash_next;
        } else {
            self.index[bucket] = entry.hash_next;
        }
        if let Some(next) = entry.hash_next {
            self.entries[next].as_mut().unwrap().hash_previous = entry.hash_previous;
        }
        let (previous, next) = (entry.previous, entry.next);
        if let Some(previous) = previous {
            self.entries[previous].as_mut().unwrap().next = next;
        }
        if let Some(next) = next {
            self.entries[next].as_mut().unwrap().previous = previous;
        }
        if self.most_recent == Some(slot) {
            self.most_recent = next;
        }
        if self.least_recent == Some(slot) {
            self.least_recent = previous;
        }
        self.len -= 1;
        self.free_slots[self.free_len] = slot;
        self.free_len += 1;
        Ok(entry)
    }

    fn insert(
        &mut self,
        mut entry: Entry<'plan>,
        control: WorkControl<'_>,
    ) -> Result<usize, StageAudioError> {
        // One fixed insertion replaces binary search and a sorted-array shift.
        // No allocation or collision walk is needed when linking a fresh slot.
        control.spend_plan_work(HASH_WORK + LINK_WORK)?;
        let bucket = entry.lookup.bucket();
        let slot = self.free_slots[self.free_len - 1];
        let head = self.index[bucket];
        entry.hash_previous = None;
        entry.hash_next = head;
        if let Some(head) = head {
            self.entries[head].as_mut().unwrap().hash_previous = Some(slot);
        }
        self.index[bucket] = Some(slot);
        self.entries[slot] = Some(entry);
        self.free_len -= 1;
        self.len += 1;
        self.touch(slot);
        Ok(slot)
    }
}

fn same_context(left: &Context<'_>, right: &Context<'_>) -> bool {
    match (left, right) {
        (Context::Root, Context::Root) => true,
        (Context::Domain(left), Context::Domain(right)) => Arc::ptr_eq(left, right),
        (Context::Retained(left), Context::Retained(right)) => left.ptr_eq(right),
        _ => false,
    }
}

fn resident(block: &ReadBlock) -> Option<u64> {
    let bytes = block
        .samples
        .capacity()
        .checked_mul(std::mem::size_of::<[f32; 2]>())?
        .checked_add(
            block
                .suppressed
                .capacity()
                .checked_add(block.exhausted.capacity())?
                .checked_mul(std::mem::size_of::<Range<AudioSample>>())?,
        )?
        // Cover sparse BTreeMap nodes as well as live pairs. Asset identity
        // text is shared with the immutable plan.
        .checked_add(block.dependencies.len().checked_mul(1024)?)?
        .checked_add(std::mem::size_of::<ReadBlock>())?
        .checked_add(2 * std::mem::size_of::<usize>())?
        // Conservative controller BTreeMap node/entry overhead.
        .checked_add(1024)?;
    Some(u64::try_from(bytes).ok()?.div_ceil(8))
}

impl StageAudio {
    fn remove_original_input(&mut self, key: &u64) {
        self.original_inputs.remove(key);
        // BTreeMap can retain an empty root after its last removal. Release
        // that allocation before cached residency reports no payloads.
        if self.original_inputs.is_empty() {
            self.original_inputs = BTreeMap::new();
        }
    }

    pub(in crate::stages) fn evict_original_input(
        &mut self,
        control: WorkControl<'_>,
    ) -> Result<bool, StageAudioError> {
        for (&key, cached) in &self.original_inputs {
            control.spend_plan_work(1)?;
            if Arc::strong_count(&cached.block) == 1 {
                self.remove_original_input(&key);
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn shared_original_input<'plan, 'work>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, 'work>,
        context: &Context<'plan>,
        samples: Range<AudioSample>,
        control: WorkControl<'work>,
        depth: usize,
    ) -> Result<Read, StageAudioError> {
        self.shared_original_input_mode(provider, graph, context, (samples, None), control, depth)
    }

    pub(super) fn shared_original_input_prepared<'context, 'plan, 'work>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, 'work>,
        context: &Context<'plan>,
        (samples, prepared): (
            Range<AudioSample>,
            original_request::PreparedOriginalRequest<'context, 'plan>,
        ),
        control: WorkControl<'work>,
        depth: usize,
    ) -> Result<Read, StageAudioError> {
        self.shared_original_input_mode(
            provider,
            graph,
            context,
            (samples, Some(prepared)),
            control,
            depth,
        )
    }

    fn shared_original_input_mode<'context, 'plan, 'work>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, 'work>,
        context: &Context<'plan>,
        (samples, prepared): (
            Range<AudioSample>,
            Option<original_request::PreparedOriginalRequest<'context, 'plan>>,
        ),
        control: WorkControl<'work>,
        depth: usize,
    ) -> Result<Read, StageAudioError> {
        control.check()?;
        if !std::ptr::eq(graph.plan, self.plan.as_ref())
            || graph
                .original_inputs
                .scope
                .as_ref()
                .is_some_and(|scope| !Arc::ptr_eq(scope, &self.original_input_scope))
            || match context {
                Context::Root => false,
                Context::Domain(domain) => !domain.belongs_to(&self.plan),
                Context::Retained(input) => !input.belongs_to(&self.plan),
            }
        {
            return Err(StageAudioError::ForeignDomain);
        }
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        let frames = count(&samples)?;
        if frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        if let Some(request) = &prepared {
            policy_prediction::validate_original(
                request,
                context,
                graph.plan,
                samples.start,
                frames,
                control,
            )?;
        }
        let lookup = LookupKey::new(context, &samples, depth);
        if let Some(slot) = graph.original_inputs.find(lookup, control)? {
            let entry = graph.original_inputs.entries[slot]
                .as_ref()
                .expect("occupied cache slot");
            if entry.depth != depth
                || entry.samples != samples
                || !same_context(&entry.context, context)
            {
                return Err(StageAudioError::ForeignDomain);
            }
            let key = entry.key;
            graph.original_inputs.touch(slot);
            if let Some(cached) = self.original_inputs.get(&key) {
                let block = Arc::clone(&cached.block);
                if depth
                    .checked_add(block.relative_depth)
                    .is_none_or(|depth| depth > self.limits.maximum_depth)
                {
                    return Err(StageAudioError::Limit("nested stage depth"));
                }
                for (asset, fingerprint) in &block.dependencies {
                    control.spend_plan_work(1)?;
                    let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
                    if control.observe(asset, source)? != *fingerprint {
                        return Err(PreparationError::IndexMismatch.into());
                    }
                }
                control.check()?;
                return Ok(Read::Cached(block));
            }
            graph.original_inputs.remove(slot, control)?;
        }
        // Every context has one fixed private read mode. Retained applies its
        // creative fades with authored=false, so no owner treatment is cached.
        let block = if let Some(request) = prepared {
            self.read_prepared_original(provider, request, control, depth)?
        } else {
            match context {
                Context::Root => {
                    self.read_controlled(provider, samples.start, frames, control, true, depth)?
                }
                Context::Domain(domain) => self.read_context_controlled(
                    provider,
                    domain,
                    samples.start,
                    frames,
                    control,
                    (depth, true),
                )?,
                Context::Retained(input) => self.read_retained_controlled(
                    provider,
                    input,
                    samples.start,
                    frames,
                    control,
                    (depth, false),
                )?,
            }
        };
        control.check()?;
        let Some(resident) = resident(&block) else {
            return Ok(Read::Fresh(block));
        };
        let Some(next) = self.next_original_input.checked_add(1) else {
            return Ok(Read::Fresh(block));
        };
        let slots = self.limits.maximum_cached_stages;
        if graph.original_inputs.len == slots {
            // Entries can outlive evicted payloads. Remove a stale/unpinned
            // least-recent key, never one still needed by an active reader.
            let mut removable = None;
            let mut candidate = graph.original_inputs.least_recent;
            while let Some(index) = candidate {
                control.spend_plan_work(1)?;
                let entry = graph.original_inputs.entries[index]
                    .as_ref()
                    .expect("occupied cache slot");
                if self
                    .original_inputs
                    .get(&entry.key)
                    .is_none_or(|cached| Arc::strong_count(&cached.block) == 1)
                {
                    removable = Some(index);
                    break;
                }
                candidate = entry.previous;
            }
            let Some(index) = removable else {
                return Ok(Read::Fresh(block));
            };
            let key = graph.original_inputs.remove(index, control)?.key;
            self.remove_original_input(&key);
        }
        loop {
            let fits = self
                .cached_resident_frames()
                .checked_add(self.active_frames)
                .and_then(|frames| {
                    frames.checked_add(control.work.borrow().projected_resident_frames)
                })
                .and_then(|frames| frames.checked_add(control.work.borrow().sound_context_frames))
                .and_then(|frames| frames.checked_add(resident))
                .is_some_and(|frames| frames <= u64::from(self.limits.maximum_resident_frames));
            if fits && self.cache.len() + self.original_inputs.len() < slots {
                break;
            }
            // Optional memoization must not evict unrelated prepared stages.
            if !self.evict_original_input(control)? {
                return Ok(Read::Fresh(block));
            }
        }
        let block = Arc::new(block);
        let key = self.next_original_input;
        self.next_original_input = next;
        graph
            .original_inputs
            .scope
            .get_or_insert_with(|| Arc::clone(&self.original_input_scope));
        graph.original_inputs.insert(
            Entry {
                context: context.clone(),
                samples,
                depth,
                key,
                lookup,
                hash_previous: None,
                hash_next: None,
                previous: None,
                next: None,
            },
            control,
        )?;
        self.original_inputs.insert(
            key,
            Cached {
                block: Arc::clone(&block),
                resident,
            },
        );
        Ok(Read::Cached(block))
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "original_inputs_tests.rs"]
mod tests;
