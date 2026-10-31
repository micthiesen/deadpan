//! Native destinations retain their source owners until replacement publication.
use super::ops::{MaskContext, MaskWorkPhase, begin_ranges};
use super::*;

fn mutation_work<C: MaskContext>(context: &C, phase: MaskWorkPhase) -> Result<(), StageAudioError> {
    context.check()?;
    context.spend(phase, 1)
}

impl<S: MaskSample> SuppressionMask<S> {
    pub(crate) fn plain(ranges: Vec<Range<S>>) -> Self {
        Self {
            value: Value::Plain(ranges),
        }
    }

    pub(crate) fn is_owned(&self) -> bool {
        matches!(&self.value, Value::Owned(_))
    }

    pub(crate) fn unaccounted_capacity(&self) -> usize {
        match &self.value {
            Value::Plain(ranges) => ranges.capacity(),
            Value::Owned(_) => 0,
        }
    }

    fn owned_ranges_mut(&mut self) -> &mut Vec<Range<S>> {
        match &mut self.value {
            Value::Owned(owner) => {
                &mut owner
                    .storage
                    .as_mut()
                    .expect("live unique mask storage")
                    .ranges
            }
            Value::Plain(_) => unreachable!("admitted mutation requires owned storage"),
        }
    }

    pub(crate) fn into_admitted<C: MaskContext>(
        self,
        context: &mut C,
    ) -> Result<Self, StageAudioError> {
        if self.is_owned() {
            return Ok(self);
        }
        context.check()?;
        if self.is_empty() {
            return Ok(Self::default());
        }
        let mut destination = begin_ranges::<S, C>(context, self.len())?;
        for range in self.iter() {
            destination.push(range.start..range.end, context)?;
        }
        let result = destination.finish(context)?;
        context.check()?;
        drop(self);
        Ok(result)
    }

    pub(crate) fn map<T: MaskSample, C: MaskContext>(
        self,
        mut map: impl FnMut(Range<S>) -> Range<T>,
        context: &mut C,
    ) -> Result<SuppressionMask<T>, StageAudioError> {
        match self.value {
            Value::Plain(ranges) => Ok(SuppressionMask::plain(
                ranges.into_iter().map(map).collect(),
            )),
            Value::Owned(owner) => {
                let source = Self {
                    value: Value::Owned(owner),
                };
                let mut destination = begin_ranges::<T, C>(context, source.len())?;
                for range in source.iter() {
                    mutation_work(context, MaskWorkPhase::Map)?;
                    let mapped = map(range.start..range.end);
                    destination.push(mapped, context)?;
                }
                let result = destination.finish(context)?;
                context.check()?;
                drop(source);
                Ok(result)
            }
        }
    }

    pub(crate) fn try_map<T: MaskSample, E, C: MaskContext>(
        self,
        mut map: impl FnMut(Range<S>) -> Result<Range<T>, E>,
        context: &mut C,
    ) -> Result<SuppressionMask<T>, StageAudioError>
    where
        StageAudioError: From<E>,
    {
        match self.value {
            Value::Plain(ranges) => {
                let mapped = ranges.into_iter().map(map).collect::<Result<_, E>>()?;
                Ok(SuppressionMask::plain(mapped))
            }
            Value::Owned(owner) => {
                let source = Self {
                    value: Value::Owned(owner),
                };
                let mut destination = begin_ranges::<T, C>(context, source.len())?;
                for range in source.iter() {
                    mutation_work(context, MaskWorkPhase::Map)?;
                    let mapped = map(range.start..range.end)?;
                    destination.push(mapped, context)?;
                }
                let result = destination.finish(context)?;
                context.check()?;
                drop(source);
                Ok(result)
            }
        }
    }

    pub(super) fn push_in_capacity<C: MaskContext>(
        &mut self,
        range: Range<S>,
        context: &C,
    ) -> Result<(), StageAudioError> {
        context.check()?;
        if self.len() >= self.capacity() {
            return Err(StageAudioError::Limit("funded suppression range capacity"));
        }
        mutation_work(context, MaskWorkPhase::Map)?;
        self.owned_ranges_mut().push(range);
        Ok(())
    }

    pub(crate) fn extend<C: MaskContext>(
        &mut self,
        other: Self,
        context: &mut C,
    ) -> Result<(), StageAudioError> {
        if let (Value::Plain(target), Value::Plain(_)) = (&mut self.value, &other.value) {
            let Value::Plain(source) = other.value else {
                unreachable!()
            };
            target.extend(source);
            return Ok(());
        }
        context.check()?;
        let capacity = self
            .len()
            .checked_add(other.len())
            .ok_or(StageAudioError::Range)?;
        if self.is_owned() && capacity <= self.capacity() {
            for range in other.iter() {
                self.push_in_capacity(range.start..range.end, context)?;
            }
            context.check()?;
        } else {
            let mut destination = begin_ranges::<S, C>(context, capacity)?;
            for range in self.iter().chain(other.iter()) {
                destination.push(range.start..range.end, context)?;
            }
            let result = destination.finish(context)?;
            context.check()?;
            *self = result;
        }
        drop(other);
        Ok(())
    }

    pub(crate) fn append_exhausted<C: MaskContext>(
        &mut self,
        exhausted: &mut Vec<Range<S>>,
        context: &mut C,
    ) -> Result<(), StageAudioError> {
        if let Value::Plain(target) = &mut self.value {
            target.append(exhausted);
            return Ok(());
        }
        context.check()?;
        let capacity = self
            .len()
            .checked_add(exhausted.len())
            .ok_or(StageAudioError::Range)?;
        if capacity <= self.capacity() {
            for range in exhausted.iter() {
                self.push_in_capacity(range.start..range.end, context)?;
            }
            context.check()?;
        } else {
            let mut destination = begin_ranges::<S, C>(context, capacity)?;
            for range in self.iter().chain(exhausted.iter()) {
                destination.push(range.start..range.end, context)?;
            }
            let result = destination.finish(context)?;
            context.check()?;
            *self = result;
        }
        // clear retains the source allocation, as the original Vec::append did.
        exhausted.clear();
        Ok(())
    }

    pub(crate) fn extend_filter_map<T: MaskSample, C: MaskContext>(
        &mut self,
        source: SuppressionMask<T>,
        mut map: impl FnMut(Range<T>) -> Option<Range<S>>,
        context: &mut C,
    ) -> Result<(), StageAudioError> {
        if let (Value::Plain(target), Value::Plain(_)) = (&mut self.value, &source.value) {
            let Value::Plain(ranges) = source.value else {
                unreachable!()
            };
            target.extend(ranges.into_iter().filter_map(map));
            return Ok(());
        }
        context.check()?;
        let capacity = self
            .len()
            .checked_add(source.len())
            .ok_or(StageAudioError::Range)?;
        if self.is_owned() && capacity <= self.capacity() {
            for range in source.iter() {
                mutation_work(context, MaskWorkPhase::Map)?;
                if let Some(mapped) = map(range.start..range.end) {
                    self.push_in_capacity(mapped, context)?;
                }
            }
            context.check()?;
        } else {
            let mut destination = begin_ranges::<S, C>(context, capacity)?;
            for range in self.iter() {
                destination.push(range.start..range.end, context)?;
            }
            for range in source.iter() {
                mutation_work(context, MaskWorkPhase::Map)?;
                if let Some(mapped) = map(range.start..range.end) {
                    destination.push(mapped, context)?;
                }
            }
            let result = destination.finish(context)?;
            context.check()?;
            *self = result;
        }
        drop(source);
        Ok(())
    }

    pub(crate) fn extend_iter<C: MaskContext>(
        &mut self,
        source: impl IntoIterator<Item = Range<S>>,
        context: &mut C,
    ) -> Result<(), StageAudioError> {
        if let Value::Plain(target) = &mut self.value {
            target.extend(source);
            return Ok(());
        }
        context.check()?;
        let mut source = source.into_iter();
        if let Some(upper) = source.size_hint().1 {
            let capacity = self
                .len()
                .checked_add(upper)
                .ok_or(StageAudioError::Range)?;
            if capacity > self.capacity() {
                let mut destination = begin_ranges::<S, C>(context, capacity)?;
                for range in self.iter() {
                    destination.push(range.start..range.end, context)?;
                }
                loop {
                    mutation_work(context, MaskWorkPhase::Scan)?;
                    let Some(range) = source.next() else { break };
                    // A real upper bound prevents reallocating a pending builder.
                    destination.push(range, context)?;
                }
                let result = destination.finish(context)?;
                context.check()?;
                *self = result;
                return Ok(());
            }
        }
        loop {
            mutation_work(context, MaskWorkPhase::Scan)?;
            let Some(range) = source.next() else { break };
            if self.len() < self.capacity() {
                self.push_in_capacity(range, context)?;
            } else {
                let capacity = self
                    .capacity()
                    .checked_mul(2)
                    .map(|capacity| capacity.max(1))
                    .ok_or(StageAudioError::Range)?;
                let mut destination = begin_ranges::<S, C>(context, capacity)?;
                for existing in self.iter() {
                    destination.push(existing.start..existing.end, context)?;
                }
                destination.push(range, context)?;
                let result = destination.finish(context)?;
                context.check()?;
                *self = result;
            }
        }
        context.check()
    }
}

impl<S: MaskSample + Ord> SuppressionMask<S> {
    pub(crate) fn merge<C: MaskContext>(self, context: &C) -> Result<Self, StageAudioError> {
        match self.value {
            Value::Plain(mut ranges) => {
                ranges.sort_unstable_by_key(|range| range.start);
                let mut merged: Vec<Range<S>> = Vec::with_capacity(ranges.len());
                for range in ranges {
                    if let Some(last) = merged.last_mut().filter(|last| range.start <= last.end) {
                        last.end = last.end.max(range.end);
                    } else {
                        merged.push(range);
                    }
                }
                Ok(Self::plain(merged))
            }
            Value::Owned(owner) => {
                let mut result = Self {
                    value: Value::Owned(owner),
                };
                context.check()?;
                let ranges = result.owned_ranges_mut();
                let len = ranges.len();
                for root in (0..len / 2).rev() {
                    sift_down(ranges, root, len, context)?;
                }
                for end in (1..ranges.len()).rev() {
                    mutation_work(context, MaskWorkPhase::Map)?;
                    ranges.swap(0, end);
                    sift_down(ranges, 0, end, context)?;
                }
                let mut written = 0;
                for read in 0..ranges.len() {
                    mutation_work(context, MaskWorkPhase::Scan)?;
                    let joins = written > 0 && ranges[read].start <= ranges[written - 1].end;
                    if joins {
                        mutation_work(context, MaskWorkPhase::Scan)?;
                        let end = ranges[written - 1].end.max(ranges[read].end);
                        mutation_work(context, MaskWorkPhase::Map)?;
                        ranges[written - 1].end = end;
                    } else {
                        if written != read {
                            mutation_work(context, MaskWorkPhase::Map)?;
                            ranges[written] = ranges[read].start..ranges[read].end;
                        }
                        written += 1;
                    }
                }
                context.check()?;
                ranges.truncate(written);
                context.check()?;
                Ok(result)
            }
        }
    }
}

fn sift_down<S: MaskSample + Ord, C: MaskContext>(
    ranges: &mut [Range<S>],
    mut root: usize,
    end: usize,
    context: &C,
) -> Result<(), StageAudioError> {
    loop {
        context.check()?;
        let left = root
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(StageAudioError::Range)?;
        if left >= end {
            break;
        }
        let mut child = left;
        if left + 1 < end {
            mutation_work(context, MaskWorkPhase::Scan)?;
            if ranges[left + 1].start > ranges[left].start {
                child = left + 1;
            }
        }
        mutation_work(context, MaskWorkPhase::Scan)?;
        if ranges[child].start <= ranges[root].start {
            break;
        }
        mutation_work(context, MaskWorkPhase::Map)?;
        ranges.swap(root, child);
        root = child;
    }
    Ok(())
}
