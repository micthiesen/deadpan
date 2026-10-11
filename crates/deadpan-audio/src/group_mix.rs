//! Sum voices at each nonlinear owner before running its treatments. Linear
//! stretches retain the exact combined dB calculation of the single-voice path.
//! All routing and PCM residency is admitted before source media is opened.

use deadpan_plan::{AudioOwnerClock, AudioOwnerSupport};

use super::*;

pub(super) struct Route<'plan> {
    target: Option<usize>,
    /// Outer to inner, below the nearest nonlinear owner.
    owners: Vec<AudioOwnerClock<'plan>>,
}

struct Run<'plan> {
    samples: Range<usize>,
    /// The bus owner through the ancestors below its next nonlinear parent.
    owners: Vec<AudioOwnerClock<'plan>>,
}

struct Bus<'plan> {
    parent: Option<usize>,
    depth: usize,
    runs: Vec<Run<'plan>>,
    samples: Vec<[f64; 2]>,
}

pub(super) struct GroupMix<'plan> {
    start: AudioSample,
    frames: usize,
    authored: bool,
    original: Vec<(Range<usize>, AudioOwnerSupport, Option<Route<'plan>>)>,
    root: Route<'plan>,
    by_owner: BTreeMap<InstancePath, usize>,
    // Parents are inserted before children. Reduction additionally sorts by
    // stable occurrence identity so sibling sum order never depends on a read.
    buses: Vec<Bus<'plan>>,
    sum: Vec<[f64; 2]>,
}

impl<'plan> GroupMix<'plan> {
    pub(super) fn plan(
        plan: &'plan RenderPlan,
        start: AudioSample,
        frames: u32,
        authored: bool,
        control: WorkControl<'_>,
    ) -> Result<Self, StageAudioError> {
        let mut mix = Self {
            start,
            frames: frames as usize,
            authored,
            original: Vec::new(),
            root: Route {
                target: None,
                owners: Vec::new(),
            },
            by_owner: BTreeMap::new(),
            buses: Vec::new(),
            sum: Vec::new(),
        };
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(StageAudioError::Range)?,
        );
        if !authored || !plan.has_audio_treatments() {
            mix.original.push((
                0..frames as usize,
                AudioOwnerSupport::Active,
                Some(Route {
                    target: None,
                    owners: Vec::new(),
                }),
            ));
            return Ok(mix);
        }
        let query = plan.audio_gain_owners(start..end, control.query_limits()?)?;
        control.spend_plan_work(query.work())?;
        let root = query
            .spans()
            .first()
            .and_then(|span| span.owners().first())
            .ok_or(PlanError::InvalidPlan("missing root mix clock"))?;
        if root.instance().node != plan.metadata().root {
            return Err(PlanError::InvalidPlan("invalid root mix clock").into());
        }
        mix.root = mix.route(std::slice::from_ref(root), start..end, control)?;
        let mut cursor = start;
        for span in query.spans() {
            let range = span.samples();
            if range.start != cursor || range.end <= range.start || range.end > end {
                return Err(PlanError::InvalidPlan("incomplete mix owner clocks").into());
            }
            // A picture-only recipe has no Original contribution to this
            // bus. Its retained clock must not constrain an independent sound.
            // Stateful processing is deliberately excluded from this proof.
            let route = if span.original_is_silent() {
                None
            } else {
                Some(mix.route(span.owners(), range.clone(), control)?)
            };
            let local = mix.local(range.clone())?;
            mix.original.push((local, span.support(), route));
            cursor = range.end;
        }
        if cursor != end {
            return Err(PlanError::InvalidPlan("incomplete mix owner clocks").into());
        }
        Ok(mix)
    }

    fn local(&self, range: Range<AudioSample>) -> Result<Range<usize>, StageAudioError> {
        let start =
            usize::try_from(range.start.0 - self.start.0).map_err(|_| StageAudioError::Range)?;
        let end =
            usize::try_from(range.end.0 - self.start.0).map_err(|_| StageAudioError::Range)?;
        if start >= end || end > self.frames {
            return Err(StageAudioError::Range);
        }
        Ok(start..end)
    }

    pub(super) fn route(
        &mut self,
        owners: &[AudioOwnerClock<'plan>],
        range: Range<AudioSample>,
        control: WorkControl<'_>,
    ) -> Result<Route<'plan>, StageAudioError> {
        if !self.authored {
            return Ok(Route {
                target: None,
                owners: Vec::new(),
            });
        }
        deadpan_core::validate_audio_treatment_layers(
            owners.iter().filter_map(|owner| owner.treatments()),
        )?;
        let samples = self.local(range)?;
        let mut parent = None;
        let mut below = 0;
        for (index, owner) in owners.iter().enumerate() {
            control.spend_plan_work(1)?;
            if owner
                .treatments()
                .is_none_or(|recipe| recipe.saturation().is_none())
            {
                continue;
            }
            let chain = &owners[below..=index];
            let bus = if let Some(&bus) = self.by_owner.get(owner.instance()) {
                if self.buses[bus].parent != parent {
                    return Err(
                        PlanError::InvalidPlan("inconsistent nonlinear mix ancestry").into(),
                    );
                }
                bus
            } else {
                let bus = self.buses.len();
                self.by_owner.insert(owner.instance().clone(), bus);
                self.buses.push(Bus {
                    parent,
                    depth: parent.map_or(0, |parent| self.buses[parent].depth + 1),
                    runs: Vec::new(),
                    samples: Vec::new(),
                });
                bus
            };
            self.register_run(bus, samples.clone(), chain, control)?;
            parent = Some(bus);
            below = index + 1;
        }
        Ok(Route {
            target: parent,
            owners: owners[below..].to_vec(),
        })
    }

    fn register_run(
        &mut self,
        bus: usize,
        range: Range<usize>,
        owners: &[AudioOwnerClock<'plan>],
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let mut cursor = range.start;
        let mut uncovered = Vec::new();
        for run in &self.buses[bus].runs {
            control.spend_plan_work(1)?;
            let overlap = range.start.max(run.samples.start)..range.end.min(run.samples.end);
            if overlap.start >= overlap.end {
                continue;
            }
            // Equal affine clocks prove the complete overlap directly. A
            // retained Original can legitimately have a different clock from
            // a current independent sound after an edit, even for one owner.
            // For memoryless treatments, equal transfer programs suffice.
            if run.owners.len() != owners.len() {
                return Err(PlanError::InvalidPlan("inconsistent nonlinear mix owners").into());
            }
            let mut same_clocks = true;
            for (a, b) in run.owners.iter().zip(owners) {
                control.spend_plan_work(1)?;
                let first = AudioSample(self.start.0 + overlap.start as i64);
                let last = AudioSample(self.start.0 + (overlap.end - 1) as i64);
                if a.instance() != b.instance()
                    || a.kind() != b.kind()
                    || a.gap_after() != b.gap_after()
                {
                    return Err(PlanError::InvalidPlan("inconsistent nonlinear mix owners").into());
                }
                same_clocks &= a.escalation_millidecibels() == b.escalation_millidecibels()
                    && a.sampling().local_at(first)? == b.sampling().local_at(first)?
                    && a.sampling().local_at(last)? == b.sampling().local_at(last)?;
            }
            if !same_clocks {
                // Endpoint gain equality cannot prove the interior of a
                // curve or a mute range. Check every allocated sample, under
                // the same work/deadline budget and before media admission.
                for offset in overlap.clone() {
                    control.check()?;
                    let at = AudioSample(self.start.0 + offset as i64);
                    let mut left = authored_gain::GainSum::default();
                    let mut right = authored_gain::GainSum::default();
                    for owner in run.owners.iter().rev() {
                        left.owner(owner, at, control)?;
                    }
                    for owner in owners.iter().rev() {
                        right.owner(owner, at, control)?;
                    }
                    if !left.same_transfer(&right) {
                        return Err(StageAudioError::Unsupported(
                            "different automation on independently timed inputs to one nonlinear owner",
                        ));
                    }
                }
            }
            if cursor < overlap.start {
                uncovered.push(cursor..overlap.start);
            }
            cursor = cursor.max(overlap.end);
        }
        if cursor < range.end {
            uncovered.push(cursor..range.end);
        }
        self.buses[bus]
            .runs
            .extend(uncovered.into_iter().map(|samples| Run {
                samples,
                owners: owners.to_vec(),
            }));
        self.buses[bus].runs.sort_by_key(|run| run.samples.start);
        Ok(())
    }

    /// Extra f32 stereo frame equivalents beyond the caller's f64 final sum.
    pub(super) fn resident_frames(&self) -> Result<u64, StageAudioError> {
        u64::try_from(self.buses.len())
            .ok()
            .and_then(|buses| buses.checked_mul(self.frames as u64 * 2))
            .ok_or(StageAudioError::Limit("group mix PCM"))
    }

    pub(super) fn allocate(&mut self) {
        self.sum = vec![[0.0; 2]; self.frames];
        for bus in &mut self.buses {
            bus.samples = vec![[0.0; 2]; self.frames];
        }
    }

    pub(super) fn original(
        &mut self,
        samples: &[[f32; 2]],
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        if samples.len() != self.frames {
            return Err(PlanError::InvalidPlan("incomplete Original mix PCM").into());
        }
        for (range, support, route) in &self.original {
            for index in range.clone() {
                let sample = samples[index];
                let Some(route) = route else {
                    if sample.iter().any(|value| *value != 0.0) {
                        return Err(
                            PlanError::InvalidPlan("nonzero Original in declared silence").into(),
                        );
                    }
                    continue;
                };
                if *support == AudioOwnerSupport::Inactive
                    && sample.iter().any(|value| *value != 0.0)
                {
                    return Err(StageAudioError::Unsupported(
                        "nonzero Original outside meaningful owner-clock support",
                    ));
                }
                let gain = authored_gain::beat_sound_gain(
                    &route.owners,
                    AudioSample(self.start.0 + index as i64),
                    0,
                    control,
                    self.authored,
                )?;
                let value = gain.apply(sample)?;
                // The Original is the first contribution and has one route at
                // each sample. Assignment also retains signed zero exactly.
                match route.target {
                    Some(bus) => self.buses[bus].samples[index] = value,
                    None => self.sum[index] = value,
                }
            }
        }
        Ok(())
    }

    pub(super) fn add_root(
        &mut self,
        index: usize,
        sample: [f32; 2],
        event_gain: i32,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let gain = authored_gain::beat_sound_gain(
            &self.root.owners,
            AudioSample(self.start.0 + index as i64),
            event_gain,
            control,
            self.authored,
        )?;
        self.add_value(self.root.target, index, gain.apply(sample)?)
    }

    pub(super) fn add(
        &mut self,
        route: &Route<'_>,
        index: usize,
        sample: [f32; 2],
        event_gain: i32,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let gain = authored_gain::beat_sound_gain(
            &route.owners,
            AudioSample(self.start.0 + index as i64),
            event_gain,
            control,
            self.authored,
        )?;
        self.add_value(route.target, index, gain.apply(sample)?)
    }

    fn add_value(
        &mut self,
        target: Option<usize>,
        index: usize,
        value: [f64; 2],
    ) -> Result<(), StageAudioError> {
        let total = match target {
            Some(bus) => &mut self.buses[bus].samples[index],
            None => &mut self.sum[index],
        };
        for channel in 0..2 {
            total[channel] += value[channel];
            if !total[channel].is_finite() {
                return Err(PreparationError::InvalidSamples.into());
            }
        }
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        control: WorkControl<'_>,
    ) -> Result<Vec<[f64; 2]>, StageAudioError> {
        let mut order: Vec<_> = self.by_owner.values().copied().collect();
        // Stable sorting retains the BTreeMap's occurrence order among peers.
        order.sort_by_key(|&index| std::cmp::Reverse(self.buses[index].depth));
        for index in order {
            let samples = std::mem::take(&mut self.buses[index].samples);
            let runs = std::mem::take(&mut self.buses[index].runs);
            for run in runs {
                for offset in run.samples {
                    control.check()?;
                    let mut gain = authored_gain::GainSum::default();
                    for owner in run.owners.iter().rev() {
                        gain.owner(owner, AudioSample(self.start.0 + offset as i64), control)?;
                    }
                    self.add_value(
                        self.buses[index].parent,
                        offset,
                        gain.apply_wide(samples[offset])?,
                    )?;
                }
            }
        }
        Ok(self.sum)
    }
}
