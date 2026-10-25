//! Bounded scheduling state and the acknowledged compute clock. No I/O.

use std::time::{Duration, Instant};

pub(super) const MAX_ACKNOWLEDGED_PAUSE: Duration = Duration::from_secs(2 * 60 * 60);

#[derive(Default)]
pub(super) struct PauseControl {
    latest: Option<(u64, bool)>,
    in_flight: Option<(u64, bool)>,
    acknowledged: Option<(u64, bool)>,
    paused_since: Option<Instant>,
    paused_total: Duration,
}

impl PauseControl {
    pub fn request(&mut self, sequence: u64, paused: bool) -> Result<bool, &'static str> {
        if sequence == 0 {
            return Err("pause sequence must be nonzero");
        }
        if self.latest == Some((sequence, paused)) {
            return Ok(false);
        }
        if self.latest.is_some_and(|(last, _)| sequence <= last) {
            return Err("pause sequence must increase");
        }
        self.latest = Some((sequence, paused));
        Ok(true)
    }

    pub fn pending(&self) -> Option<(u64, bool)> {
        if self.in_flight.is_none() && self.latest != self.acknowledged {
            self.latest
        } else {
            None
        }
    }

    pub fn sent(&mut self, value: (u64, bool)) {
        self.in_flight = Some(value);
    }

    /// Only a dispatched request may be acknowledged. Superseded but genuine
    /// acknowledgements update the physical compute clock and must reach observers.
    /// The return value distinguishes a newly validated acknowledgement from a duplicate;
    /// it does not imply that the latest desired scheduling state has been reached.
    pub fn acknowledge(
        &mut self,
        sequence: u64,
        paused: bool,
        now: Instant,
    ) -> Result<bool, &'static str> {
        let value = (sequence, paused);
        if self.acknowledged == Some(value) {
            return Ok(false);
        }
        if self.in_flight != Some(value) {
            return Err("pause acknowledgement does not match a sent request");
        }
        self.in_flight = None;
        self.acknowledged = Some(value);
        if paused {
            self.paused_since.get_or_insert(now);
        } else if let Some(start) = self.paused_since.take() {
            self.paused_total = self
                .paused_total
                .saturating_add(now.saturating_duration_since(start));
        }
        Ok(true)
    }

    pub fn paused_duration(&self, now: Instant) -> Duration {
        self.paused_total.saturating_add(
            self.paused_since
                .map_or(Duration::ZERO, |start| now.saturating_duration_since(start)),
        )
    }

    pub fn compute_duration(&self, started: Instant, now: Instant) -> Duration {
        now.saturating_duration_since(started)
            .saturating_sub(self.paused_duration(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_updates_coalesce_and_only_sent_acknowledgements_change_the_clock() {
        let start = Instant::now();
        let mut pause = PauseControl::default();
        pause.request(1, true).unwrap();
        assert_eq!(
            pause.paused_duration(start + Duration::from_secs(10)),
            Duration::ZERO
        );
        pause.sent(pause.pending().unwrap());
        pause.request(2, false).unwrap();
        pause.request(3, true).unwrap();
        pause.request(4, false).unwrap();
        assert!(pause.pending().is_none());
        assert!(pause.acknowledge(2, false, start).is_err());
        assert!(
            pause
                .acknowledge(1, true, start + Duration::from_secs(10))
                .unwrap()
        );
        assert_eq!(pause.pending(), Some((4, false)));
        pause.sent(pause.pending().unwrap());
        assert!(
            !pause
                .acknowledge(1, true, start + Duration::from_secs(15))
                .unwrap()
        );
        assert!(pause.acknowledge(4, true, start).is_err());
        assert!(
            pause
                .acknowledge(4, false, start + Duration::from_secs(20))
                .unwrap()
        );
        assert_eq!(
            pause.compute_duration(start, start + Duration::from_secs(25)),
            Duration::from_secs(15)
        );
        assert!(pause.pending().is_none());
        assert!(pause.request(0, true).is_err());
        assert!(pause.request(3, false).is_err());
    }

    #[test]
    fn superseded_resume_is_observable_and_stops_discounting_compute_time() {
        let start = Instant::now();
        let mut pause = PauseControl::default();
        pause.request(1, true).unwrap();
        pause.sent((1, true));
        assert!(pause.acknowledge(1, true, start).unwrap());
        pause.request(2, false).unwrap();
        pause.sent((2, false));
        pause.request(3, true).unwrap();
        assert!(
            pause
                .acknowledge(2, false, start + Duration::from_secs(5))
                .unwrap()
        );
        assert_eq!(pause.pending(), Some((3, true)));
        assert_eq!(
            pause.compute_duration(start, start + Duration::from_secs(8)),
            Duration::from_secs(3)
        );
        pause.sent((3, true));
        assert!(
            pause
                .acknowledge(3, true, start + Duration::from_secs(8))
                .unwrap()
        );
        assert_eq!(
            pause.compute_duration(start, start + Duration::from_secs(10)),
            Duration::from_secs(3)
        );
        assert!(
            !pause
                .acknowledge(3, true, start + Duration::from_secs(10))
                .unwrap()
        );
    }

    #[test]
    fn acknowledged_pause_has_a_separate_bounded_lifetime() {
        let start = Instant::now();
        let mut pause = PauseControl::default();
        pause.request(1, true).unwrap();
        pause.sent((1, true));
        pause.acknowledge(1, true, start).unwrap();
        assert_eq!(
            pause.compute_duration(start, start + MAX_ACKNOWLEDGED_PAUSE),
            Duration::ZERO
        );
        assert_eq!(
            pause.paused_duration(start + MAX_ACKNOWLEDGED_PAUSE),
            MAX_ACKNOWLEDGED_PAUSE
        );
    }
}
