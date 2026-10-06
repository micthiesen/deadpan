//! Process-local performance counters for specification Section 25.3.
//!
//! Every counter is a fixed static of relaxed atomics: recording never
//! allocates, locks or performs I/O, and a snapshot is a set of plain loads.
//! Nothing here enters authored state, history or the project database, and
//! values describe only the current process. A CLI process sees the store,
//! media and cache work it performs itself, never a running app's sessions.
//!
//! Recording sites are deliberately narrow (see docs/PERFORMANCE.md):
//! - [`IO`]: bytes and operations at the store's revision rows, the
//!   descriptor-relative object store, media snapshot copies, native decoder
//!   descriptor reads, the private PCM cache file and preview-proxy reads.
//!   These are the callers' own read/write paths, not SQLite page or kernel
//!   block I/O.
//! - [`QUEUES`]: current and high-water depth of bounded request slots and
//!   the prepared-PCM device queue, updated by their producers.
//! - [`CACHES`]: hit/miss/eviction counts and residency of PCM caches.
//! - [`GPU`]: shared picture pipeline submissions and the latency from
//!   `queue.submit` to the observed completion callback.
//! - [`WORKERS`]: physical footprint of live supervised worker processes,
//!   sampled at low frequency by their supervisor.
//!
//! Device audio callbacks never record here.

use std::sync::atomic::{
    AtomicU32, AtomicU64,
    Ordering::{Acquire, Relaxed, Release},
};
use std::time::Duration;

/// A cumulative count.
#[derive(Debug, Default)]
pub struct Counter(AtomicU64);

impl Counter {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }
    pub fn add(&self, value: u64) {
        self.0.fetch_add(value, Relaxed);
    }
    pub fn increment(&self) {
        self.add(1);
    }
    pub fn get(&self) -> u64 {
        self.0.load(Relaxed)
    }
}

/// A current level with its process-lifetime high-water mark. Several owners
/// may contribute; each adds and later subtracts its own share.
#[derive(Debug, Default)]
pub struct Gauge {
    current: AtomicU64,
    high: AtomicU64,
}

/// One gauge observation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Level {
    pub current: u64,
    pub high: u64,
}

impl Gauge {
    pub const fn new() -> Self {
        Self {
            current: AtomicU64::new(0),
            high: AtomicU64::new(0),
        }
    }
    pub fn add(&self, value: u64) {
        let now = self.current.fetch_add(value, Relaxed).saturating_add(value);
        self.high.fetch_max(now, Relaxed);
    }
    /// Saturates at zero, so an unmatched release cannot wrap the level.
    pub fn sub(&self, value: u64) {
        let _ = self
            .current
            .fetch_update(Relaxed, Relaxed, |now| Some(now.saturating_sub(value)));
    }
    /// Overwrite the level. Only for a gauge with a single writer, such as
    /// the most recent observation of a per-process value.
    pub fn set(&self, value: u64) {
        self.current.store(value, Relaxed);
        self.high.fetch_max(value, Relaxed);
    }
    /// Replace one owner's share `previous` with `next`.
    pub fn replace(&self, previous: u64, next: u64) {
        if next > previous {
            self.add(next - previous);
        } else {
            self.sub(previous - next);
        }
    }
    pub fn level(&self) -> Level {
        Level {
            current: self.current.load(Relaxed),
            high: self.high.load(Relaxed),
        }
    }
}

/// One owner's contribution to a shared [`Gauge`], released on drop.
#[derive(Debug)]
pub struct Share {
    gauge: &'static Gauge,
    value: u64,
}

impl Share {
    pub const fn new(gauge: &'static Gauge) -> Self {
        Self { gauge, value: 0 }
    }
    pub fn set(&mut self, value: u64) {
        self.gauge.replace(self.value, value);
        self.value = value;
    }
    pub const fn value(&self) -> u64 {
        self.value
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.gauge.sub(self.value);
    }
}

/// Bytes and operations along one read/write path.
#[derive(Debug, Default)]
pub struct IoCounters {
    read_ops: Counter,
    read_bytes: Counter,
    write_ops: Counter,
    write_bytes: Counter,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoSnapshot {
    pub read_ops: u64,
    pub read_bytes: u64,
    pub write_ops: u64,
    pub write_bytes: u64,
}

impl IoCounters {
    pub const fn new() -> Self {
        Self {
            read_ops: Counter::new(),
            read_bytes: Counter::new(),
            write_ops: Counter::new(),
            write_bytes: Counter::new(),
        }
    }
    pub fn read(&self, bytes: u64) {
        self.read_ops.increment();
        self.read_bytes.add(bytes);
    }
    pub fn write(&self, bytes: u64) {
        self.write_ops.increment();
        self.write_bytes.add(bytes);
    }
    pub fn snapshot(&self) -> IoSnapshot {
        IoSnapshot {
            read_ops: self.read_ops.get(),
            read_bytes: self.read_bytes.get(),
            write_ops: self.write_ops.get(),
            write_bytes: self.write_bytes.get(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Io {
    /// Revision documents and stored patches read from or written to SQLite
    /// rows by the store (logical payload bytes, not database pages).
    pub store_revisions: IoCounters,
    /// Media/Originals and Media/Generated object copies, verification reads
    /// and private snapshot writes.
    pub objects: IoCounters,
    /// Private verified copies of source media made by deadpan-media.
    pub media_snapshots: IoCounters,
    /// Native video decoder descriptor reads (FFmpeg AVIO callbacks).
    pub decoder_input: IoCounters,
    /// Private decoded-PCM cache file: writes while indexing, reads on access.
    pub pcm_cache_file: IoCounters,
    /// Preview-proxy sidecar reads.
    pub proxy: IoCounters,
}

pub static IO: Io = Io {
    store_revisions: IoCounters::new(),
    objects: IoCounters::new(),
    media_snapshots: IoCounters::new(),
    decoder_input: IoCounters::new(),
    pcm_cache_file: IoCounters::new(),
    proxy: IoCounters::new(),
};

#[derive(Debug, Default)]
pub struct Queues {
    /// Main viewer source-preview requests: the replaceable pending slot plus
    /// the request in progress (0 through 2).
    pub picture_preview: Gauge,
    /// Card thumbnail service requests, counted the same way.
    pub thumbnails: Gauge,
    /// Prepared PCM batches waiting for the playback control worker (0 or 1).
    pub playback_prepared: Gauge,
    /// Packets queued for the audio device, observed by the producer.
    pub playback_device_packets: Gauge,
    /// Frames queued before the most recent playback activation.
    pub playback_prefill_frames: Gauge,
}

pub static QUEUES: Queues = Queues {
    picture_preview: Gauge::new(),
    thumbnails: Gauge::new(),
    playback_prepared: Gauge::new(),
    playback_device_packets: Gauge::new(),
    playback_prefill_frames: Gauge::new(),
};

/// Pictures the main viewer showed during playback, by tier. Requested and
/// skipped counts come from the transport's picture scheduling (one picture
/// in flight, newest heard frame wins); the tier counts from the preview
/// worker that served them.
#[derive(Debug, Default)]
pub struct PlaybackPictures {
    /// Playback pictures requested from the preview worker.
    pub requested: Counter,
    /// Frames the heard clock passed while a picture was in flight, never
    /// requested (dropped). Loop wraps and seeks are not counted.
    pub skipped: Counter,
    /// Pictures served as the exact Original picture.
    pub exact: Counter,
    /// Pictures served from the reduced preview proxy.
    pub reduced: Counter,
    /// Original decoder repositions started between reduced pictures.
    pub repositions: Counter,
    /// Repositions that reached their target picture.
    pub repositions_reached: Counter,
    /// Repositions that failed; the decoder reopens for its next picture.
    pub repositions_failed: Counter,
    /// Look-ahead decoder positioning jobs started at an upcoming
    /// discontinuity (cut, Repeat restart).
    pub lookahead_started: Counter,
    /// Look-ahead jobs that reached their target picture.
    pub lookahead_reached: Counter,
    /// Look-ahead opens or jobs that failed.
    pub lookahead_failed: Counter,
    /// Pictures at which the look-ahead decoder became the serving one.
    pub lookahead_swaps: Counter,
}

pub static PLAYBACK_PICTURES: PlaybackPictures = PlaybackPictures {
    requested: Counter::new(),
    skipped: Counter::new(),
    exact: Counter::new(),
    reduced: Counter::new(),
    repositions: Counter::new(),
    repositions_reached: Counter::new(),
    repositions_failed: Counter::new(),
    lookahead_started: Counter::new(),
    lookahead_reached: Counter::new(),
    lookahead_failed: Counter::new(),
    lookahead_swaps: Counter::new(),
};

/// One observation of [`PLAYBACK_PICTURES`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaybackPictureTotals {
    pub requested: u64,
    pub skipped: u64,
    pub exact: u64,
    pub reduced: u64,
    pub repositions: u64,
    pub repositions_reached: u64,
    pub repositions_failed: u64,
    pub lookahead_started: u64,
    pub lookahead_reached: u64,
    pub lookahead_failed: u64,
    pub lookahead_swaps: u64,
}

impl PlaybackPictures {
    pub fn snapshot(&self) -> PlaybackPictureTotals {
        PlaybackPictureTotals {
            requested: self.requested.get(),
            skipped: self.skipped.get(),
            exact: self.exact.get(),
            reduced: self.reduced.get(),
            repositions: self.repositions.get(),
            repositions_reached: self.repositions_reached.get(),
            repositions_failed: self.repositions_failed.get(),
            lookahead_started: self.lookahead_started.get(),
            lookahead_reached: self.lookahead_reached.get(),
            lookahead_failed: self.lookahead_failed.get(),
            lookahead_swaps: self.lookahead_swaps.get(),
        }
    }
}

/// One cache's effectiveness and residency.
#[derive(Debug, Default)]
pub struct CacheCounters {
    pub hits: Counter,
    pub misses: Counter,
    pub evictions: Counter,
    pub entries: Gauge,
    pub bytes: Gauge,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheSnapshot {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entries: Level,
    pub bytes: Level,
}

impl CacheCounters {
    pub const fn new() -> Self {
        Self {
            hits: Counter::new(),
            misses: Counter::new(),
            evictions: Counter::new(),
            entries: Gauge::new(),
            bytes: Gauge::new(),
        }
    }
    pub fn snapshot(&self) -> CacheSnapshot {
        CacheSnapshot {
            hits: self.hits.get(),
            misses: self.misses.get(),
            evictions: self.evictions.get(),
            entries: self.entries.level(),
            bytes: self.bytes.level(),
        }
    }
}

impl CacheSnapshot {
    /// Hits over lookups, when any lookup happened.
    pub fn hit_rate(&self) -> Option<f64> {
        let lookups = self.hits + self.misses;
        (lookups != 0).then(|| self.hits as f64 / lookups as f64)
    }
}

/// One resident cache entry's contribution, released when the entry drops,
/// whether it is evicted or its whole cache is discarded.
#[derive(Debug)]
pub struct Resident {
    cache: &'static CacheCounters,
    bytes: u64,
}

impl Resident {
    pub fn new(cache: &'static CacheCounters, bytes: u64) -> Self {
        cache.entries.add(1);
        cache.bytes.add(bytes);
        Self { cache, bytes }
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        self.cache.entries.sub(1);
        self.cache.bytes.sub(self.bytes);
    }
}

#[derive(Debug, Default)]
pub struct Caches {
    /// Private decoded-PCM source sessions (CLI inspection and playback
    /// preparation; each host bounds its own at 16 sessions and 1 GiB).
    pub decoded_pcm: CacheCounters,
    /// Verified limited-output tiles (entries only; bytes are not tracked).
    pub limiter_tiles: CacheCounters,
    /// Exact limiter input bus blocks (entries and sample bytes).
    pub limiter_inputs: CacheCounters,
}

pub static CACHES: Caches = Caches {
    decoded_pcm: CacheCounters::new(),
    limiter_tiles: CacheCounters::new(),
    limiter_inputs: CacheCounters::new(),
};

/// Recent latency samples kept in a fixed ring.
pub const LATENCY_SAMPLES: usize = 128;

/// Submission counts and submit-to-completion latency in microseconds.
#[derive(Debug)]
pub struct LatencyRing {
    submissions: Counter,
    completions: Counter,
    last_us: AtomicU64,
    max_us: AtomicU64,
    next_slot: AtomicU64,
    samples: [AtomicU32; LATENCY_SAMPLES],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LatencySnapshot {
    pub submissions: u64,
    pub completions: u64,
    pub last_us: u64,
    pub max_us: u64,
    /// Over the most recent [`LATENCY_SAMPLES`] completions, when any.
    pub p50_us: Option<u64>,
    pub p95_us: Option<u64>,
}

impl LatencyRing {
    pub const fn new() -> Self {
        Self {
            submissions: Counter::new(),
            completions: Counter::new(),
            last_us: AtomicU64::new(0),
            max_us: AtomicU64::new(0),
            next_slot: AtomicU64::new(0),
            samples: [const { AtomicU32::new(0) }; LATENCY_SAMPLES],
        }
    }
    pub fn submitted(&self) {
        self.submissions.increment();
    }
    /// Store the sample in a reserved slot, then publish the completion, so a
    /// reader that observes the count also observes its sample. With one
    /// recording thread (the picture pipeline's device poll) this is exact;
    /// concurrent recorders could briefly expose a slot reserved by another
    /// recorder that has not stored yet.
    pub fn completed(&self, latency: Duration) {
        let micros = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
        let slot = self.next_slot.fetch_add(1, Relaxed) as usize % LATENCY_SAMPLES;
        self.samples[slot].store(u32::try_from(micros).unwrap_or(u32::MAX), Relaxed);
        self.last_us.store(micros, Relaxed);
        self.max_us.fetch_max(micros, Relaxed);
        self.completions.0.fetch_add(1, Release);
    }
    pub fn snapshot(&self) -> LatencySnapshot {
        let completions = self.completions.0.load(Acquire);
        let count = completions.min(LATENCY_SAMPLES as u64) as usize;
        let mut recent = [0_u32; LATENCY_SAMPLES];
        for (value, sample) in recent.iter_mut().zip(&self.samples).take(count) {
            *value = sample.load(Relaxed);
        }
        let recent = &mut recent[..count];
        recent.sort_unstable();
        let percentile = |p: usize| -> Option<u64> {
            (!recent.is_empty()).then(|| u64::from(recent[(recent.len() - 1) * p / 100]))
        };
        LatencySnapshot {
            submissions: self.submissions.get(),
            completions,
            last_us: self.last_us.load(Relaxed),
            max_us: self.max_us.load(Relaxed),
            p50_us: percentile(50),
            p95_us: percentile(95),
        }
    }
}

impl Default for LatencyRing {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared picture pipeline submissions (deadpan-render).
pub static GPU: LatencyRing = LatencyRing::new();

/// Live supervised worker processes and their summed physical footprint.
#[derive(Debug, Default)]
pub struct WorkerMemory {
    pub live: Gauge,
    pub footprint_bytes: Gauge,
    pub samples: Counter,
    /// Samples the operating system refused or could not supply.
    pub failures: Counter,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkerMemorySnapshot {
    pub live: Level,
    pub footprint_bytes: Level,
    pub samples: u64,
    pub failures: u64,
}

impl WorkerMemory {
    pub const fn new() -> Self {
        Self {
            live: Gauge::new(),
            footprint_bytes: Gauge::new(),
            samples: Counter::new(),
            failures: Counter::new(),
        }
    }
    pub fn snapshot(&self) -> WorkerMemorySnapshot {
        WorkerMemorySnapshot {
            live: self.live.level(),
            footprint_bytes: self.footprint_bytes.level(),
            samples: self.samples.get(),
            failures: self.failures.get(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Workers {
    /// Model inference: transcription, AI pause generation, face and
    /// tracking analysis.
    pub model: WorkerMemory,
    /// Every other supervised worker (render, encode, verification, probes).
    pub other: WorkerMemory,
}

pub static WORKERS: Workers = Workers {
    model: WorkerMemory::new(),
    other: WorkerMemory::new(),
};

/// How often a supervisor samples a live worker's footprint.
pub const WORKER_SAMPLE_INTERVAL: Duration = Duration::from_millis(500);

/// Which [`WorkerMemory`] a supervised worker reports into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerClass {
    Model,
    Other,
}

impl WorkerClass {
    pub fn memory(self) -> &'static WorkerMemory {
        match self {
            Self::Model => &WORKERS.model,
            Self::Other => &WORKERS.other,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoTotals {
    pub store_revisions: IoSnapshot,
    pub objects: IoSnapshot,
    pub media_snapshots: IoSnapshot,
    pub decoder_input: IoSnapshot,
    pub pcm_cache_file: IoSnapshot,
    pub proxy: IoSnapshot,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueLevels {
    pub picture_preview: Level,
    pub thumbnails: Level,
    pub playback_prepared: Level,
    pub playback_device_packets: Level,
    pub playback_prefill_frames: Level,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheTotals {
    pub decoded_pcm: CacheSnapshot,
    pub limiter_tiles: CacheSnapshot,
    pub limiter_inputs: CacheSnapshot,
}

/// Every counter at one moment. Fields are read independently; a snapshot
/// is not an atomic cut across counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub io: IoTotals,
    pub queues: QueueLevels,
    pub caches: CacheTotals,
    pub playback_pictures: PlaybackPictureTotals,
    pub gpu: LatencySnapshot,
    pub model_workers: WorkerMemorySnapshot,
    pub other_workers: WorkerMemorySnapshot,
}

pub fn snapshot() -> Snapshot {
    Snapshot {
        io: IoTotals {
            store_revisions: IO.store_revisions.snapshot(),
            objects: IO.objects.snapshot(),
            media_snapshots: IO.media_snapshots.snapshot(),
            decoder_input: IO.decoder_input.snapshot(),
            pcm_cache_file: IO.pcm_cache_file.snapshot(),
            proxy: IO.proxy.snapshot(),
        },
        queues: QueueLevels {
            picture_preview: QUEUES.picture_preview.level(),
            thumbnails: QUEUES.thumbnails.level(),
            playback_prepared: QUEUES.playback_prepared.level(),
            playback_device_packets: QUEUES.playback_device_packets.level(),
            playback_prefill_frames: QUEUES.playback_prefill_frames.level(),
        },
        caches: CacheTotals {
            decoded_pcm: CACHES.decoded_pcm.snapshot(),
            limiter_tiles: CACHES.limiter_tiles.snapshot(),
            limiter_inputs: CACHES.limiter_inputs.snapshot(),
        },
        playback_pictures: PLAYBACK_PICTURES.snapshot(),
        gpu: GPU.snapshot(),
        model_workers: WORKERS.model.snapshot(),
        other_workers: WORKERS.other.snapshot(),
    }
}

impl IoTotals {
    /// Named paths in display order.
    pub fn paths(&self) -> [(&'static str, IoSnapshot); 6] {
        [
            ("store_revisions", self.store_revisions),
            ("objects", self.objects),
            ("media_snapshots", self.media_snapshots),
            ("decoder_input", self.decoder_input),
            ("pcm_cache_file", self.pcm_cache_file),
            ("proxy", self.proxy),
        ]
    }
}

impl QueueLevels {
    pub fn named(&self) -> [(&'static str, Level); 5] {
        [
            ("picture_preview", self.picture_preview),
            ("thumbnails", self.thumbnails),
            ("playback_prepared", self.playback_prepared),
            ("playback_device_packets", self.playback_device_packets),
            ("playback_prefill_frames", self.playback_prefill_frames),
        ]
    }
}

impl CacheTotals {
    pub fn named(&self) -> [(&'static str, CacheSnapshot); 3] {
        [
            ("decoded_pcm", self.decoded_pcm),
            ("limiter_tiles", self.limiter_tiles),
            ("limiter_inputs", self.limiter_inputs),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauges_track_shares_and_keep_their_high_water_mark() {
        static GAUGE: Gauge = Gauge::new();
        let mut first = Share::new(&GAUGE);
        let mut second = Share::new(&GAUGE);
        first.set(2);
        second.set(3);
        assert_eq!(
            GAUGE.level(),
            Level {
                current: 5,
                high: 5
            }
        );
        first.set(1);
        assert_eq!(
            GAUGE.level(),
            Level {
                current: 4,
                high: 5
            }
        );
        drop(second);
        assert_eq!(
            GAUGE.level(),
            Level {
                current: 1,
                high: 5
            }
        );
        GAUGE.sub(10);
        assert_eq!(GAUGE.level().current, 0);
        drop(first);
        assert_eq!(GAUGE.level().current, 0);
    }

    #[test]
    fn io_counts_operations_and_bytes() {
        let io = IoCounters::new();
        io.read(10);
        io.read(5);
        io.write(7);
        assert_eq!(
            io.snapshot(),
            IoSnapshot {
                read_ops: 2,
                read_bytes: 15,
                write_ops: 1,
                write_bytes: 7
            }
        );
    }

    #[test]
    fn latency_ring_keeps_recent_percentiles_in_bounded_storage() {
        let ring = LatencyRing::new();
        assert_eq!(ring.snapshot().p50_us, None);
        for micros in 1..=200_u64 {
            ring.submitted();
            ring.completed(Duration::from_micros(micros));
        }
        let snapshot = ring.snapshot();
        assert_eq!(snapshot.submissions, 200);
        assert_eq!(snapshot.completions, 200);
        assert_eq!(snapshot.last_us, 200);
        assert_eq!(snapshot.max_us, 200);
        // Only the last 128 samples (73 through 200) remain.
        assert_eq!(snapshot.p50_us, Some(136));
        assert_eq!(snapshot.p95_us, Some(193));
    }

    #[test]
    fn cache_hit_rate_is_undefined_without_lookups() {
        let cache = CacheCounters::new();
        assert_eq!(cache.snapshot().hit_rate(), None);
        cache.hits.add(3);
        cache.misses.increment();
        assert_eq!(cache.snapshot().hit_rate(), Some(0.75));
    }
}
