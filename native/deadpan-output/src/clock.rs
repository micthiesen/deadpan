//! Bounded interpretation of submitted device intervals on the stream clock.
//! This controller-side clock never infers delivery from producer progress or
//! extrapolates through a missing report. Host timestamps remain estimates of
//! playback, not measurements at the speaker.

use crate::{Generation, MAX_CALLBACK_FRAMES, RenderReport, RenderStatus, SAMPLE_RATE};

const NANOS_PER_SECOND: u128 = 1_000_000_000;
pub const DELIVERY_CLOCK_INTERVALS: usize = 64;

/// One bounded telemetry record, also constructible by a headless device.
#[derive(Debug, Clone, Copy)]
pub struct DeviceReport {
    pub render: RenderReport,
    pub callback_ns: u64,
    pub playback_ns: u64,
    /// Timestamp validation and kernel rendering, excluding telemetry publish
    /// and CPAL/CoreAudio work outside the closure. Not a callback deadline.
    pub render_cost_ns: u64,
}

impl DeviceReport {
    /// Map only this submitted content prefix. Match its stream/generation
    /// before calling; silent padding and future/past intervals return None.
    pub fn sample_at(&self, stream_clock_ns: u64) -> Option<i64> {
        sample_at(
            self.render.first_sample?,
            self.render.rendered_frames,
            self.playback_ns,
            stream_clock_ns,
        )
    }
}

fn sample_at(start: i64, frames: usize, playback_ns: u64, now_ns: u64) -> Option<i64> {
    let elapsed = now_ns.checked_sub(playback_ns)?;
    let offset = u128::from(elapsed) * u128::from(SAMPLE_RATE) / NANOS_PER_SECOND;
    if offset >= frames as u128 {
        return None;
    }
    start.checked_add(i64::try_from(offset).ok()?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockPosition {
    /// No content has reached its first scheduled playback instant yet.
    Pending,
    Content {
        sample: i64,
    },
    /// No retained submitted interval covers this instant. This is never a
    /// license to continue the picture clock with a wall-clock estimate.
    Gap {
        last_sample: Option<i64>,
    },
    /// The final submitted prefix has reached its scheduled end. end_sample
    /// is an exclusive content boundary, including for a starved generation.
    Terminal {
        status: RenderStatus,
        end_sample: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClockError {
    #[error("delivery clock needs a nonnegative ordered sample interval")]
    InvalidRange,
    #[error("device report belongs to another channel or generation")]
    ForeignGeneration,
    #[error("device report has an invalid content prefix")]
    InvalidReport,
    #[error("device report timestamps are not monotonic")]
    NonMonotonicTimestamp,
    #[error("device report playback intervals overlap")]
    OverlappingReports,
    #[error("expected submitted sample {expected}, received {actual}")]
    DiscontinuousContent { expected: i64, actual: i64 },
    #[error("device content exceeds the selected playback interval")]
    BeyondEnd,
    #[error("device ended before the selected playback boundary")]
    PrematureEnd,
    #[error("device resumed content after its terminal report")]
    ContentAfterTerminal,
    #[error("device playback deadline overflows the stream clock")]
    TimestampOverflow,
    #[error("device output has faulted")]
    OutputFault,
}

#[derive(Debug, Clone, Copy)]
struct Interval {
    first: i64,
    frames: usize,
    playback_ns: u64,
    end_ns: u64,
}

#[derive(Debug, Clone, Copy)]
struct Terminal {
    status: RenderStatus,
    sample: i64,
    deadline_ns: u64,
}

/// One generation's exact submitted sample coverage. Retains past and future
/// intervals in fixed storage, because the newest callback commonly describes
/// audio scheduled ahead of clock_now_ns(). Invalid observations leave the
/// clock unchanged; the controller must handle the error and stop/rebuild.
#[derive(Debug)]
pub struct DeliveryClock {
    generation: Generation,
    end: i64,
    next: i64,
    previous: Option<(u64, u64, usize)>,
    intervals: [Option<Interval>; DELIVERY_CLOCK_INTERVALS],
    write: usize,
    len: usize,
    first_playback_ns: Option<u64>,
    terminal: Option<Terminal>,
}

impl DeliveryClock {
    pub fn new(generation: Generation, start: i64, end: i64) -> Result<Self, ClockError> {
        if start < 0 || end < start {
            return Err(ClockError::InvalidRange);
        }
        Ok(Self {
            generation,
            end,
            next: start,
            previous: None,
            intervals: [None; DELIVERY_CLOCK_INTERVALS],
            write: 0,
            len: 0,
            first_playback_ns: None,
            terminal: None,
        })
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn observe(&mut self, report: DeviceReport) -> Result<(), ClockError> {
        let render = report.render;
        if render.generation != self.generation {
            return Err(ClockError::ForeignGeneration);
        }
        if render.status == RenderStatus::Fault {
            return Err(ClockError::OutputFault);
        }
        let frames = render
            .rendered_frames
            .checked_add(render.silent_frames)
            .filter(|frames| *frames <= MAX_CALLBACK_FRAMES)
            .ok_or(ClockError::InvalidReport)?;
        if (render.rendered_frames == 0) != render.first_sample.is_none()
            || (render.status == RenderStatus::Paused && render.rendered_frames != 0)
        {
            return Err(ClockError::InvalidReport);
        }
        if report.playback_ns < report.callback_ns {
            return Err(ClockError::NonMonotonicTimestamp);
        }
        if let Some((callback, playback, previous_frames)) = self.previous {
            if report.callback_ns < callback || report.playback_ns < playback {
                return Err(ClockError::NonMonotonicTimestamp);
            }
            // Host playback estimates have sub-sample jitter (also present in
            // retained hardware qualification). Admit at most one sample of
            // overlap, choosing the newer interval in position(). Larger clock
            // contradictions are not safe to turn into a picture position.
            let elapsed = u128::from(report.playback_ns - playback) * u128::from(SAMPLE_RATE);
            if elapsed + NANOS_PER_SECOND < previous_frames as u128 * NANOS_PER_SECOND {
                return Err(ClockError::OverlappingReports);
            }
        }
        if self.terminal.is_some()
            && (render.rendered_frames != 0 || render.status == RenderStatus::Playing)
        {
            return Err(ClockError::ContentAfterTerminal);
        }
        let mut next = self.next;
        let interval = if let Some(first) = render.first_sample {
            if first != next {
                return Err(ClockError::DiscontinuousContent {
                    expected: next,
                    actual: first,
                });
            }
            next = first
                .checked_add(
                    i64::try_from(render.rendered_frames).map_err(|_| ClockError::BeyondEnd)?,
                )
                .filter(|next| *next <= self.end)
                .ok_or(ClockError::BeyondEnd)?;
            Some(Interval {
                first,
                frames: render.rendered_frames,
                playback_ns: report.playback_ns,
                end_ns: deadline(report.playback_ns, render.rendered_frames)?,
            })
        } else {
            None
        };
        let terminal = match render.status {
            RenderStatus::Ended | RenderStatus::Starved => {
                if render.status == RenderStatus::Ended && next != self.end {
                    return Err(ClockError::PrematureEnd);
                }
                if self
                    .terminal
                    .is_some_and(|terminal| terminal.status != render.status)
                {
                    return Err(ClockError::InvalidReport);
                }
                // Sub-sample timestamp overlap is permitted above, but an
                // empty terminal callback cannot shorten a previously
                // submitted content prefix's scheduled playback interval.
                let previous_content_end = self.intervals
                    [(self.write + DELIVERY_CLOCK_INTERVALS - 1) % DELIVERY_CLOCK_INTERVALS]
                    .map_or(0, |interval| interval.end_ns);
                Some(Terminal {
                    status: render.status,
                    sample: next,
                    deadline_ns: deadline(report.playback_ns, render.rendered_frames)?
                        .max(previous_content_end),
                })
            }
            _ => None,
        };
        // Publish only after every coordinate, status and timestamp check.
        self.previous = Some((report.callback_ns, report.playback_ns, frames));
        self.next = next;
        if let Some(interval) = interval {
            self.first_playback_ns.get_or_insert(interval.playback_ns);
            self.intervals[self.write] = Some(interval);
            self.write = (self.write + 1) % DELIVERY_CLOCK_INTERVALS;
            self.len = (self.len + 1).min(DELIVERY_CLOCK_INTERVALS);
        }
        if self.terminal.is_none() {
            self.terminal = terminal;
        }
        Ok(())
    }

    pub fn position(&self, now_ns: u64) -> ClockPosition {
        if let Some(terminal) = self.terminal
            && now_ns >= terminal.deadline_ns
        {
            return ClockPosition::Terminal {
                status: terminal.status,
                end_sample: terminal.sample,
            };
        }
        let mut last_sample = None;
        for offset in 0..self.len {
            let index =
                (self.write + DELIVERY_CLOCK_INTERVALS - offset - 1) % DELIVERY_CLOCK_INTERVALS;
            let interval = self.intervals[index].expect("initialized clock interval");
            if let Some(sample) = sample_at(
                interval.first,
                interval.frames,
                interval.playback_ns,
                now_ns,
            ) {
                return ClockPosition::Content { sample };
            }
            if last_sample.is_none() && now_ns >= interval.end_ns {
                last_sample = interval.first.checked_add(interval.frames as i64 - 1);
            }
        }
        if self.first_playback_ns.is_none_or(|first| now_ns < first) {
            ClockPosition::Pending
        } else {
            ClockPosition::Gap { last_sample }
        }
    }
}

fn deadline(playback_ns: u64, frames: usize) -> Result<u64, ClockError> {
    let duration = (frames as u128 * NANOS_PER_SECOND).div_ceil(u128::from(SAMPLE_RATE));
    playback_ns
        .checked_add(u64::try_from(duration).map_err(|_| ClockError::TimestampOverflow)?)
        .ok_or(ClockError::TimestampOverflow)
}
