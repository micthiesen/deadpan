# Native encoding boundary

`native/deadpan-encode` accepts composed limited-range Rec.709 I420 (SDR) or
planar 10-bit BT.2020 NCL PQ/HLG pictures (HDR), and finite 48 kHz planar stereo
PCM. It writes one private H.264/AAC or HEVC Main10/AAC MP4 through an owned
read/write file descriptor. It has no project, source decoder, GPU, audio device,
destination path or publication authority.

The [encoded render child](ENCODED_RENDER.md) now streams real committed pictures
and canonical audio directly into this library. Independent production verification
of the encoded candidate and publication remain required. The complete
audio/effects graph and HDR host integration also remain open.

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

After the native writer closes, a bounded descriptor-only finalizer checks the
video media-duration header. Pinned FFmpeg 8.0.3 can overstate it for certain
legal B-frame orders. A correction requires complete packet-table proof of the
authored CFR interval and the exact first-CTS/minimum-CTS overestimate. Only the
video `mdhd` duration field changes, with synchronization and readback; PTS/DTS,
sample tables, edits and AAC remain intact. The optional
`EncodeReport.video_media_duration_correction` records the measured old/new
ticks and composition offsets. Inconsistent evidence fails. The independent
finished-file verifier retains every strict clock check. See the
[retained-file regression](qualification/mux-duration-2026-10-08.md).

`EncodeError::kind()` classifies exact native error codes separately from their
diagnostics. A missing named video encoder is distinct from missing AAC and
generic codec-open failures. A video packet with PTS before DTS fails with
`video_timestamp_order` before packet admission or muxing, retaining both actual
timestamps. Capacity, I/O, invalid input and control failures cannot become
capability evidence through their message text. These kinds do not authorize
automatic fallback; a future host policy must separately qualify its choice.

## HDR HEVC Main10 contract

`EncodeContract::new_hdr_v1(raster, rate, frames, samples, mode, b_frames,
HdrSignal)` reuses every SDR v1 admission rule, clock, GOP, movie timescale,
B-frame request and AAC target. Frozen `HDR_POLICY_VERSION_V1` changes only the
video bitrate: `(sdr_v1_bitrate * 5 + 2) / 4` in u64 arithmetic. `new_v1`,
`SDR_POLICY_VERSION_V1` and the SDR serialization are unchanged; HDR contracts
serialize one additional `hdr` object (`policy_version` and the signal).
`video_format()` distinguishes `H264Rec709I420`, `HevcMain10Rec2100Pq` and
`HevcMain10Rec2100Hlg`; HDR `picture_bytes()` is `2*(w*h*3/2)`.

HDR input is tight planar 4:2:0 little-endian u16 samples in the low 10 bits:
Y (w*h), Cb (w/2*h/2), Cr. Both Rust and native admission require limited range
(Y 64..=940, Cb/Cr 64..=960); any other code, including 941..1023 headroom and
anything above 1023, fails as invalid input and poisons the session. Native code
packs P010LE (`code << 6`, interleaved CbCr) for `hevc_videotoolbox` with the
`main10` profile option. The codec context and every AVFrame declare BT.2020
primaries, BT.2020 NCL, SMPTE ST 2084 or ARIB STD-B67, limited range and left
chroma; VideoToolbox derives emitted VUI from the frame attachments, so the
frame tags are required. The stream uses the `hvc1` sample entry (parameter sets
only in `hvcC`).

Static metadata follows the shared design units. `MasteringDisplay` has R,G,B
`primaries` and `white_point` in 1/50000 and `max_luminance`/`min_luminance` in
1/10000 cd/m²; validation requires positive CIE xy points with x+y<=1, a
counter-clockwise R,G,B triangle enclosing the white point, max in
50..=10000 cd/m², min <= 50 cd/m² and min < max. `ContentLight { max_cll,
max_fall }` is in cd/m², at most 10000, with FALL <= CLL (zero means unknown).
HLG is metadata-free: an HLG signal with mastering is rejected, and HLG/SDR
finish refuses content light. A PQ session must call
`EncoderSession::finish_with_light(Some(light))` with host-computed MaxCLL/
MaxFALL; the native finish attaches mastering (when configured) and content
light to the output stream's coded side data immediately before the trailer,
where the pinned `movenc` writes `mdcv` and `clli` in the `hvc1` sample entry.
Pinned VideoToolbox inserts no mastering/content-light SEI, so these container
boxes are the only static metadata in the file.

VideoToolbox attaches a Dolby Vision profile 8.4 RPU (HEVC NAL type 62) to every
HLG picture. Deadpan neither authors nor qualifies Dolby Vision, and its files
carry no `dvcC` record, so the native adapter removes NAL types 62 and 63 from
each Annex B packet before muxing, retaining every other NAL byte for byte, and
reports the count as `VideoCodecInfo.removed_unspecified_nal_units` (46 for the
46-picture hardware HLG test file, 0 for PQ). Source admission still rejects
those NAL types, so a verified output proves it is plain HLG.

`EncoderSession::video_codec()` and `EncodedOutput::video_codec()` return a
separate `VideoCodecInfo` (encoder, profile, pixel format, libavutil color enums,
stream tag and metadata attachment flags), admitted against the contract. It
is separate from `EncoderInfo`/`EncodeReport`; HDR `EncoderInfo.video_profile`
is 2 (HEVC Main10) instead of 100. The optional media-duration correction is
omitted when no correction was needed, preserving those report bytes.
The base C structs keep ABI version 1. HDR uses additive entry points
(`dp_encode_open_hdr`, `dp_encode_finish_hdr`, `dp_encode_query_video`) with
extension structs versioned by `DP_ENCODE_HDR_ABI_VERSION` 1.

`probe::HdrEncoderProbe` mirrors the SDR probe's clocks and audio markers with
known 10-bit codes: black, 0.1, 10, 203 and 1000 cd/m² neutral patches, BT.2020
primaries at reference-white signal level, an ordinal bit strip, a moving
secondary block and a luma ramp. Its PQ contract declares BT.2020/D65
1000 cd/m² mastering and fixed `clli` 1000/203 values for metadata round trips.

On the measured M5 Max (macOS 26.5.2), hardware and OS software HEVC Main10 both
exist and succeed for PQ and HLG, with and without the B-frame request. Unlike
hardware H.264, HEVC B-frame attempts emitted reordered packets with no PTS
before DTS; the rejection remains in place. OS software PQ declared `topleft`
chroma location in its VUI despite left-sited input, while hardware PQ/HLG and
software HLG declared `left`; software PQ therefore needs verifier rejection or a
separate decision. See the
[HEVC Main10 qualification](qualification/hevc-main10-encoding-2026-10-05.md).

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
