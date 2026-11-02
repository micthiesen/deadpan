//! Post-DSP output policy stays admitted until the block has been suppressed.
use super::*;

#[cfg(not(test))]
impl StageAudio {
    pub(super) fn suppress_stage_block(
        &mut self,
        output: SignalInput<'_, '_>,
        start: SignalSample,
        samples: &mut [[f32; 2]],
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let end = SignalSample(
            start
                .0
                .checked_add(i64::try_from(samples.len()).map_err(|_| TimeError::Overflow)?)
                .ok_or(TimeError::Overflow)?,
        );
        let limits = control.query_limits()?;
        let policy = self.read_signal_policy_input(output, start..end, limits, control)?;
        if !policy.was_forwarded() {
            control.spend_plan_work(policy.view().work)?;
        }
        for content in policy.view().contents {
            control.preflight_content(content, &self.plan)?;
        }
        control.check()?;
        apply_suppression(start, samples, policy.view().suppressed, |sample| sample.0)?;
        drop(policy);
        Ok(())
    }
}

#[cfg(test)]
impl StageAudio {
    pub(super) fn suppress_stage_block(
        &mut self,
        output: SignalInput<'_, '_>,
        start: SignalSample,
        samples: &mut [[f32; 2]],
        control: WorkControl<'_>,
        ticket: Option<stage_block_policy_native_trace::Ticket>,
    ) -> Result<(), StageAudioError> {
        use super::stage_block_policy_native_trace as trace;
        let _invocation = trace::invocation(control, ticket);
        let result = (|| {
            let end_result: Result<SignalSample, StageAudioError> = (|| {
                Ok(SignalSample(
                    start
                        .0
                        .checked_add(i64::try_from(samples.len()).map_err(|_| TimeError::Overflow)?)
                        .ok_or(TimeError::Overflow)?,
                ))
            })();
            trace::mark(control, ticket, "range_result", || match &end_result {
                Ok(end) => serde_json::json!({"status":"Ok","range":start..*end}),
                Err(error) => {
                    serde_json::json!({"status":"Err","error":stage_block_policy_before_trace::error(error)})
                }
            });
            let end = end_result?;
            trace::cancel_at(control, ticket, trace::UsePoint::BeforeLimits);
            let limits_result = control.query_limits();
            trace::mark(
                control,
                ticket,
                "query_limits_result",
                || match &limits_result {
                    Ok(limits) => {
                        serde_json::json!({"status":"Ok","maximum_work":limits.maximum_work,"maximum_spans":limits.maximum_spans})
                    }
                    Err(error) => {
                        serde_json::json!({"status":"Err","error":stage_block_policy_before_trace::error(error)})
                    }
                },
            );
            let limits = limits_result?;
            trace::factory_begin(control, ticket);
            let policy_result = self.read_signal_policy_input(output, start..end, limits, control);
            trace::factory_result(control, ticket, &policy_result);
            let policy = policy_result?;
            if !policy.was_forwarded() {
                trace::mark(
                    control,
                    ticket,
                    "manual_debit_before",
                    || serde_json::json!({"amount":policy.view().work}),
                );
                let result = control.spend_plan_work(policy.view().work);
                trace::result(control, ticket, "manual_debit_result", &result);
                result?;
            }
            let owner = trace::owner(control, ticket, &policy);
            for (index, content) in policy.view().contents.iter().enumerate() {
                owner.observe("content_before", &policy);
                trace::cancel_at(control, ticket, trace::UsePoint::ContentBefore(index));
                trace::mark(
                    control,
                    ticket,
                    "content_index",
                    || serde_json::json!({"index":index}),
                );
                let result = control.preflight_content(content, &self.plan);
                trace::result(control, ticket, "content_result", &result);
                owner.observe("content_after", &policy);
                result?;
            }
            owner.observe("final_check_before", &policy);
            trace::cancel_at(control, ticket, trace::UsePoint::FinalCheck);
            let result = control.check();
            trace::result(control, ticket, "final_check_result", &result);
            owner.observe("final_check_after", &policy);
            result?;
            owner.observe("apply_before", &policy);
            trace::mark(
                control,
                ticket,
                "apply_pcm_before",
                || serde_json::json!({"pcm_bits":stage_block_policy_before_trace::pcm_value(samples)}),
            );
            let result =
                apply_suppression(start, samples, policy.view().suppressed, |sample| sample.0);
            trace::result(control, ticket, "apply_result", &result);
            trace::mark(
                control,
                ticket,
                "apply_pcm_after",
                || serde_json::json!({"pcm_bits":stage_block_policy_before_trace::pcm_value(samples)}),
            );
            owner.observe("apply_after", &policy);
            result?;
            drop(owner);
            trace::disposed(control, ticket, "owner_drop_before");
            drop(policy);
            trace::disposed(control, ticket, "owner_drop_after");
            Ok(())
        })();
        trace::result(control, ticket, "method_result", &result);
        result
    }
}
