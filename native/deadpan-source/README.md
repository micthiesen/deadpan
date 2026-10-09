# Persistent source decoder

The separate `audio` module decodes an explicitly selected AAC-LC, mono/stereo
Opus, MP3 or unsigned8, signed16/24/32 and float32 PCM stream at its original rate and channel layout. It retains raw
PTS/DTS, duration, sample count/format, discard and manual skip evidence, and
returns owned interleaved f32 without resampling, mixing or additional gain.
AAC/MP4, raw MP3, Opus/WebM/Matroska and PCM/WAV have actual fixture coverage. A strict header guard checks allocation
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
`next_rgba` combines both operations. `seek` seeks backward in the reported
presentation clock, flushes reorder state, and leaves exact indexed target selection to the
host. `seek_to` additionally names the target PTS: packets presented before it
skip their non-reference pictures (`AVDISCARD_NONREF`), which are never
returned and on which no other picture depends. Skipping applies only while
every SPS, from the AVC configuration and in-band, declares
`bitstream_restriction` and `frame_mbs_only_flag`: without a declared reorder
depth FFmpeg estimates it from the pictures it sees, which skipping changes,
and field pictures pair across packets. `DecodeWork::decoded_pictures` counts
pictures the codec actually decoded. Pictures at or after the target
always decode, so forward steps continue. Interlaced input disables skipping and
uses the preceding GOP as temporal context. Returned pixels survive subsequent
decoding, seek, and session destruction.

`DecodeLimits::threads` (1 to 16, default 1) enables FFmpeg frame and slice
threading above one. Both threading and skipping return pictures bit-identical
to a sequential single-threaded decode (`tests/threaded_seek.rs`); they change
latency and memory only. With frame threading, `get_format` and `get_buffer2`
run on codec threads, possibly after an exported call has returned. They
therefore read only immutable limits and record a rejection in an atomic code
that the next caller-thread check turns into its specific error; they never
touch the borrowed cancellation pointer, deadline or error buffer. Codec threads
are joined when the context is freed, and a replacement codec clears the code. `lower_current_thread_priority` sets the
calling thread to utility QoS on macOS, so a background index measurement and
its codec threads yield to interactive decoding.

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

MP4/MOV admits one qualified H.264, HEVC, VP9 or ProRes 422 video stream and up to 32
ignored AAC streams. Finite Matroska admits one FFV1 or SDR VP9 video track;
WebM admits one SDR VP9 video track. Both admit up to 32 qualified Opus tracks,
including audio-only files through the separate audio decoder.
The bounded audio inventory
retains each stream's index, codec, original time base and available probe-level
start, duration, sample-rate and channel-count observations. It does not decode
audio, establish exact sample bounds or claim an audio stream is ready for use.
`AVDISCARD_ALL` alone is not a decode barrier during FFmpeg probing. The format
codec allowlist contains only `h264,ffv1,hevc,vp9,prores`; pinned FFmpeg propagates that allowlist
to probe decoder initialization and rejects AAC/Opus before opening their decoders.
Audio-bearing fixtures therefore emit expected audio-not-on-whitelist diagnostics.
The measured fixtures are H.264/HEVC/VP9 in MP4, ProRes 422 in QuickTime, FFV1/VP9 in Matroska and
VP9 with mono/stereo Opus in WebM; this is not
qualification of every profile/container combination. Other video codecs require
further fixtures and explicit admission. The picture decoder never decodes audio
or uses VideoToolbox acceleration.

ProRes 422 Proxy/LT/Standard/HQ has a packet guard before FFmpeg submission:
exact frame/picture/slice lengths, bounded stable raster, explicit SDR color,
no alpha, consistent field order and container rate/aspect hints. Ten-bit 422
uses the existing direct RGBA64 converter; progressive and both interlaced
orders retain exact source time. Closed QuickTime AAC wrappers preserve audio.
The fixtures include Apple's VideoToolbox HQ output, its bounded redundant
ImageDescription, and its initially unspecified frame-rate hint. See
[ProRes qualification](../../docs/qualification/prores-sources-2026-10-09.md).

VP9 MP4 profile 0/eight-bit and profile 2/ten-bit 4:2:0 preserve explicit SDR
color, left/top-left chroma siting and exact sample clocks. Bounded `vpcC`
admission and packet headers reject changing geometry, unhandled render-size
transforms and contradictory configuration before decode. Hidden altref
pictures and `show_existing_frame` use their normal presentation semantics.
See [qualification](../../docs/qualification/vp9-sources-2026-10-09.md) for
real color, thread, seek, proxy and emitted-file evidence and remaining formats.

The [WebM/Matroska VP9 path](../../docs/qualification/webm-vp9-sources-2026-10-09.md)
retains explicit color, container SAR, millisecond PTS and a separately reported
nominal nanosecond frame duration. Admission requires a leading key and uses
the same per-packet bounds as MP4. The [Opus path](../../docs/qualification/opus-sources-2026-10-09.md)
uses pinned libopus 1.6.1, explicit pre-skip and terminal padding, and a checked
48 kHz sample clock alongside retained raw container timestamps. CELT, SILK,
hybrid and pre-skip spanning packets have fixtures. Ogg, mapping families above
zero, lacing and the broader container grammar remain unqualified.

The single HDR interpretation (PQ/HLG, BT.2020, ten-bit 4:2:0 HEVC Main10 or
H.264 High10 in MP4, with optional exact static metadata) and the sixteen-bit
`next_rgba16` and ten-bit `next_yuv420p10` outputs are described in
[source admission](../../docs/SOURCE_ADMISSION.md#hdr-sources) and tested by
`tests/hdr_decode.rs`. SDR also admits HEVC Main/Main10 eight/ten-bit and
H.264 High10 ten-bit 4:2:0 and ProRes ten-bit 4:2:2 with RGBA64 picture output,
tested by `tests/sdr_hevc.rs` and `tests/prores.rs`. Other admitted SDR inputs use eight-bit three-component
pixels with explicit
range, matrix, transfer and primaries. RGB must be full-range GBR; YUV supports
BT.709, BT.601 and BT.2020 nonconstant matrices. Supported transfers are BT.709,
sRGB and linear, with BT.709/BT.2020/P3-D65 primaries retained for the shared
renderer. libswscale applies the matrix/range transform into full-range RGBA8.
It uses explicit source chroma siting for subsampled YUV, rejecting missing or
changing siting metadata. It does not change gamma or gamut. Exact RGB and BT.709 range/matrix vectors are
covered by tests; wider-gamut and BT.601 admission still needs additional end-to-
end qualification. Stream, packet and frame HDR/ICC/ambient metadata are rejected
even when the transfer tags claim SDR. Missing interpretation, unqualified HDR/depth, alpha,
unsupported display transforms and stream/geometry/color changes fail
explicitly. Standard codec padding is cropped only to the immutable declared
visible rectangle. Right-angle rotation and SAR are reported, not baked into
pixels. Missing SAR retains FFmpeg's conventional square-pixel interpretation.

Interlaced pictures use the pinned LGPL FFmpeg 8.0.3 BWDIF filter with
`mode=send_field:parity=auto:deint=interlaced`, before color conversion. A
one-coded-picture clip has no temporal neighbors and uses spatial bob: copy
the current field's rows, average nearest same-field rows, replicate boundaries.
The stream's `bwdif_fields` flag (wire `bwdif_fields_v2`) identifies this recipe.
A bounded three-entry clock queue maps internal ordinals to exact sixth-source
ticks. Two-field and three-field pictures divide the measured interval into
halves or thirds, repeating the first field for the third. The last picture
requires its own decoded duration, never an extrapolated interval. Progressive
pictures inside an admitted field stream pass through once. H.264's explicit
picture-timing SEI, carried through packet/frame references, determines field
structure independently of codec thread history. Repeat hints are removed
before BWDIF so they cannot bypass neighboring interlaced pictures.
No automatic format conversion occurs. Every plane must have at least 3x4
samples; missing terminal duration and unrepresentable clocks fail explicitly.
Both field orders, B frames, odd VFR intervals, single-picture clips, thread
equivalence and seeks have real encoded fixtures in `tests/deinterlace.rs`.
`DecodeLimits::progressive_only` validates outputs without source presentation:
progressive HRD timing passes, while interlace and repeats fail explicitly.

MP4 `clap` clean apertures retain exact rational bounds within the sample-entry
raster. Integral rectangles compact RGBA8/RGBA64 after full-raster color
conversion, preserving chroma interpolation at odd crop origins. Fractional
rectangles retain the complete backing raster and exact clean geometry for the
shared renderer and analysis consumers. Source PTS, audio, SAR and rotation stay
intact. Raw I420 and ten-bit planes require an even chroma-aligned integral
rectangle and reject otherwise before consuming a frame. Input/codec allocation
limits still apply to the full raster. See the
[fractional-aperture qualification](../../docs/qualification/fractional-aperture-2026-10-09.md).

Sources outside the field recipe retain original PTS, not
`best_effort_timestamp`; the field recipe uses the exact sixth-tick clock above,
including progressive pictures with H.264 picture-timing support. Missing PTS is rejected.
DTS hints and positive decoder-reported progressive durations are optional. Stream
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
orientation and anamorphic metadata, unsupported interlace cases/HDR/depth/missing
color, stream-level HDR with SDR transfer tags, packet/frame/byte/geometry bounds,
cancellation, and external-playlist
rejection. `tests/fixtures/manifest.json` records committed file identities and
producer provenance. Sanitizer results and host/GPU integration belong to the
root task's evidence report.
