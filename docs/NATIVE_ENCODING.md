# Native SDR encoding boundary

`native/deadpan-encode` accepts composed limited-range Rec.709 I420 and finite
48 kHz planar stereo PCM. It writes one private H.264/AAC MP4 through an owned
read/write file descriptor. It has no project, source decoder, GPU, audio device,
destination path or publication authority.

The [encoded render child](ENCODED_RENDER.md) now streams real committed pictures
and canonical audio directly into this library. Independent production verification
of the encoded candidate and publication remain required. The complete
audio/effects graph and HDR also remain open.

## Exact input and policy

`EncodeContract::new` captures the even raster, reduced rational frame rate,
picture count, exact authored sample count, explicit hardware/software attempt
and requested B-frame policy. For rate N/D, output picture ordinal n has PTS nD,
duration D and time base 1/N. PCM starts at output sample zero. The caller supplies
`B(project_end)-B(project_start)`, including nonzero-origin rounding phase.
For 30000/1001, project frames `[1,2)` contain 1601 samples, not 1602.

`NextInput` orders inputs by exact clocks, with picture winning ties. Every
picture and sample is accepted exactly once. AAC inputs contain 1024 samples
except for the exact short final block. Invalid clocks, missing or duplicate
inputs, malformed planes, nonfinite PCM and native failures poison the session.
No later finish can turn that session into a successful output.

The engineering policy derives video bitrate from specification 22.4's pixel
classes and frame-rate band, interpolating by actual pixel count. It requests
High profile, progressive square pixels, left-sited limited Rec.709 I420, a GOP
near half the frame rate, and AAC-LC stereo at 384 kbit/s. Hardware and OS software
are separate attempts with explicit VideoToolbox options. No attempt silently
switches encoder mode or B-frame policy.

The pinned VideoToolbox adapter omits packet durations. This boundary assigns
only missing video durations from the already authored CFR contract, rejects
conflicting nonzero durations, and records the assignment count. It retains
encoder PTS/DTS, including negative priming/reordering coordinates. It never
shifts PCM or drops packets to conceal encoder delay.

`EncodeError::kind()` classifies exact native error codes separately from their
diagnostics. A missing named video encoder is distinct from missing AAC and
generic codec-open failures. A video packet with PTS before DTS fails with
`video_timestamp_order` before packet admission or muxing, retaining both actual
timestamps. Capacity, I/O, invalid input and control failures cannot become
capability evidence through their message text. These kinds do not authorize
automatic fallback; a future host policy must separately qualify its choice.

## File, work and cancellation limits

Output must be an empty regular file owned by the current user, with one link,
private permissions and read/write access without append. The Rust owner keeps
the descriptor alive until native close. No destructor writes a trailer.

Hard bounds include 8192 per axis, 33,554,432 pixels, one million video frames,
24 hours, 64 GiB output, two million packets and 32 MiB per packet. The caller
may lower byte and packet limits. Chronological input, bounded pending codec
frames and drain work bound interleave residency. Packet counts bound the pinned
MP4 sample tables and the two fast-start relocation buffers; the documented
conservative moov bound is 128 bytes per admitted packet plus 1 MiB.

MP4 uses explicit `use_editlist=1`, `avoid_negative_ts=disabled`, an exact checked
LCM movie timescale and `+faststart`. The muxer's one permitted secondary open
is a fixed sentinel in read mode during the trailer. It gets an independent
`pread` cursor over the same owned output descriptor. Other opens are rejected.
Successful finish requires every input, complete packets, both codec EOFs,
completed relocation, flush, file synchronization and matching file extent.

One borrowed cancellation flag and absolute monotonic deadline cover open,
every push and finish. Rust checks before and after native work; C receives only
the remaining allowance. These are cooperative limits. A driver or filesystem
call still needs the supervised child process for hard cancellation.

## Canonical offline audio

`deadpan_cli::audio::OfflineAudioSession` captures an explicit immutable revision
and nonempty frame range through a read-only store. It returns up to 8192 limited
stereo samples on the absolute project grid, preserving canonical source/DSP
context and limiter halos beyond the requested output interval. It adds no
monitoring gain, normalization or authored tail.

The caller's original deadline includes capture, source snapshotting, native
opening and every read. Cold source phases receive separately recomputed
remaining budgets. The existing 256-sample inspection API remains unchanged.
Unsupported processing and missing cold media fail explicitly. Warm verified
PCM and historical revisions survive concurrent edits, undo/redo and linked
path loss.

## Qualification and remaining integration

The native fixture example and `qualify_native_encode.py` retain actual MP4,
FFmpeg ordinary/manual audio, AVFoundation audio, packet/box observations and
fresh-decoder GOP suffixes. Queried codec fields are requested/context properties;
only emitted-file observations qualify a particular path. Short audio uses
separate nonoverlapping absolute marker windows, with missing/shifted-marker
negative tests. No decoded PCM is aligned or cropped by observed events.

On the measured M5 Max, hardware B-frame attempts fail because a packet PTS
precedes DTS. Retain that rejected capability. Hardware without B-frames and
explicit OS software attempts require their own file evidence. An encoder
success is never permission to publish a file without independent verification.

The supervised child feeds one retained committed picture and one canonical AAC
input block directly, without spooling an uncompressed movie through the 512 MiB
raw qualification format. The host still needs isolated emitted-file verification,
durable job/recovery state, checked
destination-side partial-file publication and the native Render workflow.
