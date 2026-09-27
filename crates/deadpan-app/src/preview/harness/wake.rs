//! Repaint-driven waits for the offscreen harness, without timer polling.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

#[derive(Clone, Copy)]
struct Pending {
    pass: u64,
    at: Instant,
}

#[derive(Default)]
struct State {
    completed: u64,
    // Only the current and preceding pass can still require a repaint. Two
    // slots also bound notifications while a multi-pass egui step is running.
    pending: [Option<Pending>; 2],
}

impl State {
    fn schedule(&mut self, pass: u64, delay: Duration, now: Instant) {
        if pass < self.completed.saturating_sub(1) {
            return;
        }
        let Some(at) = now.checked_add(delay) else {
            return; // egui uses Duration::MAX for no scheduled repaint.
        };
        let slot = &mut self.pending[usize::from(!pass.is_multiple_of(2))];
        match slot {
            Some(previous) if previous.pass > pass => {}
            Some(previous) if previous.pass == pass => previous.at = previous.at.min(at),
            _ => *slot = Some(Pending { pass, at }),
        }
    }

    fn next(&self) -> Option<Instant> {
        self.pending
            .iter()
            .flatten()
            .map(|pending| pending.at)
            .min()
    }
}

#[derive(Default)]
pub(super) struct RepaintWake {
    state: Mutex<State>,
    changed: Condvar,
}

impl RepaintWake {
    /// Only install on kittest's fresh context. Native eframe owns its callback.
    pub fn install(self: &Arc<Self>, context: &egui::Context) {
        let wake = Arc::clone(self);
        context.set_request_repaint_callback(move |info| {
            if info.viewport_id == egui::ViewportId::ROOT {
                wake.request(info.current_cumulative_pass_nr, info.delay, Instant::now());
            }
        });
    }

    fn request(&self, pass: u64, delay: Duration, now: Instant) {
        // egui invokes this under its context lock. Do not call egui or app code.
        self.state
            .lock()
            .expect("harness repaint wake")
            .schedule(pass, delay, now);
        self.changed.notify_one();
    }

    pub fn begin_step(&self, now: Instant) {
        // Service due notifications, retaining future deadlines until their
        // pass expires. An input-driven step can precede a one-shot delayed
        // repaint without replacing it. Requests arriving during this step
        // also survive, including a worker finishing after the UI polls it.
        let mut state = self.state.lock().expect("harness repaint wake");
        for slot in &mut state.pending {
            if slot.is_some_and(|pending| pending.at <= now) {
                *slot = None;
            }
        }
    }

    pub fn finish_step(&self, completed: u64, repaint_delay: Duration, now: Instant) {
        let mut state = self.state.lock().expect("harness repaint wake");
        state.completed = completed;
        for slot in &mut state.pending {
            if slot.is_some_and(|pending| pending.pass < completed.saturating_sub(1)) {
                *slot = None;
            }
        }
        // The final output includes egui's outstanding immediate pass and
        // delayed repaint requests, even when its callback was coalesced.
        state.schedule(completed.saturating_sub(1), repaint_delay, now);
    }

    /// Returns true for a due repaint, false at the finite scenario deadline.
    /// The mutex predicate covers notifications both before and during waiting.
    pub fn wait_until(&self, deadline: Instant) -> bool {
        let mut state = self.state.lock().expect("harness repaint wake");
        loop {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let next = state.next();
            if next.is_some_and(|at| at <= now) {
                return true;
            }
            let until = next.map_or(deadline, |at| at.min(deadline));
            (state, _) = self
                .changed
                .wait_timeout(state, until - now)
                .expect("harness repaint wake");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn pending_before_wait_is_not_lost_and_step_consumes_it() {
        let wake = RepaintWake::default();
        wake.request(0, Duration::ZERO, Instant::now());
        assert!(wake.wait_until(Instant::now() + Duration::from_secs(5)));
        wake.begin_step(Instant::now());
        wake.finish_step(1, Duration::MAX, Instant::now());
        assert!(wake.state.lock().unwrap().next().is_none());
        assert!(!wake.wait_until(Instant::now()));
    }

    #[test]
    fn delayed_requests_coalesce_and_immediate_work_wins() {
        let mut state = State::default();
        let now = Instant::now();
        state.schedule(0, Duration::from_secs(4), now);
        state.schedule(0, Duration::from_secs(8), now);
        assert_eq!(state.next(), Some(now + Duration::from_secs(4)));
        state.schedule(0, Duration::from_secs(2), now);
        assert_eq!(state.next(), Some(now + Duration::from_secs(2)));
        state.schedule(0, Duration::ZERO, now);
        assert_eq!(state.next(), Some(now));
    }

    #[test]
    fn requests_during_step_and_final_output_survive_old_pass_pruning() {
        let wake = RepaintWake::default();
        let now = Instant::now();
        wake.begin_step(now);
        wake.request(0, Duration::ZERO, now); // discarded internal pass
        wake.request(1, Duration::from_secs(2), now);
        wake.request(2, Duration::from_secs(1), now); // after final pass
        wake.finish_step(2, Duration::from_secs(3), now);
        assert_eq!(
            wake.state.lock().unwrap().next(),
            Some(now + Duration::from_secs(1))
        );
        wake.request(0, Duration::ZERO, now); // stale callback cannot evict pass 2
        assert_eq!(
            wake.state.lock().unwrap().next(),
            Some(now + Duration::from_secs(1))
        );
        wake.begin_step(now);
        wake.finish_step(3, Duration::ZERO, now);
        assert_eq!(wake.state.lock().unwrap().next(), Some(now));
    }

    #[test]
    fn absent_or_late_repaint_obeys_finite_deadline() {
        let wake = RepaintWake::default();
        wake.request(0, Duration::from_secs(60), Instant::now());
        assert!(!wake.wait_until(Instant::now() + Duration::from_millis(10)));
        // The scenario timeout must not consume the still-pending repaint.
        assert!(wake.state.lock().unwrap().next().is_some());
    }

    #[test]
    fn future_deadline_survives_an_early_input_step_until_due_or_stale() {
        let wake = RepaintWake::default();
        let now = Instant::now();
        let due = now + Duration::from_secs(5);
        // A delayed callback at the end of pass 1 is still eligible after an
        // input-driven pass 2, even if that pass does not reissue the timer.
        wake.finish_step(1, Duration::MAX, now);
        wake.request(1, Duration::from_secs(5), now);
        wake.begin_step(now + Duration::from_secs(1));
        wake.finish_step(2, Duration::MAX, now + Duration::from_secs(1));
        assert_eq!(wake.state.lock().unwrap().next(), Some(due));
        wake.begin_step(due);
        assert!(wake.state.lock().unwrap().next().is_none());

        // A future request must still expire once two passes supersede it.
        wake.request(2, Duration::from_secs(5), due);
        wake.begin_step(due);
        wake.finish_step(4, Duration::MAX, due);
        assert!(wake.state.lock().unwrap().next().is_none());
    }

    #[test]
    fn notification_crossing_the_wait_handoff_is_not_lost() {
        let wake = Arc::new(RepaintWake::default());
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let (done_tx, done_rx) = mpsc::sync_channel(0);
        let producer = Arc::clone(&wake);
        let thread = std::thread::spawn(move || {
            ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            producer.request(0, Duration::ZERO, Instant::now());
            done_tx.send(()).unwrap();
        });
        // The producer may notify either just before or after wait_timeout
        // releases the predicate lock. Both orderings must wake the consumer.
        ready_tx.send(()).unwrap();
        assert!(wake.wait_until(Instant::now() + Duration::from_secs(5)));
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        thread.join().unwrap();
    }

    #[test]
    fn actual_egui_context_wakes_from_cloned_background_context() {
        let context = egui::Context::default();
        let wake = Arc::new(RepaintWake::default());
        wake.install(&context);
        // Drain initial layout/egui settling repaints without a native window.
        for _ in 0..4 {
            wake.begin_step(Instant::now());
            let mut output = context.run_ui(egui::RawInput::default(), |_ui| {});
            output.textures_delta.clear();
            wake.finish_step(
                context.cumulative_pass_nr(),
                output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
                Instant::now(),
            );
        }
        assert!(wake.state.lock().unwrap().next().is_none());
        let producer = context.clone();
        let thread = std::thread::spawn(move || producer.request_repaint());
        assert!(wake.wait_until(Instant::now() + Duration::from_secs(5)));
        thread.join().unwrap();
    }
}
