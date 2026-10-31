//! Cached masks remain borrowed while each derived destination is admitted.
use super::ops::{MaskContext, MaskWorkPhase, begin_ranges};
use super::*;

impl<S: MaskSample> SuppressionMask<S> {
    pub(crate) fn push<C: MaskContext>(
        &mut self,
        range: Range<S>,
        context: &mut C,
    ) -> Result<(), StageAudioError> {
        if let Value::Plain(target) = &mut self.value {
            target.push(range);
            return Ok(());
        }
        if self.len() < self.capacity() {
            return self.push_in_capacity(range, context);
        }
        self.extend_iter(std::iter::once(range), context)
    }

    pub(crate) fn extend_try_map<T: MaskSample, E, C: MaskContext>(
        &mut self,
        source: SuppressionMask<T>,
        mut map: impl FnMut(Range<T>) -> Result<Range<S>, E>,
        context: &mut C,
    ) -> Result<(), StageAudioError>
    where
        StageAudioError: From<E>,
    {
        if let (Value::Plain(target), Value::Plain(_)) = (&mut self.value, &source.value) {
            let Value::Plain(ranges) = source.value else {
                unreachable!()
            };
            for range in ranges {
                target.push(map(range)?);
            }
            return Ok(());
        }
        context.check()?;
        let capacity = self
            .len()
            .checked_add(source.len())
            .ok_or(StageAudioError::Range)?;
        if self.is_owned() && capacity <= self.capacity() {
            for range in source.iter() {
                context.spend(MaskWorkPhase::Map, 1)?;
                let mapped = map(range.start..range.end)?;
                self.push_in_capacity(mapped, context)?;
            }
            return context.check();
        }
        let mut destination = begin_ranges::<S, C>(context, capacity)?;
        for range in self.iter() {
            destination.push(range.start..range.end, context)?;
        }
        for range in source.iter() {
            context.spend(MaskWorkPhase::Map, 1)?;
            let mapped = map(range.start..range.end)?;
            destination.push(mapped, context)?;
        }
        let result = destination.finish(context)?;
        context.check()?;
        *self = result;
        Ok(())
    }

    pub(crate) fn extend_borrowed_filter_map<T: MaskSample, C: MaskContext>(
        &mut self,
        source: &SuppressionMask<T>,
        mut map: impl FnMut(&Range<T>) -> Option<Range<S>>,
        context: &mut C,
    ) -> Result<(), StageAudioError> {
        if let (Value::Plain(target), Value::Plain(ranges)) = (&mut self.value, &source.value) {
            target.extend(ranges.iter().filter_map(map));
            return Ok(());
        }
        context.check()?;
        let capacity = self
            .len()
            .checked_add(source.len())
            .ok_or(StageAudioError::Range)?;
        if self.is_owned() && capacity <= self.capacity() {
            for range in source.iter() {
                context.spend(MaskWorkPhase::Map, 1)?;
                if let Some(mapped) = map(range) {
                    self.push_in_capacity(mapped, context)?;
                }
            }
            return context.check();
        }
        let mut destination = begin_ranges::<S, C>(context, capacity)?;
        for range in self.iter() {
            destination.push(range.start..range.end, context)?;
        }
        for range in source.iter() {
            context.spend(MaskWorkPhase::Map, 1)?;
            if let Some(mapped) = map(range) {
                destination.push(mapped, context)?;
            }
        }
        let result = destination.finish(context)?;
        context.check()?;
        *self = result;
        Ok(())
    }
}

impl<S: MaskSample + Ord> SuppressionMask<S> {
    pub(crate) fn append_clipped<C: MaskContext>(
        &mut self,
        source: &Self,
        requested: &Range<S>,
        context: &mut C,
    ) -> Result<(), StageAudioError> {
        if let (Value::Plain(output), Value::Plain(ranges)) = (&mut self.value, &source.value) {
            for range in ranges {
                let clipped = range.start.max(requested.start)..range.end.min(requested.end);
                if clipped.start >= clipped.end {
                    continue;
                }
                if let Some(last) = output.last_mut()
                    && last.end >= clipped.start
                {
                    last.end = last.end.max(clipped.end);
                } else {
                    output.push(clipped);
                }
            }
            return Ok(());
        }
        self.extend_borrowed_filter_map(
            source,
            |range| {
                let clipped = range.start.max(requested.start)..range.end.min(requested.end);
                (clipped.start < clipped.end).then_some(clipped)
            },
            context,
        )?;
        *self = std::mem::take(self).merge(context)?;
        Ok(())
    }

    pub(crate) fn intersect<C: MaskContext>(
        left: &Self,
        right: &Self,
        context: &mut C,
    ) -> Result<Self, StageAudioError> {
        if !left.is_owned() && !right.is_owned() {
            let mut result = Vec::new();
            let (mut a, mut b) = (0, 0);
            while a < left.len() && b < right.len() {
                let range = left[a].start.max(right[b].start)..left[a].end.min(right[b].end);
                if range.start < range.end {
                    result.push(range);
                }
                if left[a].end < right[b].end {
                    a += 1;
                } else {
                    b += 1;
                }
            }
            return Ok(Self::plain(result));
        }
        context.check()?;
        let capacity = left
            .len()
            .checked_add(right.len())
            .ok_or(StageAudioError::Range)?;
        let mut result = begin_ranges::<S, C>(context, capacity)?;
        let (mut a, mut b) = (0, 0);
        while a < left.len() && b < right.len() {
            context.spend(MaskWorkPhase::Scan, 1)?;
            let range = left[a].start.max(right[b].start)..left[a].end.min(right[b].end);
            if range.start < range.end {
                result.push(range, context)?;
            }
            context.spend(MaskWorkPhase::Scan, 1)?;
            if left[a].end < right[b].end {
                a += 1;
            } else {
                b += 1;
            }
        }
        result.finish(context)
    }
}
