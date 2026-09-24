# Prepared audio output boundary

`native/deadpan-output` is a narrow device boundary and a headlessly testable
prepared-PCM queue. It implements no project edits, decoding, expensive DSP,
master limiter or project transport. The separate [playback engine](PLAYBACK.md)
connects canonical pre-master `StageAudio` samples to this output.

## Queue and clock contract

One preparation/controller thread owns `Feed`; one callback owns `Callback`.
Construction allocates 32 PCM slots plus one reserved terminal slot. Each PCM packet contains at most 256
stereo float frames by value. PCM must be finite and within magnitude one.
Admission rejects invalid data; it never normalizes, clips or limits it.
Failed admission leaves the contiguous 48 kHz sample cursor unchanged. Explicit
EOS is queued after all accepted PCM in its reserved slot, even when PCM is full.

`restart(start)` allocates a fresh generation, invalidates earlier PCM and stays
muted while the producer prepares data. `activate(generation)` publishes the
prepared generation only after prefill. It cannot be repeated to revive a
starved generation. `pause()` also consumes a generation. Tokens are scoped to
one channel instance so a recreated device cannot accept old worker results.
Both channel identity and generation belong in future audio/video work tags.

After a native device pause, use `Feed::restart`, `DeviceOutput::start_device`,
prefill with bounded off-callback retries, then `Feed::activate`. Starting the
muted callbacks first lets them release a completely full stale queue. Waiting
for prefill before restarting native callbacks can otherwise stall on `Full`.
Initial construction has an empty queue and can be prefilled before starting.

The callback handles partial packets, arbitrary interleaved buffer partitions,
bounded stale-packet cleanup and EOS. At most 33 stale packets are discarded per
call; matching packet work is additionally bounded by the 8,192-frame output
limit. Cleanup can produce a silent buffer while preparation catches up.
An underrun preserves any already copied prefix, silences the suffix and latches
`Starved`. Late PCM never resumes against a drifting device clock. The controller
must explicitly prepare and activate a fresh generation. Invalid buffer shapes,
discontinuous packets and backend faults permanently silence the channel.

A final generation/fault check silences a buffer invalidated during copying.
This cannot retract a buffer already submitted to CoreAudio or prevent a change
immediately after the final check. Seek acknowledgement and video invalidation
must account for that device-buffer boundary. The application transport is not
implemented by these queue states.

The callback uses no heap-owned packet contents, locks, waiting, logging, media
I/O or expensive DSP. PCM storage, telemetry and shared state are allocated
before starting. Render buffers are admitted up to 8,192 stereo frames. An
invalid or oversized host buffer is zeroed and faults; zeroing necessarily
scales with the supplied buffer size. Endpoints and stream ownership are
destroyed on the controller side, never inside `render`.

## Initial macOS device admission

The adapter pins CPAL 0.18.2 and rtrb 0.4.0. It reads the current default output
and reacquires its stable ID as an explicit device. This avoids CPAL's automatic
default-route following. It admits only the existing reported 48 kHz, stereo,
F32 configuration and leaves the requested buffer size at `Default`. CPAL still
attempts to set the physical format internally; matching the reported virtual
configuration does not prove that no physical format changes. The adapter
rechecks the reported default route/format after construction and before start.

The controller must poll `check_route()` and handle native lifecycle
notifications. Every backend notification faults the channel, including xruns.
`check_route` also reports an existing permanent fault even if the route has not
changed.
Errors are represented by atomic category flags; CPAL can drop notifications
under contention, so these are not exhaustive error counts. Recovery requires
recreating the boundary and preparing a new generation. Real device switching,
headphone disconnect, sleep/wake, other rates/layouts and automatic recovery are
still unqualified.

`DeviceReport` records the callback's generation, submitted content prefix,
silence, status, discarded packets, host callback/playback timestamps and measured
timestamp-validation/kernel-render cost. Timing excludes telemetry publication
and upstream/driver work; it is not a full callback deadline. A bounded
256-record ring drops telemetry on overflow
and increments a counter without blocking audio. `sample_at` maps the stream
clock only inside the reported content prefix. Consumers must match the active
generation and stream first; it never extrapolates across silence or a seek.
CPAL's playback timestamp is a latency estimate, not evidence of acoustic
delivery. Display scheduling and an application audio master clock remain open.

## Verification and limits

Ordinary tests do not open audio devices:

```sh
cargo test --locked -p deadpan-output
```

Tests cover independent PCM partitions, queue admission, stale/partial seeks,
prepare/activate interleavings, cross-channel tokens, explicit end, starvation,
permanent faults, checked identity/cursor exhaustion and producer/consumer
threads. A thread-local allocator probe counts allocation, zeroed allocation,
reallocation and deallocation inside the queue callback. It does not instrument
CPAL or CoreAudio before/after our closure.

The explicit hardware harness emits JSON and never changes system volume:

```sh
cargo run --release --locked -p deadpan-output --example qualify_output
cargo run --release --locked -p deadpan-output --example qualify_output -- --tone
```

The optional tone is 200 ms at -42 dBFS with 10 ms ramps. The harness also checks
seek, deliberately induced starvation, late refill, pause and a freshly
prepared restart. Source audit and retained results are separate evidence in
[dependency audit](qualification/audio-output-dependency-audit-2026-09-21.md)
and [hardware qualification](qualification/audio-output-2026-09-21.md).
Stock CPAL has an exceptional OS-clock failure path that may allocate before
our closure; that path and the full release realtime contract remain open.
There is no loopback/listening corpus, editing/inference stress suite or product
latency claim here. Gate A and DP-09/16/24 remain partial/open.
