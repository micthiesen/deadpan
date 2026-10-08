//! Ticket-bound picture measurements, independent of the app and GPU.

use std::time::Instant;

/// Keeps background completion tied to its admitted input through this frame,
/// unless routing actual new keyboard or pointer input replaces that origin.
#[derive(Default)]
pub(super) struct InputOrigins {
    admitted: Option<Instant>,
    last_dispatched: Option<Instant>,
    current: Option<Instant>,
    committed: Option<Instant>,
    dispatching: bool,
    repeats: std::collections::VecDeque<Option<Instant>>,
    dequeued_repeat: Option<Option<Instant>>,
}

impl InputOrigins {
    pub fn begin_frame(&mut self, current: Option<Instant>) {
        self.current = current;
        self.committed = None;
        self.dispatching = false;
    }

    /// Returns an origin only for a command completion, for its commit metric.
    pub fn observe(&mut self, stage: &str) -> Option<Instant> {
        match stage {
            "input_dispatch" => {
                self.dispatching = true;
                if self.current.is_some() {
                    self.last_dispatched = self.current;
                }
            }
            "command_admitted" => {
                // A scripted picker can complete before new input is routed.
                self.admitted = self.dequeued_repeat.take().unwrap_or(self.last_dispatched);
            }
            "command_rejected" => {
                self.dequeued_repeat = None;
            }
            "repeat_queued" => {
                if self.repeats.len() < crate::preview::repeat_queue::MAX_WAITING {
                    self.repeats.push_back(self.last_dispatched);
                }
            }
            "repeat_dequeued" => {
                self.dequeued_repeat = Some(self.repeats.pop_front().flatten());
            }
            "repeat_cancelled" => {
                self.repeats.pop_front();
            }
            "command_committed" => {
                self.committed = self.admitted.take();
                return self.committed;
            }
            _ => {}
        }
        None
    }

    pub fn picture_input(&self) -> Option<Instant> {
        if self.dispatching {
            // Final-layout requests can follow an idle input-dispatch stage.
            // Preserve this frame's commit origin until actual input replaces it;
            // begin_frame clears it before any unrelated following frame.
            self.current.or(self.committed)
        } else {
            self.committed
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Requested,
    Decoded,
    Submitted,
    Completed,
    Superseded,
    Failed,
    Stale,
    Repeated,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Decoded => "decoded",
            Self::Submitted => "submitted",
            Self::Completed => "completed",
            Self::Superseded => "superseded",
            Self::Failed => "failed",
            Self::Stale => "stale",
            Self::Repeated => "retained_or_repeated",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Event<T> {
    pub stage: &'static str,
    pub at: Instant,
    pub ticket: Option<T>,
    pub outcome: Option<Outcome>,
    pub request_elapsed_ms: Option<f64>,
    pub worker_timing: Option<crate::worker::WorkerTiming>,
}

#[derive(Debug)]
pub(super) struct Measurement {
    pub name: &'static str,
    pub elapsed_ms: f64,
}

pub(super) struct Observation<T> {
    pub events: Vec<Event<T>>,
    pub measurements: Vec<Measurement>,
}

struct Request<T> {
    ticket: T,
    requested: Instant,
    input: Option<Instant>,
    decoded: bool,
    submitted: bool,
}

pub(super) struct PictureTelemetry<T> {
    active: Option<Request<T>>,
}

impl<T> Default for PictureTelemetry<T> {
    fn default() -> Self {
        Self { active: None }
    }
}

impl<T: Copy + Eq> PictureTelemetry<T> {
    pub fn observe(&mut self, mut event: Event<T>, input: Option<Instant>) -> Observation<T> {
        let mut observed = Observation {
            events: Vec::new(),
            measurements: Vec::new(),
        };
        let Some(ticket) = event.ticket else {
            observed.events.push(event);
            return observed;
        };
        let mut requested = None;
        let mut first_decode = false;
        if event.stage == "picture_requested" {
            if let Some(previous) = self.active.take() {
                observed.events.push(Event {
                    stage: "picture_superseded",
                    at: event.at,
                    ticket: Some(previous.ticket),
                    outcome: Some(Outcome::Superseded),
                    request_elapsed_ms: Some(milliseconds(event.at, previous.requested)),
                    worker_timing: None,
                });
            }
            self.active = Some(Request {
                ticket,
                requested: event.at,
                input,
                decoded: false,
                submitted: false,
            });
        } else if event.outcome != Some(Outcome::Stale) {
            let active = self
                .active
                .as_mut()
                .filter(|request| request.ticket == ticket);
            if let Some(request) = active {
                requested = Some(request.requested);
                event.request_elapsed_ms = Some(milliseconds(event.at, request.requested));
                if event.outcome == Some(Outcome::Failed) {
                    // Expected injected failures remain explicit trace evidence,
                    // and never enter successful latency distributions.
                    self.active = None;
                } else if event.stage == "picture_received"
                    && event.outcome == Some(Outcome::Decoded)
                    && !request.decoded
                {
                    request.decoded = true;
                    first_decode = true;
                    observed.measurements.push(Measurement {
                        name: "request_to_decode_delivery_ms",
                        elapsed_ms: milliseconds(event.at, request.requested),
                    });
                } else if event.stage == "picture_submitted"
                    && request.decoded
                    && !request.submitted
                {
                    request.submitted = true;
                    observed.measurements.push(Measurement {
                        name: "request_to_picture_submission_ms",
                        elapsed_ms: milliseconds(event.at, request.requested),
                    });
                } else {
                    event.outcome = Some(Outcome::Repeated);
                }
            } else if event.outcome != Some(Outcome::Failed) {
                event.outcome = Some(if event.stage == "picture_submitted" {
                    Outcome::Repeated
                } else {
                    Outcome::Stale
                });
            }
        }
        if event.stage == "picture_received" {
            observed.worker_timing(&event, requested, first_decode);
        }
        observed.events.push(event);
        observed
    }

    /// The caller invokes this only after its full UI submission has completed.
    pub fn composed(&mut self, at: Instant) -> Observation<T> {
        let mut observed = Observation {
            events: Vec::new(),
            measurements: Vec::new(),
        };
        if !self
            .active
            .as_ref()
            .is_some_and(|request| request.submitted)
        {
            return observed;
        }
        let request = self.active.take().expect("submitted request checked");
        let elapsed_ms = milliseconds(at, request.requested);
        observed.events.push(Event {
            stage: "picture_composed",
            at,
            ticket: Some(request.ticket),
            outcome: Some(Outcome::Completed),
            request_elapsed_ms: Some(elapsed_ms),
            worker_timing: None,
        });
        observed.measurements.push(Measurement {
            name: "request_to_picture_complete_ms",
            elapsed_ms,
        });
        if let Some(input) = request.input {
            observed.measurements.push(Measurement {
                name: "input_to_picture_complete_ms",
                elapsed_ms: milliseconds(at, input),
            });
        }
        observed
    }
}

impl<T: Copy> Observation<T> {
    fn worker_timing(
        &mut self,
        receipt: &Event<T>,
        requested: Option<Instant>,
        first_decode: bool,
    ) {
        let Some(timing) = receipt.worker_timing else {
            return;
        };
        // Trace entries retain actual worker observations even for failed,
        // stale, repeated or malformed timing. They arrive with the receipt,
        // so trace insertion order is not necessarily timestamp order.
        for (stage, at) in [
            ("picture_worker_started", Some(timing.started)),
            ("picture_worker_finished", Some(timing.finished)),
            ("picture_worker_published", timing.published),
        ] {
            if let Some(at) = at {
                self.events.push(Event {
                    stage,
                    at,
                    ticket: receipt.ticket,
                    outcome: receipt.outcome,
                    request_elapsed_ms: requested
                        .filter(|requested| *requested <= at)
                        .map(|requested| milliseconds(at, requested)),
                    worker_timing: None,
                });
            }
        }
        let (Some(requested), Some(published)) = (requested, timing.published) else {
            return;
        };
        if !first_decode
            || requested > timing.started
            || timing.started > timing.finished
            || timing.finished > published
            || published > receipt.at
        {
            return;
        }
        for (name, start, end) in [
            ("request_to_worker_start_ms", requested, timing.started),
            ("worker_execution_ms", timing.started, timing.finished),
            ("worker_publication_delay_ms", timing.finished, published),
            ("publication_to_decode_delivery_ms", published, receipt.at),
        ] {
            self.measurements.push(Measurement {
                name,
                elapsed_ms: milliseconds(end, start),
            });
        }
    }
}

fn milliseconds(end: Instant, start: Instant) -> f64 {
    end.saturating_duration_since(start).as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn queued_repeats_keep_fifo_input_origins_and_cancel_without_leaking_them() {
        let first = Instant::now();
        let second = first + Duration::from_millis(10);
        let third = first + Duration::from_millis(20);
        let mut origins = InputOrigins::default();
        origins.begin_frame(Some(first));
        origins.observe("input_dispatch");
        origins.observe("command_admitted");
        origins.observe("repeat_queued");
        origins.begin_frame(Some(second));
        origins.observe("input_dispatch");
        origins.observe("repeat_queued");
        assert_eq!(origins.observe("command_committed"), Some(first));
        origins.observe("repeat_dequeued");
        origins.observe("command_admitted");
        assert_eq!(origins.observe("command_committed"), Some(first));
        origins.observe("repeat_dequeued");
        origins.observe("command_admitted");
        assert_eq!(origins.observe("command_committed"), Some(second));
        origins.observe("repeat_queued");
        origins.observe("repeat_cancelled");
        origins.begin_frame(Some(third));
        origins.observe("input_dispatch");
        origins.observe("command_admitted");
        assert_eq!(origins.observe("command_committed"), Some(third));
        assert!(origins.repeats.is_empty());
        // Missing origin evidence stays unknown, never inferred from a newer key.
        origins.observe("repeat_dequeued");
        origins.observe("command_admitted");
        assert_eq!(origins.observe("command_committed"), None);
    }

    #[test]
    fn completion_before_new_input_keeps_its_admitted_origin() {
        let original = Instant::now();
        let later = original + Duration::from_millis(100);
        let mut origins = InputOrigins::default();
        origins.begin_frame(Some(original));
        origins.observe("input_dispatch");
        origins.observe("command_admitted");

        // Production ui() receives the earlier command before routing the
        // unrelated event already present in this frame's RawInput.
        origins.begin_frame(Some(later));
        assert_eq!(origins.observe("command_committed"), Some(original));
        assert_eq!(origins.picture_input(), Some(original));
        origins.observe("input_dispatch");
        assert_eq!(origins.picture_input(), Some(later));
        origins.observe("command_admitted");

        origins.begin_frame(None);
        assert_eq!(origins.observe("command_committed"), Some(later));
        assert_eq!(origins.picture_input(), Some(later));
        origins.observe("input_dispatch");
        assert_eq!(origins.picture_input(), Some(later));
        origins.begin_frame(None);
        assert_eq!(origins.picture_input(), None);
    }

    #[test]
    fn deferred_commit_picture_keeps_its_input_without_leaking_to_later_requests() {
        let base = Instant::now();
        let input = base + Duration::from_millis(1);
        let mut origins = InputOrigins::default();
        let mut pictures = PictureTelemetry::default();
        origins.begin_frame(Some(input));
        origins.observe("input_dispatch");
        origins.observe("command_admitted");

        // Slip completion is consumed before idle input routing, but its new
        // committed picture is requested in the final layout pass afterward.
        origins.begin_frame(None);
        let committed_input = origins.observe("command_committed").unwrap();
        assert_eq!(committed_input, input);
        assert_eq!(
            milliseconds(base + Duration::from_millis(10), committed_input),
            9.0
        );
        origins.observe("input_dispatch");
        pictures.observe(
            event(base, 12, "picture_requested", 1, Outcome::Requested),
            origins.picture_input(),
        );

        // The request owns its captured input even after the next frame clears
        // the origin. Completion still requires decode, submit and compose events.
        origins.begin_frame(None);
        origins.observe("input_dispatch");
        assert_eq!(origins.picture_input(), None);
        pictures.observe(
            event(base, 18, "picture_received", 1, Outcome::Decoded),
            origins.picture_input(),
        );
        pictures.observe(
            event(base, 20, "picture_submitted", 1, Outcome::Submitted),
            origins.picture_input(),
        );
        let completed = pictures.composed(base + Duration::from_millis(25));
        assert_eq!(completed.events[0].ticket, Some(1));
        assert_eq!(completed.events[0].outcome, Some(Outcome::Completed));
        assert_eq!(
            measured(&completed),
            vec![
                ("request_to_picture_complete_ms", 13.0),
                ("input_to_picture_complete_ms", 24.0),
            ]
        );
        assert!(
            pictures
                .composed(base + Duration::from_millis(26))
                .measurements
                .is_empty()
        );

        // An unrelated request on a later idle frame has no admitted input.
        origins.begin_frame(None);
        origins.observe("input_dispatch");
        assert_eq!(origins.picture_input(), None);
        pictures.observe(
            event(base, 30, "picture_requested", 2, Outcome::Requested),
            origins.picture_input(),
        );
        pictures.observe(
            event(base, 35, "picture_received", 2, Outcome::Decoded),
            origins.picture_input(),
        );
        pictures.observe(
            event(base, 36, "picture_submitted", 2, Outcome::Submitted),
            origins.picture_input(),
        );
        let unrelated = pictures.composed(base + Duration::from_millis(40));
        assert_eq!(unrelated.events[0].ticket, Some(2));
        assert_eq!(
            measured(&unrelated),
            vec![("request_to_picture_complete_ms", 10.0)]
        );
    }

    #[test]
    fn picker_completion_before_dispatch_does_not_admit_the_new_input() {
        let picker_input = Instant::now();
        let new_input = picker_input + Duration::from_millis(10);
        let mut origins = InputOrigins::default();
        origins.begin_frame(Some(picker_input));
        origins.observe("input_dispatch");
        origins.begin_frame(Some(new_input));
        origins.observe("command_admitted");
        origins.observe("input_dispatch");
        origins.begin_frame(None);
        assert_eq!(origins.observe("command_committed"), Some(picker_input));
    }

    fn event(
        base: Instant,
        ms: u64,
        stage: &'static str,
        ticket: u64,
        outcome: Outcome,
    ) -> Event<u64> {
        Event {
            stage,
            at: base + Duration::from_millis(ms),
            ticket: Some(ticket),
            outcome: Some(outcome),
            request_elapsed_ms: None,
            worker_timing: None,
        }
    }

    #[test]
    fn superseded_decode_and_retained_resize_cannot_complete_new_request() {
        let base = Instant::now();
        let mut telemetry = PictureTelemetry::default();
        telemetry.observe(
            event(base, 1, "picture_requested", 1, Outcome::Requested),
            Some(base),
        );
        telemetry.observe(
            event(base, 2, "picture_received", 1, Outcome::Decoded),
            None,
        );
        let superseded = telemetry.observe(
            event(base, 3, "picture_requested", 2, Outcome::Requested),
            Some(base),
        );
        assert_eq!(superseded.events[0].ticket, Some(1));
        assert_eq!(superseded.events[0].outcome, Some(Outcome::Superseded));
        let retained = telemetry.observe(
            event(base, 4, "picture_submitted", 1, Outcome::Submitted),
            None,
        );
        assert!(retained.measurements.is_empty());
        assert!(
            telemetry
                .composed(base + Duration::from_millis(5))
                .measurements
                .is_empty()
        );
        let stale = telemetry.observe(event(base, 6, "picture_received", 1, Outcome::Stale), None);
        assert!(stale.measurements.is_empty());
        let decoded = telemetry.observe(
            event(base, 7, "picture_received", 2, Outcome::Decoded),
            None,
        );
        assert_eq!(decoded.measurements[0].elapsed_ms, 4.0);
        let submitted = telemetry.observe(
            event(base, 8, "picture_submitted", 2, Outcome::Submitted),
            None,
        );
        assert_eq!(submitted.measurements[0].elapsed_ms, 5.0);
        let completed = telemetry.composed(base + Duration::from_millis(9));
        assert_eq!(completed.events[0].ticket, Some(2));
        assert_eq!(completed.measurements[0].elapsed_ms, 6.0);
        assert_eq!(completed.measurements[1].elapsed_ms, 9.0);
    }

    #[test]
    fn failures_are_retained_without_success_samples_and_recovery_has_its_own_clock() {
        let base = Instant::now();
        let mut telemetry = PictureTelemetry::default();
        telemetry.observe(
            event(base, 1, "picture_requested", 1, Outcome::Requested),
            Some(base),
        );
        let failed =
            telemetry.observe(event(base, 8, "picture_received", 1, Outcome::Failed), None);
        assert_eq!(failed.events[0].outcome, Some(Outcome::Failed));
        assert_eq!(failed.events[0].request_elapsed_ms, Some(7.0));
        assert!(failed.measurements.is_empty());
        assert!(
            telemetry
                .composed(base + Duration::from_millis(9))
                .measurements
                .is_empty()
        );
        telemetry.observe(
            event(base, 10, "picture_requested", 2, Outcome::Requested),
            None,
        );
        telemetry.observe(
            event(base, 12, "picture_received", 2, Outcome::Decoded),
            None,
        );
        telemetry.observe(
            event(base, 13, "picture_submitted", 2, Outcome::Submitted),
            None,
        );
        let recovered = telemetry.composed(base + Duration::from_millis(14));
        assert_eq!(recovered.measurements.len(), 1);
        assert_eq!(recovered.measurements[0].elapsed_ms, 4.0);
    }

    #[test]
    fn repeated_delivery_resize_and_camera_submissions_do_not_duplicate_latency() {
        let base = Instant::now();
        let mut telemetry = PictureTelemetry::default();
        telemetry.observe(
            event(base, 1, "picture_requested", 1, Outcome::Requested),
            None,
        );
        telemetry.observe(
            event(base, 2, "picture_received", 1, Outcome::Decoded),
            None,
        );
        let duplicate = telemetry.observe(
            event(base, 3, "picture_received", 1, Outcome::Decoded),
            None,
        );
        assert!(duplicate.measurements.is_empty());
        telemetry.observe(
            event(base, 4, "picture_submitted", 1, Outcome::Submitted),
            None,
        );
        assert_eq!(
            telemetry
                .composed(base + Duration::from_millis(5))
                .measurements
                .len(),
            1
        );
        for ms in [6, 10] {
            let repeated = telemetry.observe(
                event(base, ms, "picture_submitted", 1, Outcome::Submitted),
                None,
            );
            assert!(repeated.measurements.is_empty());
            assert_eq!(repeated.events[0].outcome, Some(Outcome::Repeated));
            assert!(
                telemetry
                    .composed(base + Duration::from_millis(ms + 1))
                    .measurements
                    .is_empty()
            );
        }
    }

    fn timed_receipt(
        base: Instant,
        received: u64,
        ticket: u64,
        outcome: Outcome,
        phases: [u64; 3],
    ) -> Event<u64> {
        let mut receipt = event(base, received, "picture_received", ticket, outcome);
        receipt.worker_timing = Some(crate::worker::WorkerTiming {
            started: base + Duration::from_millis(phases[0]),
            finished: base + Duration::from_millis(phases[1]),
            published: Some(base + Duration::from_millis(phases[2])),
        });
        receipt
    }

    fn measured(observed: &Observation<u64>) -> Vec<(&'static str, f64)> {
        observed
            .measurements
            .iter()
            .map(|measurement| (measurement.name, measurement.elapsed_ms))
            .collect()
    }

    #[test]
    fn worker_phases_decompose_delivery_and_retain_actual_ticket_timestamps() {
        let base = Instant::now();
        let mut telemetry = PictureTelemetry::default();
        telemetry.observe(
            event(base, 10, "picture_requested", 1, Outcome::Requested),
            Some(base),
        );
        let decoded = telemetry.observe(
            timed_receipt(base, 25, 1, Outcome::Decoded, [12, 17, 18]),
            None,
        );
        assert_eq!(
            measured(&decoded),
            vec![
                ("request_to_decode_delivery_ms", 15.0),
                ("request_to_worker_start_ms", 2.0),
                ("worker_execution_ms", 5.0),
                ("worker_publication_delay_ms", 1.0),
                ("publication_to_decode_delivery_ms", 7.0),
            ]
        );
        assert_eq!(
            decoded.measurements[1..]
                .iter()
                .map(|measurement| measurement.elapsed_ms)
                .sum::<f64>(),
            decoded.measurements[0].elapsed_ms
        );
        let expected = [
            ("picture_worker_started", 12, 2.0),
            ("picture_worker_finished", 17, 7.0),
            ("picture_worker_published", 18, 8.0),
            ("picture_received", 25, 15.0),
        ];
        assert_eq!(decoded.events.len(), expected.len());
        for (event, (stage, ms, elapsed)) in decoded.events.iter().zip(expected) {
            assert_eq!(event.stage, stage);
            assert_eq!(event.at, base + Duration::from_millis(ms));
            assert_eq!(event.ticket, Some(1));
            assert_eq!(event.outcome, Some(Outcome::Decoded));
            assert_eq!(event.request_elapsed_ms, Some(elapsed));
        }
        telemetry.observe(
            event(base, 26, "picture_submitted", 1, Outcome::Submitted),
            None,
        );
        assert_eq!(
            measured(&telemetry.composed(base + Duration::from_millis(27))),
            vec![
                ("request_to_picture_complete_ms", 17.0),
                ("input_to_picture_complete_ms", 27.0),
            ]
        );
    }

    #[test]
    fn held_receipt_changes_observation_delay_without_changing_worker_phases() {
        let base = Instant::now();
        let mut observations = Vec::new();
        for received in [25, 125] {
            let mut telemetry = PictureTelemetry::default();
            telemetry.observe(
                event(base, 10, "picture_requested", 1, Outcome::Requested),
                None,
            );
            observations.push(telemetry.observe(
                timed_receipt(base, received, 1, Outcome::Decoded, [12, 17, 18]),
                None,
            ));
        }
        let direct = measured(&observations[0]);
        let held = measured(&observations[1]);
        assert_eq!(direct[1..4], held[1..4]);
        assert_eq!(held[0].1 - direct[0].1, 100.0);
        assert_eq!(held[4].1 - direct[4].1, 100.0);
        for (direct, held) in observations[0].events[..3]
            .iter()
            .zip(&observations[1].events[..3])
        {
            assert_eq!(direct.at, held.at);
        }
    }

    #[test]
    fn unsuccessful_and_repeated_receipts_keep_trace_without_worker_samples() {
        let base = Instant::now();
        for (ticket, outcome, first_decoded, expected) in [
            (2, Outcome::Decoded, false, Outcome::Stale),
            (1, Outcome::Stale, false, Outcome::Stale),
            (1, Outcome::Failed, false, Outcome::Failed),
            (1, Outcome::Decoded, true, Outcome::Repeated),
        ] {
            let mut telemetry = PictureTelemetry::default();
            telemetry.observe(
                event(base, 10, "picture_requested", 1, Outcome::Requested),
                None,
            );
            if first_decoded {
                telemetry.observe(
                    timed_receipt(base, 25, 1, Outcome::Decoded, [12, 17, 18]),
                    None,
                );
            }
            let observation =
                telemetry.observe(timed_receipt(base, 30, ticket, outcome, [12, 17, 18]), None);
            assert!(observation.measurements.is_empty());
            assert_eq!(observation.events.len(), 4);
            assert_eq!(observation.events[0].stage, "picture_worker_started");
            for event in observation.events {
                assert_eq!(event.ticket, Some(ticket));
                assert_eq!(event.outcome, Some(expected));
            }
        }
    }

    #[test]
    fn invalid_or_incomplete_worker_timing_never_produces_phase_samples() {
        let base = Instant::now();
        for (phases, published) in [
            ([9, 17, 18], true),
            ([12, 11, 18], true),
            ([12, 17, 16], true),
            ([12, 17, 26], true),
            ([12, 17, 18], false),
        ] {
            let mut telemetry = PictureTelemetry::default();
            telemetry.observe(
                event(base, 10, "picture_requested", 1, Outcome::Requested),
                None,
            );
            let mut receipt = timed_receipt(base, 25, 1, Outcome::Decoded, phases);
            if !published {
                receipt.worker_timing.as_mut().unwrap().published = None;
            }
            let observation = telemetry.observe(receipt, None);
            assert_eq!(
                measured(&observation),
                vec![("request_to_decode_delivery_ms", 15.0)]
            );
            assert_eq!(observation.events.len(), if published { 4 } else { 3 });
            assert_eq!(
                observation.events[0].at,
                base + Duration::from_millis(phases[0])
            );
            if phases[0] < 10 {
                assert_eq!(observation.events[0].request_elapsed_ms, None);
            }
        }
    }

    #[test]
    fn absent_worker_timing_preserves_existing_delivery_without_zero_samples() {
        let base = Instant::now();
        let mut telemetry = PictureTelemetry::default();
        telemetry.observe(
            event(base, 10, "picture_requested", 1, Outcome::Requested),
            None,
        );
        let observation = telemetry.observe(
            event(base, 25, "picture_received", 1, Outcome::Decoded),
            None,
        );
        assert_eq!(
            measured(&observation),
            vec![("request_to_decode_delivery_ms", 15.0)]
        );
        assert_eq!(observation.events.len(), 1);
    }
}
