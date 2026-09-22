//! A bounded prepared-PCM handoff. No decoding, DSP, allocation, locking or I/O
//! occurs in `Callback::render`. Construction and submission belong elsewhere.
//!
//! Generation changes take effect at callback boundaries. A final control check
//! silences a buffer invalidated during rendering, but a seek cannot retract a
//! buffer already handed to the device. This kernel is not application transport.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

pub const PACKET_FRAMES: usize = 256;
pub const QUEUE_PACKETS: usize = 32;
pub const MAX_CALLBACK_FRAMES: usize = 8192;
const MAX_GENERATION: u64 = u64::MAX >> 1;
static LAST_CHANNEL: AtomicU64 = AtomicU64::new(0);

/// Never reused within one channel, including pause transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation {
    channel: u64,
    serial: u64,
}

impl Generation {
    pub const fn get(self) -> u64 {
        self.serial
    }

    pub const fn channel_id(self) -> u64 {
        self.channel
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStatus {
    Paused,
    Playing,
    Starved,
    Ended,
    Fault,
}

/// Positions describe only the content actually retained in this buffer.
/// Waiting for bounded stale-packet cleanup reports `Playing` with zero content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderReport {
    pub generation: Generation,
    pub status: RenderStatus,
    pub first_sample: Option<i64>,
    pub rendered_frames: usize,
    pub silent_frames: usize,
    pub discarded_packets: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FeedError {
    #[error("audio packet queue is full")]
    Full,
    #[error("audio generation is stale")]
    StaleGeneration,
    #[error("audio output is paused")]
    Paused,
    #[error("audio generation has already ended")]
    Ended,
    #[error("audio preparation contains neither PCM nor an explicit end")]
    EmptyPreparation,
    #[error("audio generation is already active; restart to reset its clock")]
    AlreadyActive,
    #[error("audio output has a permanent fault")]
    Fault,
    #[error("audio packets require 1 through 256 stereo frames")]
    InvalidFrames,
    #[error("audio samples must be finite and within -1 through 1")]
    InvalidSamples,
    #[error("audio start sample must be nonnegative")]
    InvalidStart,
    #[error("audio sample cursor would overflow")]
    SampleOverflow,
    #[error("audio generation identities are exhausted")]
    GenerationExhausted,
    #[error("audio channel identities are exhausted")]
    ChannelIdentitiesExhausted,
}

struct Shared {
    channel: u64,
    // One publication carries both identity and playback state. Bit zero is
    // playing; all higher bits are the monotonically increasing generation.
    control: AtomicU64,
    fault: AtomicU8,
}

/// Device errors are permanent for this channel. Recovery creates a new channel.
#[derive(Clone)]
pub struct FaultSignal(Arc<Shared>);

impl FaultSignal {
    pub fn raise(&self) {
        self.0.fault.store(1, Ordering::Release);
    }

    pub fn is_faulted(&self) -> bool {
        self.0.fault.load(Ordering::Acquire) != 0
    }
}

#[derive(Clone, Copy)]
enum PacketKind {
    Pcm,
    End,
}

// All slots have the same fixed storage, including EOS. There is no boxed
// variant whose destruction could deallocate on the callback thread.
#[derive(Clone, Copy)]
struct Packet {
    kind: PacketKind,
    generation: Generation,
    position: i64,
    frames: usize,
    samples: [[f32; 2]; PACKET_FRAMES],
}

impl Packet {
    fn generation(&self) -> Generation {
        self.generation
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FeedState {
    Paused,
    Preparing,
    PreparedEnd,
    Playing,
    Ended,
}

/// One producer owns generation allocation and the contiguous project cursor.
pub struct Feed {
    producer: Producer<Packet>,
    shared: Arc<Shared>,
    generation: Generation,
    next: i64,
    state: FeedState,
    submitted: bool,
}

impl Feed {
    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn next_sample(&self) -> i64 {
        self.next
    }

    pub fn fault_signal(&self) -> FaultSignal {
        FaultSignal(Arc::clone(&self.shared))
    }

    /// Prepare a new generation without starting its clock. Existing queued
    /// packets occupy slots until the paused callback drains them. Submit the
    /// desired prefill, then call `activate`; callbacks during preparation stay
    /// silent without latching starvation.
    pub fn restart(&mut self, start: i64) -> Result<Generation, FeedError> {
        if start < 0 {
            return Err(FeedError::InvalidStart);
        }
        let next = self.next_generation()?;
        self.next = start;
        self.generation = next;
        self.state = FeedState::Preparing;
        self.submitted = false;
        self.shared
            .control
            .store(encode(next, false), Ordering::Release);
        Ok(next)
    }

    /// Publish the prepared generation only after at least one PCM packet or
    /// explicit EOS. Repeated activation cannot revive a starved generation.
    pub fn activate(&mut self, generation: Generation) -> Result<(), FeedError> {
        self.check_generation(generation)?;
        self.state = match self.state {
            FeedState::Preparing if self.submitted => FeedState::Playing,
            FeedState::Preparing => return Err(FeedError::EmptyPreparation),
            FeedState::PreparedEnd => FeedState::Ended,
            FeedState::Paused => return Err(FeedError::Paused),
            FeedState::Playing | FeedState::Ended => return Err(FeedError::AlreadyActive),
        };
        self.shared
            .control
            .store(encode(generation, true), Ordering::Release);
        Ok(())
    }

    /// Pausing also consumes an identity, so late packets can never resume it.
    pub fn pause(&mut self) -> Result<Generation, FeedError> {
        let next = self.next_generation()?;
        self.generation = next;
        self.state = FeedState::Paused;
        self.shared
            .control
            .store(encode(next, false), Ordering::Release);
        Ok(next)
    }

    fn next_generation(&self) -> Result<Generation, FeedError> {
        if self.shared.fault.load(Ordering::Acquire) != 0 {
            return Err(FeedError::Fault);
        }
        self.generation
            .serial
            .checked_add(1)
            .filter(|next| *next <= MAX_GENERATION)
            .map(|serial| Generation {
                channel: self.generation.channel,
                serial,
            })
            .ok_or(FeedError::GenerationExhausted)
    }

    fn check_generation(&self, generation: Generation) -> Result<(), FeedError> {
        if self.shared.fault.load(Ordering::Acquire) != 0 {
            return Err(FeedError::Fault);
        }
        if generation != self.generation {
            return Err(FeedError::StaleGeneration);
        }
        Ok(())
    }

    fn admit(&self, generation: Generation) -> Result<(), FeedError> {
        self.check_generation(generation)?;
        match self.state {
            FeedState::Paused => Err(FeedError::Paused),
            FeedState::PreparedEnd | FeedState::Ended => Err(FeedError::Ended),
            FeedState::Preparing | FeedState::Playing => Ok(()),
        }
    }

    /// Errors never advance the cursor or publish a partial packet. Admission
    /// preserves submitted levels; it does not normalize or limit the signal.
    pub fn submit(
        &mut self,
        generation: Generation,
        samples: &[[f32; 2]],
    ) -> Result<(), FeedError> {
        self.admit(generation)?;
        if samples.is_empty() || samples.len() > PACKET_FRAMES {
            return Err(FeedError::InvalidFrames);
        }
        if samples
            .iter()
            .flatten()
            .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err(FeedError::InvalidSamples);
        }
        let next = self
            .next
            .checked_add(samples.len() as i64)
            .ok_or(FeedError::SampleOverflow)?;
        let mut packet_samples = [[0.0; 2]; PACKET_FRAMES];
        packet_samples[..samples.len()].copy_from_slice(samples);
        let packet = Packet {
            kind: PacketKind::Pcm,
            generation,
            position: self.next,
            frames: samples.len(),
            samples: packet_samples,
        };
        if self.producer.push(packet).is_err() {
            return Err(FeedError::Full);
        }
        self.next = next;
        self.submitted = true;
        Ok(())
    }

    /// EOS occupies a queue slot and is ordered after the accepted PCM.
    pub fn finish(&mut self, generation: Generation) -> Result<(), FeedError> {
        self.admit(generation)?;
        if self
            .producer
            .push(Packet {
                kind: PacketKind::End,
                generation,
                position: self.next,
                frames: 0,
                samples: [[0.0; 2]; PACKET_FRAMES],
            })
            .is_err()
        {
            return Err(FeedError::Full);
        }
        self.state = if self.state == FeedState::Preparing {
            FeedState::PreparedEnd
        } else {
            FeedState::Ended
        };
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Pending {
    packet: Packet,
    offset: usize,
}

/// The sole consumer. Keep this object on the device callback's owner thread.
pub struct Callback {
    consumer: Consumer<Packet>,
    shared: Arc<Shared>,
    generation: Generation,
    playing: bool,
    expected: Option<i64>,
    pending: Option<Pending>,
    status: RenderStatus,
}

/// Allocates the bounded ring and shared state; initial generation zero is paused.
pub fn channel() -> Result<(Feed, Callback), FeedError> {
    let channel = allocate_channel(&LAST_CHANNEL)?;
    let generation = Generation { channel, serial: 0 };
    let (producer, consumer) = RingBuffer::new(QUEUE_PACKETS);
    let shared = Arc::new(Shared {
        channel,
        control: AtomicU64::new(0),
        fault: AtomicU8::new(0),
    });
    Ok((
        Feed {
            producer,
            shared: Arc::clone(&shared),
            generation,
            next: 0,
            state: FeedState::Paused,
            submitted: false,
        },
        Callback {
            consumer,
            shared,
            generation,
            playing: false,
            expected: None,
            pending: None,
            status: RenderStatus::Paused,
        },
    ))
}

fn allocate_channel(last: &AtomicU64) -> Result<u64, FeedError> {
    last.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| {
        last.checked_add(1)
    })
    .map(|last| last + 1)
    .map_err(|_| FeedError::ChannelIdentitiesExhausted)
}

fn encode(generation: Generation, playing: bool) -> u64 {
    (generation.serial << 1) | u64::from(playing)
}

fn decode(control: u64, channel: u64) -> (Generation, bool) {
    (
        Generation {
            channel,
            serial: control >> 1,
        },
        control & 1 != 0,
    )
}

impl Callback {
    fn fault(&mut self) {
        self.shared.fault.store(1, Ordering::Release);
        self.status = RenderStatus::Fault;
    }

    /// Writes stereo interleaved f32. A short queue produces a silent suffix and
    /// latches starvation until an explicit restart. Late refill cannot resume a
    /// stale device clock. No locks, allocation, logging, file access or DSP.
    pub fn render(&mut self, output: &mut [f32]) -> RenderReport {
        output.fill(0.0);
        let frames = output.len() / 2;
        let control = self.shared.control.load(Ordering::Acquire);
        let (generation, playing) = decode(control, self.shared.channel);
        let mut discarded = 0;
        if generation != self.generation || playing != self.playing {
            self.generation = generation;
            self.playing = playing;
            self.expected = None;
            self.status = if playing {
                RenderStatus::Playing
            } else {
                RenderStatus::Paused
            };
            if self
                .pending
                .is_some_and(|pending| pending.packet.generation() < generation)
            {
                self.pending = None;
                discarded += 1;
            }
        }
        if !output.len().is_multiple_of(2) || frames > MAX_CALLBACK_FRAMES {
            self.fault();
        }
        if self.shared.fault.load(Ordering::Acquire) != 0 {
            self.status = RenderStatus::Fault;
        }
        let mut report = RenderReport {
            generation,
            status: self.status,
            first_sample: None,
            rendered_frames: 0,
            silent_frames: frames,
            discarded_packets: discarded,
        };
        if self.status == RenderStatus::Playing || self.status == RenderStatus::Paused {
            self.fill(output, &mut report);
        }
        self.finish_render(output, control, report)
    }

    fn finish_render(
        &mut self,
        output: &mut [f32],
        control: u64,
        mut report: RenderReport,
    ) -> RenderReport {
        let frames = output.len() / 2;
        let final_control = self.shared.control.load(Ordering::Acquire);
        let faulted = self.shared.fault.load(Ordering::Acquire) != 0;
        if final_control != control || faulted {
            // Packets consumed before invalidation stay consumed. In particular,
            // do not rewind an old partial packet or replay a submitted buffer.
            output.fill(0.0);
            let (generation, playing) = decode(final_control, self.shared.channel);
            report.generation = generation;
            report.status = if faulted {
                RenderStatus::Fault
            } else if playing {
                RenderStatus::Playing
            } else {
                RenderStatus::Paused
            };
            report.first_sample = None;
            report.rendered_frames = 0;
            report.silent_frames = frames;
        } else {
            report.status = self.status;
            report.silent_frames = frames - report.rendered_frames;
        }
        report
    }

    fn fill(&mut self, output: &mut [f32], report: &mut RenderReport) {
        while report.rendered_frames < output.len() / 2 || !self.playing {
            if self.pending.is_none() {
                // Peeking limits stale cleanup even when a producer refills the
                // queue concurrently. Matching/future packets are not discarded.
                if let Ok(packet) = self.consumer.peek() {
                    if packet.generation() < self.generation
                        && report.discarded_packets >= QUEUE_PACKETS
                    {
                        break;
                    }
                    if !self.playing && packet.generation() >= self.generation {
                        break;
                    }
                }
                match self.consumer.pop() {
                    Ok(packet) => self.pending = Some(Pending { packet, offset: 0 }),
                    Err(_) => {
                        if self.playing {
                            self.status = RenderStatus::Starved;
                        }
                        break;
                    }
                }
            }
            let Some(pending) = self.pending else {
                break;
            };
            match pending.packet.generation().cmp(&self.generation) {
                std::cmp::Ordering::Less => {
                    self.pending = None;
                    report.discarded_packets += 1;
                    continue;
                }
                // Control may have changed after the initial acquire. Retain a
                // future packet intact until the matching boundary is observed.
                std::cmp::Ordering::Greater => break,
                std::cmp::Ordering::Equal => {}
            }
            if !self.playing {
                break;
            }
            match pending.packet.kind {
                PacketKind::End => {
                    let next = pending.packet.position;
                    self.pending = None;
                    if next < 0 || self.expected.is_some_and(|expected| expected != next) {
                        self.fault();
                    } else {
                        self.status = RenderStatus::Ended;
                    }
                    break;
                }
                PacketKind::Pcm => {
                    let start = pending.packet.position;
                    let frames = pending.packet.frames;
                    let samples = &pending.packet.samples;
                    let at = start.checked_add(pending.offset as i64);
                    if start < 0
                        || frames == 0
                        || frames > PACKET_FRAMES
                        || pending.offset >= frames
                        || at.is_none()
                        || self.expected.is_some_and(|expected| Some(expected) != at)
                    {
                        self.fault();
                        break;
                    }
                    let count =
                        (frames - pending.offset).min(output.len() / 2 - report.rendered_frames);
                    let next = at.and_then(|at| at.checked_add(count as i64));
                    let Some(next) = next else {
                        self.fault();
                        break;
                    };
                    for (index, frame) in samples[pending.offset..pending.offset + count]
                        .iter()
                        .enumerate()
                    {
                        let offset = (report.rendered_frames + index) * 2;
                        output[offset..offset + 2].copy_from_slice(frame);
                    }
                    if report.first_sample.is_none() {
                        report.first_sample = at;
                    }
                    report.rendered_frames += count;
                    self.expected = Some(next);
                    self.pending = (pending.offset + count < frames).then_some(Pending {
                        packet: pending.packet,
                        offset: pending.offset + count,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_identity_exhaustion_never_reuses_a_scope() {
        let last = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate_channel(&last), Ok(u64::MAX));
        assert_eq!(
            allocate_channel(&last),
            Err(FeedError::ChannelIdentitiesExhausted)
        );
        assert_eq!(last.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn generation_exhaustion_never_wraps_or_changes_control() {
        let (mut feed, _) = channel().unwrap();
        feed.generation.serial = MAX_GENERATION;
        feed.shared
            .control
            .store(encode(feed.generation, false), Ordering::Release);
        let before = feed.shared.control.load(Ordering::Acquire);
        assert_eq!(feed.restart(20), Err(FeedError::GenerationExhausted));
        assert_eq!(feed.pause(), Err(FeedError::GenerationExhausted));
        assert_eq!(feed.shared.control.load(Ordering::Acquire), before);
        assert_eq!(feed.next_sample(), 0);
    }

    #[test]
    fn discontinuous_packet_faults_and_silences_the_whole_buffer() {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(10).unwrap();
        feed.submit(generation, &[[0.2, -0.2]]).unwrap();
        feed.producer
            .push(Packet {
                kind: PacketKind::Pcm,
                generation,
                position: 20,
                frames: 1,
                samples: [[0.4; 2]; PACKET_FRAMES],
            })
            .ok()
            .unwrap();
        feed.activate(generation).unwrap();
        let mut output = [0.9; 4];
        let report = callback.render(&mut output);
        assert_eq!(report.status, RenderStatus::Fault);
        assert_eq!(output, [0.0; 4]);
        assert_eq!(report.first_sample, None);
        assert_eq!(report.rendered_frames, 0);
        assert!(feed.fault_signal().is_faulted());
    }

    #[test]
    fn newer_packet_is_retained_until_its_control_boundary() {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(10).unwrap();
        // Model a restart arriving after a callback captured the older word.
        callback.generation = Generation {
            serial: generation.serial - 1,
            ..generation
        };
        callback.playing = true;
        callback.status = RenderStatus::Playing;
        feed.submit(generation, &[[0.3, -0.3]; 2]).unwrap();
        feed.activate(generation).unwrap();
        let mut output = [0.0; 4];
        let mut report = RenderReport {
            generation: callback.generation,
            status: RenderStatus::Playing,
            first_sample: None,
            rendered_frames: 0,
            silent_frames: 2,
            discarded_packets: 0,
        };
        callback.fill(&mut output, &mut report);
        assert_eq!(report.rendered_frames, 0);
        assert_eq!(report.discarded_packets, 0);
        assert_eq!(callback.pending.unwrap().packet.generation(), generation);
        assert_eq!(callback.render(&mut output).first_sample, Some(10));
        assert_eq!(output, [0.3, -0.3, 0.3, -0.3]);
    }

    #[test]
    fn control_change_after_fill_erases_old_content_and_never_replays_it() {
        let (mut feed, mut callback) = channel().unwrap();
        let old = feed.restart(0).unwrap();
        feed.submit(old, &[[0.2; 2]; 5]).unwrap();
        feed.activate(old).unwrap();
        let mut report = callback.render(&mut []);
        let control = feed.shared.control.load(Ordering::Acquire);
        let mut output = [0.0; 4];
        callback.fill(&mut output, &mut report);
        assert_eq!(report.rendered_frames, 2);
        let next = feed.restart(100).unwrap();
        feed.submit(next, &[[0.8, -0.8]; 2]).unwrap();
        feed.activate(next).unwrap();
        let invalidated = callback.finish_render(&mut output, control, report);
        assert_eq!(invalidated.generation, next);
        assert_eq!(invalidated.rendered_frames, 0);
        assert_eq!(invalidated.first_sample, None);
        assert_eq!(output, [0.0; 4]);
        let current = callback.render(&mut output);
        assert_eq!(current.first_sample, Some(100));
        assert_eq!(current.discarded_packets, 1);
        assert_eq!(output, [0.8, -0.8, 0.8, -0.8]);
    }

    #[test]
    fn permanent_fault_after_fill_erases_newly_written_pcm() {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(0).unwrap();
        feed.submit(generation, &[[0.4; 2]; 2]).unwrap();
        feed.activate(generation).unwrap();
        let mut report = callback.render(&mut []);
        let control = feed.shared.control.load(Ordering::Acquire);
        let mut output = [0.0; 4];
        callback.fill(&mut output, &mut report);
        assert_eq!(output, [0.4; 4]);
        feed.fault_signal().raise();
        let invalidated = callback.finish_render(&mut output, control, report);
        assert_eq!(invalidated.status, RenderStatus::Fault);
        assert_eq!(invalidated.rendered_frames, 0);
        assert_eq!(invalidated.first_sample, None);
        assert_eq!(output, [0.0; 4]);
    }

    #[test]
    fn discontinuous_end_marker_faults_instead_of_reporting_a_clean_end() {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(20).unwrap();
        feed.submit(generation, &[[0.3; 2]]).unwrap();
        feed.producer
            .push(Packet {
                kind: PacketKind::End,
                generation,
                position: 22,
                frames: 0,
                samples: [[0.0; 2]; PACKET_FRAMES],
            })
            .ok()
            .unwrap();
        feed.activate(generation).unwrap();
        let mut output = [0.0; 4];
        assert_eq!(callback.render(&mut output).status, RenderStatus::Fault);
        assert_eq!(output, [0.0; 4]);
    }
}
