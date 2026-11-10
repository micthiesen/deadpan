//! Request-local intrinsic preparations. Authored descriptors alone cannot key
//! these results: independent projections can select different live inputs.

#[cfg(test)]
use super::intrinsic_preflight_before_trace as before;
#[cfg(test)]
use super::intrinsic_preflight_policy_native_trace as native_trace;
use super::*;

#[path = "stage_projection/intrinsic_policy.rs"]
mod intrinsic_policy;
#[cfg(test)]
use intrinsic_policy::IntrinsicPolicy;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "stage_projection/tests.rs"]
mod tests;

#[derive(Clone)]
pub(super) enum ProjectionCacheKey {
    Opaque(AudioProjectionIdentity),
    Source(Arc<RenderPlan>, Arc<deadpan_plan::AudioSourceProjectionKey>),
    Owned(Arc<RenderPlan>, Arc<deadpan_plan::AudioOwnedProjectionKey>),
}

impl PartialEq for ProjectionCacheKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Opaque(a), Self::Opaque(b)) => a == b,
            (Self::Source(a_plan, a), Self::Source(b_plan, b)) => {
                Arc::ptr_eq(a_plan, b_plan) && a == b
            }
            (Self::Owned(a_plan, a), Self::Owned(b_plan, b)) => {
                Arc::ptr_eq(a_plan, b_plan) && a == b
            }
            _ => false,
        }
    }
}
impl Eq for ProjectionCacheKey {}

impl StageAudio {
    fn projection_cache_key(
        &self,
        projection: &AudioStageProjection<'_>,
        control: WorkControl<'_>,
    ) -> Result<ProjectionCacheKey, StageAudioError> {
        if !projection.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let (key, comparison) = match (projection.owned_key(), projection.source_key()) {
            (Some(key), _) => (
                ProjectionCacheKey::Owned(Arc::clone(&self.plan), Arc::clone(key)),
                key.comparison_work(),
            ),
            (None, Some(key)) => (
                ProjectionCacheKey::Source(Arc::clone(&self.plan), Arc::clone(key)),
                key.comparison_work(),
            ),
            (None, None) => (ProjectionCacheKey::Opaque(projection.identity()), 1),
        };
        let count = {
            let work = control.work.borrow();
            work.projected.len() + work.projected_preflight.len() + work.projected_active.len() + 1
        };
        control.spend_plan_work(count * comparison)?;
        Ok(key)
    }

    pub(super) fn prepare_projected(
        &mut self,
        projection: &AudioStageProjection<'_>,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Arc<PreparedBlock>, StageAudioError> {
        #[cfg(test)]
        let _validation_caller = stage_validation_policy_before_trace::enter(
            self,
            control,
            "caller_projected",
            || serde_json::json!({"depth":depth,"descriptor":projection.stage().descriptor()}),
        );
        control.check()?;
        let identity = self.projection_cache_key(projection, control)?;
        let cached = control
            .work
            .borrow()
            .projected
            .iter()
            .find(|(key, _)| *key == identity)
            .map(|(_, block)| Arc::clone(block));
        #[cfg(test)]
        stage_validation_policy_before_trace::mark(
            self,
            control,
            "projected_memo_candidate",
            || serde_json::json!({"hit":cached.is_some()}),
        );
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
            #[cfg(test)]
            stage_validation_policy_before_trace::mark(
                self,
                control,
                "projected_memo_return",
                || serde_json::json!({"relative_depth":block.relative_depth}),
            );
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
        if projection.stage().descriptor().pitch.ramp().is_some() {
            // Projections retime a stage's input tape; a ramp's quadratic
            // clock has no such projection, and validation keeps the pauses
            // and derived clocks that would need one outside it.
            return Err(PlanError::InvalidPlan("a speed ramp has no projected input").into());
        }
        let rate = projection.stage().descriptor().rate;
        let recipe = StreamRecipe::new(
            input_frames,
            output_frames,
            StretchRate::new(
                u64::try_from(rate.numerator()).map_err(|_| TimeError::Overflow)?,
                u64::try_from(rate.denominator()).map_err(|_| TimeError::Overflow)?,
            )?,
            i32::from(projection.stage().descriptor().pitch.semitones()),
        )?;
        #[cfg(not(test))]
        let reservation = self.reserve_stream(input_frames, output_frames, control)?;
        #[cfg(test)]
        let reservation = {
            let result = self.reserve_stream(input_frames, output_frames, control);
            let reservation = stage_validation_policy_before_trace::step(
                self,
                control,
                "stream_reservation_result",
                result,
            )?;
            stage_validation_policy_before_trace::reserved(self, control, &reservation);
            reservation
        };
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
        #[cfg(test)]
        stage_validation_policy_before_trace::block_result(
            self,
            control,
            "build_result_before_release",
            &result,
        );
        control.work.borrow_mut().projected_active.pop();
        #[cfg(test)]
        stage_validation_policy_before_trace::mark(self, control, "stream_release_before", || {
            serde_json::json!({
            "resident_frames":reservation.resident_frames,"disk_bytes":reservation.disk_bytes})
        });
        self.release_stream(reservation);
        #[cfg(test)]
        stage_validation_policy_before_trace::mark(self, control, "stream_release_after", || {
            serde_json::json!({})
        });
        let block = result?;
        let frames = block.samples.resident_frames();
        let disk = block.samples.disk_bytes();
        self.make_room_for(frames, disk, false, control)?;
        let block = Arc::new(block);
        let mut work = control.work.borrow_mut();
        work.projected_resident_frames += frames;
        work.projected_disk_bytes += disk;
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
        &mut self,
        projection: &AudioStageProjection<'_>,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        #[cfg(test)]
        let _before_scope = before::enter(self, control, "projected", depth);
        control.check()?;
        self.check_projected_depth(depth, 0)?;
        let identity = self.projection_cache_key(projection, control)?;
        if let Some((_, relative)) = control
            .work
            .borrow()
            .projected_preflight
            .iter()
            .find(|(key, _)| *key == identity)
        {
            #[cfg(test)]
            before::mark(
                self,
                control,
                "memo_hit",
                || serde_json::json!({"relative_depth":relative}),
            );
            self.check_projected_depth(depth, *relative)?;
            return Ok(*relative);
        }
        #[cfg(test)]
        before::mark(self, control, "memo_miss", || {
            before::projection_value(projection, &self.plan)
        });
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
        #[cfg(test)]
        {
            drop(work);
            before::mark(
                self,
                control,
                "memo_inserted",
                || serde_json::json!({"relative_depth":relative_depth}),
            );
        }
        Ok(relative_depth)
    }

    fn preflight_current_stage(
        &mut self,
        stage: &AudioStage<'_>,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        #[cfg(test)]
        let _before_scope = before::enter(self, control, "current_stage", depth);
        control.check()?;
        self.check_projected_depth(depth, 0)?;
        if let Some((_, _, relative)) = control
            .work
            .borrow()
            .intrinsic_preflight
            .iter()
            .find(|(plan, key, _)| Arc::ptr_eq(plan, &self.plan) && key == stage.descriptor())
        {
            #[cfg(test)]
            before::mark(
                self,
                control,
                "memo_hit",
                || serde_json::json!({"relative_depth":relative}),
            );
            self.check_projected_depth(depth, *relative)?;
            return Ok(*relative);
        }
        #[cfg(test)]
        before::mark(self, control, "memo_miss", || {
            serde_json::json!({
            "stage":stage.descriptor(),"belongs_to_plan":stage.belongs_to(&self.plan)})
        });
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
        #[cfg(test)]
        {
            drop(work);
            before::mark(
                self,
                control,
                "memo_inserted",
                || serde_json::json!({"relative_depth":relative}),
            );
        }
        Ok(relative)
    }

    fn preflight_intrinsic(
        &mut self,
        input: SignalInput<'_, '_>,
        input_end: SignalSample,
        output: SignalInput<'_, '_>,
        output_end: SignalSample,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        #[cfg(test)]
        let _before_scope = before::enter(self, control, "intrinsic", depth);
        #[cfg(test)]
        before::mark(self, control, "intrinsic_arguments", || {
            serde_json::json!({
            "input":before::mode(input),"input_end":input_end.0,
            "output":before::mode(output),"output_end":output_end.0,"depth":depth})
        });

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
            #[cfg(test)]
            before::mark(
                self,
                control,
                "input_chunk_enter",
                || serde_json::json!({"range":cursor..end,"mode":before::mode(input)}),
            );
            if let SignalInput::Mix(mix) = input {
                let query = mix.query(cursor..end, control.query_limits()?)?;
                #[cfg(test)]
                before::mark(self, control, "mix_query_constructed", || {
                    before::mix_value(&query, &self.plan)
                });
                control.spend_plan_work(query.work)?;
                #[cfg(test)]
                before::mark(self, control, "mix_query_debited", || {
                    before::mix_value(&query, &self.plan)
                });
                relative_depth =
                    relative_depth.max(self.preflight_mix_query(&query, control, depth)?);
                #[cfg(test)]
                before::mark(
                    self,
                    control,
                    "input_chunk_complete",
                    || serde_json::json!({"range":cursor..end,"mode":"Mix","relative_depth":relative_depth}),
                );
                cursor = end;
                continue;
            }
            #[cfg(test)]
            let _policy_scope = intrinsic_policy::scope(
                control,
                input,
                native_trace::Role::Input,
                cursor..end,
                depth,
            );
            let policy = self.intrinsic_policy(input, cursor..end, control)?;
            #[cfg(test)]
            policy.observe(
                self,
                control,
                input,
                cursor..end,
                "input_policy_constructed",
                0,
            );
            #[cfg(test)]
            if let IntrinsicPolicy::Ordinary(query) = &policy {
                native_tape_source_before_trace::record_preflight(
                    input,
                    control,
                    cursor,
                    end,
                    query,
                    "projection_input_policy_constructed",
                );
            }
            if !policy.was_forwarded() {
                control.spend_plan_work(policy.view().work)?;
            }
            #[cfg(test)]
            policy.observe(self, control, input, cursor..end, "input_policy_debited", 0);
            #[cfg(test)]
            let mut before_completed = 0;
            for content in policy.view().contents {
                #[cfg(test)]
                policy.observe(
                    self,
                    control,
                    input,
                    cursor..end,
                    "input_content_before",
                    before_completed,
                );
                #[cfg(test)]
                native_trace::use_view(
                    control,
                    native_trace::UsePhase::ContentBefore(before_completed),
                    policy.view(),
                );
                control.preflight_content(content, &self.plan)?;
                #[cfg(test)]
                {
                    before_completed += 1;
                    policy.observe(
                        self,
                        control,
                        input,
                        cursor..end,
                        "input_content_accepted",
                        before_completed,
                    );
                    native_trace::use_view(
                        control,
                        native_trace::UsePhase::ContentAccepted(before_completed),
                        policy.view(),
                    );
                }
            }
            #[cfg(test)]
            if let IntrinsicPolicy::Ordinary(query) = &policy {
                native_tape_source_before_trace::record_preflight(
                    input,
                    control,
                    cursor,
                    end,
                    query,
                    "projection_input_inventory_accepted",
                );
            }
            #[cfg(test)]
            {
                native_trace::use_view(control, native_trace::UsePhase::GraphBefore, policy.view());
                native_trace::mark(control, native_trace::GraphPhase::Begin);
            }
            let query = input.query(cursor..end, control.query_limits()?)?;
            #[cfg(test)]
            before::graph(self, control, &query, "graph_constructed");
            control.spend_plan_work(query.work)?;
            #[cfg(test)]
            before::graph(self, control, &query, "graph_debited");
            #[cfg(test)]
            {
                native_trace::use_view(
                    control,
                    native_trace::UsePhase::GraphDebited,
                    policy.view(),
                );
                native_trace::mark(control, native_trace::GraphPhase::Debited);
            }
            relative_depth =
                relative_depth.max(self.preflight_signal_query(&query, control, depth)?);
            #[cfg(test)]
            {
                native_trace::use_view(
                    control,
                    native_trace::UsePhase::RecursiveComplete,
                    policy.view(),
                );
                native_trace::mark(control, native_trace::GraphPhase::RecursiveComplete);
            }
            #[cfg(test)]
            before::mark(
                self,
                control,
                "input_chunk_complete",
                || serde_json::json!({"range":cursor..end,"relative_depth":relative_depth}),
            );
            cursor = end;
        }
        cursor = SignalSample(0);
        while cursor < output_end {
            let end = SignalSample((cursor.0 + i64::from(MAX_OUTPUT_FRAMES)).min(output_end.0));
            #[cfg(test)]
            before::mark(
                self,
                control,
                "output_chunk_enter",
                || serde_json::json!({"range":cursor..end,"mode":before::mode(output)}),
            );
            #[cfg(test)]
            let _policy_scope = intrinsic_policy::scope(
                control,
                output,
                native_trace::Role::Output,
                cursor..end,
                depth,
            );
            let policy = self.intrinsic_policy(output, cursor..end, control)?;
            #[cfg(test)]
            policy.observe(
                self,
                control,
                output,
                cursor..end,
                "output_policy_constructed",
                0,
            );
            #[cfg(test)]
            if let IntrinsicPolicy::Ordinary(query) = &policy {
                native_tape_source_before_trace::record_preflight(
                    output,
                    control,
                    cursor,
                    end,
                    query,
                    "projection_output_policy_constructed",
                );
            }
            if !policy.was_forwarded() {
                control.spend_plan_work(policy.view().work)?;
            }
            #[cfg(test)]
            policy.observe(
                self,
                control,
                output,
                cursor..end,
                "output_policy_debited",
                0,
            );
            #[cfg(test)]
            let mut before_completed = 0;
            for content in policy.view().contents {
                #[cfg(test)]
                policy.observe(
                    self,
                    control,
                    output,
                    cursor..end,
                    "output_content_before",
                    before_completed,
                );
                #[cfg(test)]
                native_trace::use_view(
                    control,
                    native_trace::UsePhase::ContentBefore(before_completed),
                    policy.view(),
                );
                control.preflight_content(content, &self.plan)?;
                #[cfg(test)]
                {
                    before_completed += 1;
                    policy.observe(
                        self,
                        control,
                        output,
                        cursor..end,
                        "output_content_accepted",
                        before_completed,
                    );
                    native_trace::use_view(
                        control,
                        native_trace::UsePhase::ContentAccepted(before_completed),
                        policy.view(),
                    );
                }
            }
            #[cfg(test)]
            if let IntrinsicPolicy::Ordinary(query) = &policy {
                native_tape_source_before_trace::record_preflight(
                    output,
                    control,
                    cursor,
                    end,
                    query,
                    "projection_output_inventory_accepted",
                );
            }
            #[cfg(test)]
            native_trace::use_view(
                control,
                native_trace::UsePhase::OutputComplete,
                policy.view(),
            );
            #[cfg(test)]
            before::mark(
                self,
                control,
                "output_chunk_complete",
                || serde_json::json!({"range":cursor..end}),
            );
            cursor = end;
        }
        Ok(relative_depth)
    }

    pub(super) fn preflight_signal_query(
        &mut self,
        query: &AudioSignalQuery<'_>,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        #[cfg(test)]
        let _before_scope = before::enter(self, control, "signal_query", depth);
        self.check_projected_depth(depth, 0)?;
        #[cfg(test)]
        before::graph(self, control, query, "recursive_graph_enter");
        let mut relative_depth = 0;
        #[cfg(test)]
        let mut before_indices = 0..query.spans.len();
        for span in &query.spans {
            control.check()?;
            #[cfg(test)]
            let before_completed = before_indices.next().unwrap();
            #[cfg(test)]
            before::mark(self, control, "recursive_span_before", || {
                serde_json::json!({"index":before_completed,"samples":span.samples,
                "kind":match &span.content {AudioSignalContent::Leaf(_)=>"Leaf",AudioSignalContent::Stage(_)=>"Stage",
                    AudioSignalContent::Bound(_)=>"Bound",AudioSignalContent::ProjectedStage(_)=>"ProjectedStage"}})
            });
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
            #[cfg(test)]
            {
                before::mark(
                    self,
                    control,
                    "recursive_span_complete",
                    || serde_json::json!({"completed":before_completed+1,"relative_depth":relative_depth}),
                );
            }
        }
        Ok(relative_depth)
    }
}
