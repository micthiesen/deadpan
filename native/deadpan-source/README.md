# Persistent source decoder

The separate `audio` module decodes an explicitly selected AAC-LC or signed16
little-endian PCM stream at its original rate and channel layout. It retains raw
PTS/DTS, duration, sample count/format, discard and manual skip evidence, and
returns owned interleaved f32 without resampling, mixing or gain. AAC/MP4 and
PCM/WAV have actual fixture coverage. A strict header guard checks allocation
sizes and table expansion before FFmpeg opens the audio container. Other audio
container grammars, custom layouts and unsupported representations fail.
See [source audio](../../docs/SOURCE_AUDIO.md) for host indexing, private PCM
cache semantics and remaining work. `tests/audio_decode.rs` and the media
crate's audio session tests run without the GUI.

`SourceDecoder` owns one regular input descriptor and one persistent FFmpeg demux/
software decoder session. It performs no subprocess launch, path reopening,
networking, database write, or authored mutation. `next_metadata` decodes and
retains the next presented AVFrame without allocating/converting RGB.
`copy_current_rgba` converts only that frame and copies into a Rust-owned `Vec`.
`next_rgba` combines both operations. `seek` seeks backward in original stream
ticks, flushes reorder state, and leaves exact indexed target selection to the
host. Returned pixels survive subsequent decoding, seek, and session destruction.

The safe Rust interface contains all unsafe code in its private FFI module. The
C ABI owns its heap context and borrows the descriptor until Drop. Operation
callbacks borrow an `AtomicBool` only during the synchronous call; C clears the
callback and borrowed error pointer before returning. The context has no global
mutable state or process-wide FFmpeg callback changes. It is movable between
threads and cannot be shared concurrently. A failed native operation poisons the
session. A pre-call cancellation or invalid timeout does not consume its state.

Owning `File` does not make its contents immutable. The host must provide a
private immutable snapshot and retain content identity independently. C rechecks
that the descriptor is regular and that its stated size matches at opening.
`pread` keeps file-position sharing out of the contract. AVIO bounds seeks/reads
to the descriptor, denies all secondary opens, disables MOV data references, and
requires a network-disabled, LGPL-only FFmpeg 8.0.3 build at build time and runtime.

Both probe and decoder have pixel, stream, packet and I/O limits. Probe bytes,
probe packets, analysis duration, index memory, decode iterations, frames between
seeks and input length are bounded. `DecodeLimits::validate` checks hard bounds
before host snapshot copying or descriptor inspection; C independently rechecks
them. Caller limits cannot exceed hard bounds. A
separate `max_pixels` budget defaults to 16,777,216 coded/visible pixels, so an
owned RGBA frame is at most 64 MiB by default. It is checked before stream probing,
applied to FFmpeg probe/decoder allocation and checked again before Rust RGBA
allocation. The configurable hard limit is 8192 squared pixels; the dimension
limit also applies. Cancellation/timeout preflight precedes RGBA allocation. A
positive per-call timeout of at most 60 seconds and cancellation are checked on
I/O and around native work. These are cooperative deadlines: an individual
FFmpeg codec operation is not preempted. This adapter does not establish an OS
process memory/CPU limit and must stay off UI/audio callback threads.

The initial admitted containers are MP4/MOV and Matroska/WebM, with one H.264 or
FFV1 video stream and up to 32 ignored audio streams. The bounded audio inventory
retains each stream's index, codec, original time base and available probe-level
start, duration, sample-rate and channel-count observations. It does not decode
audio, establish exact sample bounds or claim an audio stream is ready for use.
`AVDISCARD_ALL` alone is not a decode barrier during FFmpeg probing. The format
codec allowlist contains only `h264,ffv1`; pinned FFmpeg propagates that allowlist
to probe decoder initialization and rejects AAC before opening its decoder.
Audio-bearing fixtures therefore emit expected AAC-not-on-whitelist diagnostics.
The measured fixtures are H.264 in MP4 and FFV1 in Matroska; this is not
qualification of every profile/container combination. Other video codecs require
further fixtures and explicit admission. No audio decode or VideoToolbox
acceleration is implemented here.

The boundary admits progressive eight-bit three-component SDR input with explicit
range, matrix, transfer and primaries. RGB must be full-range GBR; YUV supports
BT.709, BT.601 and BT.2020 nonconstant matrices. Supported transfers are BT.709,
sRGB and linear, with BT.709/BT.2020/P3-D65 primaries retained for the shared
renderer. libswscale applies the matrix/range transform into full-range RGBA8.
It uses explicit source chroma siting for subsampled YUV, rejecting missing or
changing siting metadata. It does not change gamma or gamut. Exact RGB and BT.709 range/matrix vectors are
covered by tests; wider-gamut and BT.601 admission still needs additional end-to-
end qualification. Stream, packet and frame HDR/ICC/ambient metadata are rejected
even when the transfer tags claim SDR. Missing interpretation, HDR, depth above eight bits, alpha,
interlace, unsupported display transforms and stream/geometry/color changes fail
explicitly. Standard codec padding is cropped only to the immutable declared
visible rectangle. Right-angle rotation and SAR are reported, not baked into
pixels. Missing SAR retains FFmpeg's conventional square-pixel interpretation.

Frame PTS remains original, not `best_effort_timestamp`; missing PTS is rejected.
DTS hints and positive decoder-reported frame durations are optional. Stream
start/duration and container start/duration remain distinct observed candidates.
No frame-rate fallback, terminal-duration guess, or origin normalization occurs.
The VFR fixture intentionally reports a final 1001-tick duration despite the
fixture author's intended 3003 ticks. Tests require the actual 1001 ticks; the
host index must not claim recovery of that lost intent.

## Verification

With `DEADPAN_FFMPEG_PREFIX` naming the pinned build:

```sh
cargo test -p deadpan-source --locked
cargo clippy -p deadpan-source --all-targets --locked -- -D warnings
```

The integration suite covers real CFR/B-frame, VFR and offset source clocks,
396 backward/random seeks with exact PTS and linear-decode pixel-hash agreement,
retained pixel ownership, exact RGB values, BT.709 limited/full vectors,
orientation and anamorphic metadata, unsupported interlace/HDR/depth/missing
color, stream-level HDR with SDR transfer tags, packet/frame/byte/geometry bounds,
cancellation, and external-playlist
rejection. `tests/fixtures/manifest.json` records committed file identities and
producer provenance. Sanitizer results and host/GPU integration belong to the
root task's evidence report.
