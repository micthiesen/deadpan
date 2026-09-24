# Revision-bound audition

`Engine` connects the canonical `LimitedAudio` renderer to the prepared output
queue. It applies a finite oversampled limiter after the current edge-faded bus;
voice effects, sends and the full group mix remain outside this path. A fixed
monitor gain in `[0,1]` is applied to
canonical PCM; nonfinite or out-of-range results fail instead of being clipped
or normalized.

The application supplies one immutable `Snapshot`, with source receipts resolved
at its document revision and a connection-free `OriginalImportHandle`. No worker
opens SQLite or acquires the writer lock. The application must stop on project
close, revision changes, navigation, and lifecycle interruption. `StopHandle`
performs the same bounded cancellation and atomic generation revocation as
`Engine::stop`, without media/device I/O or joining a worker.

Two persistent threads own separate responsibilities:

- Preparation compiles the document, checks receipt/original bindings, snapshots
  and decodes sources, and prepares verified canonical 8192-frame limited tiles.
  The bounded real input halo uses internal bus reads of at most 256 frames.
  A single immutable session's source and complete DSP caches survive seeks.
  Reuse requires identical document and receipt objects, session identity, and
  original records. New sessions/revisions replace the cache before preparation.
- Control owns the native device, prefills 8192 frames (or the complete shorter
  sequence), activates a fresh channel generation, and drains delivery reports.
  UI positions come only from retained device delivery intervals and its stream
  clock. Producer progress never becomes a heard position. Missing coverage
  holds position; sustained gaps, report loss, a stalled clock, route changes,
  starvation and device faults terminate playback explicitly.

The PCM handoff retains at most two 8192-frame stereo batches, in addition to the
bounded native output queue, four verified limiter tiles, twelve exact input
bus blocks of at most8192frames and bounded active
preparation context. Source PCM caches retain at
most 16 sources and 1 GiB of aggregate decoded physical samples on disk. Receipt
sample counts reserve that capacity before decoding; the reopened index must
match. Original snapshot and audio opening deadlines are respectively 15 and 30
seconds. Each limited batch shares one 60-second deadline across all cache
validation, source/stage reads and limiting. Native decode cancellation
is cooperative, and source I/O is not a hard real-time latency guarantee.

DSP uses `StageLimits::default`: complete Preserve input is limited to 1,048,576
frames, output to 8,388,608 frames, resident stereo stages to 16,777,216 frames,
and stage count/depth to 64/32. Source caches, decoder state and native FFT memory
are separate from stereo stage residency. Unsupported large stages fail before
playback rather than substituting a different stretch algorithm.

Native output accepts the current default device's existing 48 kHz stereo f32
configuration. Every play/seek opens a new device/channel generation. Stop,
restart, stale preparation replies, short EOS and fault behavior are tested with
a headless device using the real queue. Ordinary tests do not activate hardware.
`shutdown` revokes output immediately and requests worker exit; it does not block
the calling UI thread while cooperative media teardown finishes.
