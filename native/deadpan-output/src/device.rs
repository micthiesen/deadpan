use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

use cpal::{
    SampleFormat, SupportedBufferSize,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use crate::{Feed, RenderReport, SAMPLE_RATE, channel};

const REPORT_CAPACITY: usize = 256;

/// A read-only inventory of the default output configuration, before opening it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
    pub buffer_range: Option<(u32, u32)>,
}

fn describe(device: &cpal::Device) -> Result<DeviceInfo, DeviceError> {
    let config = device.default_output_config()?;
    Ok(DeviceInfo {
        id: device.id()?.to_string(),
        name: device.description()?.name().to_owned(),
        sample_rate: config.sample_rate(),
        channels: config.channels(),
        sample_format: config.sample_format().to_string(),
        buffer_range: match config.buffer_size() {
            SupportedBufferSize::Range { min, max } => Some((*min, *max)),
            SupportedBufferSize::Unknown => None,
        },
    })
}

pub fn default_device_info() -> Result<DeviceInfo, DeviceError> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or(DeviceError::Unavailable)?;
    describe(&device)
}

/// One bounded telemetry record. These are estimates from the host's clock,
/// not loopback measurements of the sound arriving at a speaker.
#[derive(Debug, Clone, Copy)]
pub struct DeviceReport {
    pub render: RenderReport,
    pub callback_ns: u64,
    pub playback_ns: u64,
    /// Timestamp validation and kernel rendering, excluding telemetry publish
    /// and CPAL/CoreAudio work outside our closure. Not a full callback deadline.
    pub render_cost_ns: u64,
}

impl DeviceReport {
    /// Map the stream clock to content only inside this submitted PCM prefix.
    /// Never extrapolate across starvation, silence, another stream or seek.
    /// Callers must first match this report's generation to their active one.
    pub fn sample_at(&self, stream_clock_ns: u64) -> Option<i64> {
        let elapsed = stream_clock_ns.checked_sub(self.playback_ns)?;
        let offset = u128::from(elapsed) * u128::from(SAMPLE_RATE) / 1_000_000_000;
        if offset >= self.render.rendered_frames as u128 {
            return None;
        }
        self.render
            .first_sample?
            .checked_add(i64::try_from(offset).ok()?)
    }
}

/// One explicitly bound device. All methods other than its internal render
/// closure belong to the controller/preparation thread. Drop on that thread.
///
/// Opening pins the current default by its stable ID; CPAL's automatic default
/// rerouting is deliberately not used. The controller must call `check_route`
/// periodically and after system lifecycle notifications, and reconstruct on
/// failure. Full native notification/transport recovery remains app work.
pub struct DeviceOutput {
    stream: cpal::Stream,
    feed: Feed,
    reports: rtrb::Consumer<DeviceReport>,
    dropped_reports: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
    info: DeviceInfo,
}

impl DeviceOutput {
    /// Admit only an already active 48 kHz stereo float configuration. No
    /// requested buffer-size change, custom device rate, or implicit downmix.
    /// The stream remains paused until `start_device`, allowing prefill first.
    pub fn open_default() -> Result<Self, DeviceError> {
        let host = cpal::default_host();
        let default = host
            .default_output_device()
            .ok_or(DeviceError::Unavailable)?;
        let id = default.id()?;
        let device = host.device_by_id(&id).ok_or(DeviceError::Unavailable)?;
        let info = describe(&device)?;
        let config = device.default_output_config()?;
        if config.sample_rate() != SAMPLE_RATE
            || config.channels() != 2
            || config.sample_format() != SampleFormat::F32
        {
            return Err(DeviceError::Unsupported(info));
        }
        let (feed, mut callback) = channel()?;
        let callback_fault = feed.fault_signal();
        let error_fault = callback_fault.clone();
        let (mut reports, consumer) = rtrb::RingBuffer::new(REPORT_CAPACITY);
        let dropped_reports = Arc::new(AtomicU64::new(0));
        let dropped = dropped_reports.clone();
        let errors = Arc::new(AtomicU64::new(0));
        let callback_errors = errors.clone();
        let stream = device.build_output_stream(
            config.config(),
            move |output: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let started = Instant::now();
                let timestamp = info.timestamp();
                let clocks = u64::try_from(timestamp.callback.as_nanos())
                    .ok()
                    .zip(u64::try_from(timestamp.playback.as_nanos()).ok())
                    .filter(|(callback, playback)| playback >= callback);
                if clocks.is_none() {
                    callback_fault.raise();
                }
                let render = callback.render(output);
                let (callback_ns, playback_ns) = clocks.unwrap_or_default();
                let report = DeviceReport {
                    render,
                    callback_ns,
                    playback_ns,
                    render_cost_ns: u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                };
                if reports.push(report).is_err() {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            },
            move |error| {
                // No formatting, logging, allocation or queue contention here.
                // Fail closed for every backend notification, including xruns.
                callback_errors.fetch_or(error_bit(error.kind()), Ordering::Relaxed);
                error_fault.raise();
            },
            Some(std::time::Duration::from_secs(2)),
        )?;
        let output = Self {
            stream,
            feed,
            reports: consumer,
            dropped_reports,
            errors,
            info,
        };
        output.check_route()?;
        Ok(output)
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }
    pub fn feed(&mut self) -> &mut Feed {
        &mut self.feed
    }
    pub fn pop_report(&mut self) -> Option<DeviceReport> {
        self.reports.pop().ok()
    }
    pub fn dropped_reports(&self) -> u64 {
        self.dropped_reports.load(Ordering::Relaxed)
    }
    /// Bit set per backend category; a nonzero value permanently faults this stream.
    pub fn error_flags(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }
    pub fn buffer_frames(&self) -> Result<u32, DeviceError> {
        Ok(self.stream.buffer_size()?)
    }
    pub fn clock_now_ns(&self) -> Option<u64> {
        u64::try_from(self.stream.now().as_nanos()).ok()
    }

    /// Run device callbacks, which remain muted until the Feed is activated.
    /// After a native pause, call `Feed::restart`, then this method BEFORE
    /// refilling. Old packets can fill the ring and only callbacks drain them.
    /// Retry producer backpressure off the callback, then activate after prefill.
    pub fn start_device(&mut self) -> Result<(), DeviceError> {
        self.check_route()?;
        if self.feed.fault_signal().is_faulted() {
            return Err(DeviceError::Faulted);
        }
        if let Err(error) = self.stream.play() {
            self.feed.fault_signal().raise();
            return Err(error.into());
        }
        Ok(())
    }

    /// Invalidate prepared audio before stopping callbacks. Restart requires a
    /// new Feed generation and freshly prepared PCM, even on the same stream.
    pub fn pause_device(&mut self) -> Result<(), DeviceError> {
        // A faulted/exhausted feed cannot allocate another generation, but the
        // hardware still must stop. Invalidation errors cannot skip that call.
        let invalidate = self.feed.pause();
        if let Err(error) = self.stream.pause() {
            self.feed.fault_signal().raise();
            return Err(error.into());
        }
        invalidate?;
        Ok(())
    }

    pub fn check_route(&self) -> Result<(), DeviceError> {
        if self.feed.fault_signal().is_faulted() {
            return Err(DeviceError::Faulted);
        }
        let result = default_device_info();
        if result.as_ref().is_ok_and(|current| {
            current.id == self.info.id
                && current.sample_rate == self.info.sample_rate
                && current.channels == self.info.channels
                && current.sample_format == self.info.sample_format
        }) {
            return if self.feed.fault_signal().is_faulted() {
                Err(DeviceError::Faulted)
            } else {
                Ok(())
            };
        }
        self.feed.fault_signal().raise();
        match result {
            Ok(_) => Err(DeviceError::RouteChanged),
            Err(error) => Err(error),
        }
    }
}

impl Drop for DeviceOutput {
    fn drop(&mut self) {
        // CPAL monitor delivery threads may briefly hold the underlying unit.
        // Explicitly stop it on the controller thread before releasing ownership.
        let _ = self.feed.pause();
        let _ = self.stream.pause();
    }
}

fn error_bit(kind: cpal::ErrorKind) -> u64 {
    use cpal::ErrorKind;
    match kind {
        ErrorKind::DeviceChanged => 1,
        ErrorKind::DeviceNotAvailable => 2,
        ErrorKind::StreamInvalidated => 4,
        ErrorKind::Xrun => 8,
        ErrorKind::RealtimeDenied => 16,
        _ => 32,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("default audio output device is unavailable")]
    Unavailable,
    #[error("output currently requires an existing 48 kHz stereo float device: {0:?}")]
    Unsupported(DeviceInfo),
    #[error("default audio route or format changed; rebuild with a new transport generation")]
    RouteChanged,
    #[error("audio output faulted; recreate the device boundary")]
    Faulted,
    #[error(transparent)]
    Feed(#[from] crate::FeedError),
    #[error(transparent)]
    Backend(#[from] cpal::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_never_extrapolates_beyond_submitted_content() {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(17).unwrap();
        feed.submit(generation, &[[0.1, -0.1]; 3]).unwrap();
        feed.finish(generation).unwrap();
        feed.activate(generation).unwrap();
        let report = DeviceReport {
            render: callback.render(&mut [0.0; 16]),
            callback_ns: 10,
            playback_ns: 100,
            render_cost_ns: 1,
        };
        assert_eq!(report.sample_at(99), None);
        assert_eq!(report.sample_at(100), Some(17));
        assert_eq!(report.sample_at(20_934), Some(18));
        assert_eq!(report.sample_at(62_599), Some(19));
        assert_eq!(report.sample_at(62_600), None);
        assert_eq!(report.sample_at(u64::MAX), None);
    }
}
