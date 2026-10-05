//! Request-local intrinsic preparations. Authored descriptors alone cannot key
//! these results: independent projections can select different live inputs.

use super::*;

impl StageAudio {
    pub(super) fn prepare_projected(
        &mut self,
        projection: &AudioStageProjection<'_>,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Arc<SignalBlock>, StageAudioError> {
        control.check()?;
        let identity = projection.identity();
        let cached = control
            .work
            .borrow()
            .projected
            .iter()
            .find(|(key, _)| *key == identity)
            .map(|(_, block)| Arc::clone(block));
        if let Some(block) = cached {
            self.check_projected_depth(depth, block.relative_depth)?;
            for (asset, fingerprint) in &block.dependencies {
                control.check()?;
                let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
                if control.observe(asset, source)? != *fingerprint {
                    return Err(
                        PlanError::InvalidPlan("projected source changed within read").into(),
                    );
                }
            }
            return Ok(block);
        }
        self.preflight_projected(projection, control, depth)?;
        if control.work.borrow().projected_active.contains(&identity) {
            return Err(PlanError::InvalidPlan("recursive projected stage").into());
        }
        let input = SignalInput::try_from(projection)?;
        let output = projection.output_policy();
        let input_frames = u32::try_from(projection.input_sample_count()?.0)
            .map_err(|_| StageAudioError::Limit("input frames"))?;
        let output_frames = u32::try_from(output.sample_count()?.0)
            .map_err(|_| StageAudioError::Limit("output frames"))?;
        let rate = projection.stage().descriptor().rate;
        let recipe = CanonicalRecipe::with_rate(
            input_frames,
            output_frames,
            StretchRate::new(
                u64::try_from(rate.numerator()).map_err(|_| TimeError::Overflow)?,
                u64::try_from(rate.denominator()).map_err(|_| TimeError::Overflow)?,
            )?,
            i32::from(projection.stage().descriptor().pitch.semitones()),
        )?;
        let reservation = self.reserve(input_frames, output_frames, control)?;
        control
            .work
            .borrow_mut()
            .projected_active
            .push(identity.clone());
        let result = self.build_stage(
            input,
            SignalInput::PreserveTape(output),
            recipe,
            provider,
            control,
            depth,
        );
        control.work.borrow_mut().projected_active.pop();
        self.active_frames -= reservation;
        let block = result?;
        let frames = block.samples.len() as u64;
        self.make_room(frames, false, control)?;
        let block = Arc::new(block);
        let mut work = control.work.borrow_mut();
        work.projected_resident_frames += frames;
        work.projected.push((identity, Arc::clone(&block)));
        Ok(block)
    }

    fn check_projected_depth(&self, depth: usize, relative: usize) -> Result<(), StageAudioError> {
        if depth
            .checked_add(relative)
            .is_none_or(|maximum| maximum > self.limits.maximum_depth)
        {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        Ok(())
    }

    /// Input and output policy are independent. Inspect the complete input,
    /// including projected descendants, before the first physical source read.
    pub(super) fn preflight_projected(
        &self,
        projection: &AudioStageProjection<'_>,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        control.check()?;
        self.check_projected_depth(depth, 0)?;
        let identity = projection.identity();
        if let Some((_, relative)) = control
            .work
            .borrow()
            .projected_preflight
            .iter()
            .find(|(key, _)| *key == identity)
        {
            self.check_projected_depth(depth, *relative)?;
            return Ok(*relative);
        }
        let input = SignalInput::try_from(projection)?;
        let input_end = projection.input_sample_count()?;
        let output_end = projection.output_policy().sample_count()?;
        let relative_depth = self.preflight_intrinsic(
            input,
            input_end,
            SignalInput::PreserveTape(projection.output_policy()),
            output_end,
            control,
            depth,
        )?;
        let mut work = control.work.borrow_mut();
        if work.projected_preflight.len() + work.intrinsic_preflight.len()
            >= self.limits.maximum_prepared_stages as usize
        {
            return Err(StageAudioError::Limit("preflight stages per read"));
        }
        work.projected_preflight.push((identity, relative_depth));
        Ok(relative_depth)
    }

    fn preflight_current_stage(
        &self,
        stage: &AudioStage<'_>,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        control.check()?;
        self.check_projected_depth(depth, 0)?;
        if let Some((_, _, relative)) = control
            .work
            .borrow()
            .intrinsic_preflight
            .iter()
            .find(|(plan, key, _)| Arc::ptr_eq(plan, &self.plan) && key == stage.descriptor())
        {
            self.check_projected_depth(depth, *relative)?;
            return Ok(*relative);
        }
        let input = stage.input_signal();
        let output = stage.output_signal();
        let relative = self.preflight_intrinsic(
            (&input).into(),
            input.sample_count()?,
            (&output).into(),
            output.sample_count()?,
            control,
            depth,
        )?;
        let mut work = control.work.borrow_mut();
        if work.projected_preflight.len() + work.intrinsic_preflight.len()
            >= self.limits.maximum_prepared_stages as usize
        {
            return Err(StageAudioError::Limit("preflight stages per read"));
        }
        work.intrinsic_preflight.push((
            Arc::clone(&self.plan),
            stage.descriptor().clone(),
            relative,
        ));
        Ok(relative)
    }

    fn preflight_intrinsic(
        &self,
        input: SignalInput<'_, '_>,
        input_end: SignalSample,
        output: SignalInput<'_, '_>,
        output_end: SignalSample,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        if input_end.0 <= 0 || input_end.0 > i64::from(self.limits.maximum_input_frames) {
            return Err(StageAudioError::Limit("input frames"));
        }
        if output_end.0 <= 0 || output_end.0 > i64::from(self.limits.maximum_output_frames) {
            return Err(StageAudioError::Limit("output frames"));
        }
        let mut cursor = SignalSample(0);
        let mut relative_depth = 0;
        while cursor < input_end {
            let end = SignalSample((cursor.0 + i64::from(MAX_OUTPUT_FRAMES)).min(input_end.0));
            if let SignalInput::Mix(mix) = input {
                let query = mix.query(cursor..end, control.query_limits()?)?;
                control.spend_plan_work(query.work)?;
                relative_depth =
                    relative_depth.max(self.preflight_mix_query(&query, control, depth)?);
                cursor = end;
                continue;
            }
            let policy = input.policy(cursor..end, control.query_limits()?)?;
            control.spend_plan_work(policy.work)?;
            for content in &policy.contents {
                control.preflight_content(content, &self.plan)?;
            }
            let query = input.query(cursor..end, control.query_limits()?)?;
            control.spend_plan_work(query.work)?;
            relative_depth =
                relative_depth.max(self.preflight_signal_query(&query, control, depth)?);
            cursor = end;
        }
        cursor = SignalSample(0);
        while cursor < output_end {
            let end = SignalSample((cursor.0 + i64::from(MAX_OUTPUT_FRAMES)).min(output_end.0));
            let policy = output.policy(cursor..end, control.query_limits()?)?;
            control.spend_plan_work(policy.work)?;
            for content in &policy.contents {
                control.preflight_content(content, &self.plan)?;
            }
            cursor = end;
        }
        Ok(relative_depth)
    }

    pub(super) fn preflight_signal_query(
        &self,
        query: &AudioSignalQuery<'_>,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        self.check_projected_depth(depth, 0)?;
        let mut relative_depth = 0;
        for span in &query.spans {
            control.check()?;
            match &span.content {
                AudioSignalContent::Leaf(content) => {
                    control.preflight_content(content, &self.plan)?
                }
                AudioSignalContent::ProjectedStage(child) => {
                    relative_depth = relative_depth
                        .max(1 + self.preflight_projected(child, control, depth + 1)?);
                }
                AudioSignalContent::Stage(child) => {
                    relative_depth = relative_depth
                        .max(1 + self.preflight_current_stage(child, control, depth + 1)?);
                }
                AudioSignalContent::Bound(bound) => {
                    self.check_projected_depth(depth, 1)?;
                    relative_depth = relative_depth.max(1);
                    if let Some(child) = bound.intrinsic_stage()? {
                        relative_depth = relative_depth
                            .max(2 + self.preflight_current_stage(&child, control, depth + 2)?);
                    }
                }
            }
        }
        Ok(relative_depth)
    }
}
