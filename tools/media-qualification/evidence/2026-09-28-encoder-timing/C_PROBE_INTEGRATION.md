# Export C probe contract, frozen for parent verification

Build `tools/media-qualification/compatible/export_probe.c` against the pinned
FFmpeg 8 headers/libraries. It includes the original fixture C source. No compiler
or media execution has been run by this worker. Existing fixture commands retain
their defaults and behavior.

CLI (all paths private fixture paths selected by the Python host):

```text
export_probe inventory
export_probe encode OUTPUT_MP4 PACKETS_JSONL MODE EDIT_LISTS FPS_NUM FPS_DEN FRAME_COUNT PCM_KIND
export_probe video INPUT_MP4
export_probe audio INPUT_MP4 PCM_F32 ordinary|manual
export_probe packets INPUT_MP4
```

Modes: `hardware-no-b`, `software-no-b`, `hardware-b`, `software-b`.
Edit-list values: `default` (option omitted), `disabled` (`use_editlist=0`).
PCM kinds: `impulses`, `edges`. Five Stage A calls are hardware-no-b/default,
hardware-no-b/disabled, software-no-b/disabled, software-b/disabled,
hardware-b/disabled, all at 30000 1001 120 impulses.

Admission: frame count 1..240, reduced positive rational frame rate 1..60,
numerator <=60000000, denominator <=1000000, at most 120 seconds/5760000 authored
audio samples. The movie timescale is lcm(fps numerator,48000), admitted only when
it fits signed32. Video is 320x180 YUV420P, video time base [1,fps numerator],
frame duration fps denominator. Audio time base [1,48000]. Audio count is one
origin-based ties-to-even rounding of the full video duration.

Encode stdout is one schema_version=1 object, kind="encode", with:

- `mode`, `edit_lists`, `pcm_kind`, `require_software`, `width`, `height`, `frame_count`,
  `frame_rate:[num,den]`, `time_base:[1,num]`, `start_pts:0`, `duration_ticks`,
  `audio_samples`, `audio_offset_samples:0`, `impulses:[100,min(48000,N/2),N-200]`;
- `requested_b_frames` (0 or2), `requested_gop_frames`, `movie_timescale`,
  `audio_encoder_initial_padding`, `audio_encoder_frame_size`,
  `audio_encoder_trailing_padding`, `encode_ms`, `packet_count`;
- `color_yuv_patches` actual pre-encode five patch centers, and
  `neutral_linear_levels:["0.001","0.01","0.018","0.18","0.5"]`,
  `neutral_yuv_patches` actual pre-encode neutral patch centers;
- `encoder_video` queried time_base/frame_rate/SAR/color/profile/GOP/B properties,
  and `encoder_audio` queried rate/channels/profile/bitrate;
- `mux_options` with movflags, movie_timescale, use_editlist (null for default,
  integer0 for disabled), avoid_negative_ts="disabled", unconsumed_options=0.
  Encoder/mux options fail if unconsumed. Queried codec color/profile enum fields
  are integers; decoded frame interpretation names are strings.

Neutral patches use BT709 OETF (x<.018 ? 4.5x : 1.099*x^.45-.099), then
floor(16+219*OETF+.5), with U/V128. They occupy y64..80; existing numbered
pictures and five color patches are preserved. `edges` adds bounded deterministic
low-level signed content to the first/last2048 samples, explicit nonzero sample0
and sampleN-1, with the same three impulses overriding the background. It does
not add samples or retime the signal. For channel c and absolute sample s, compute
uint32 wrapping `bits=s*1664525+1013904223+c*2246822519`, then
`(((bits>>24)&63)-32)/1024.0f`. Sample0 overrides L/R with .3125/-.28125;
sampleN-1 overrides with .28125/-.25. Each impulse then overrides L/R with
.75/-.65. Edge-content survival is not established merely by detecting these
markers; it needs a separately qualified, unshifted reference comparison.

Each before-mux JSONL entry has kind="packet", stage="before_mux", ordinal,
stream_index, codec_type, codec_time_base, time_base (mux), encoder:{pts,dts,duration},
pts,dts,duration (rescaled mux coordinates), flags,keyframe,size,sha256,skip.
`skip` is null or {leading,trailing,leading_reason,trailing_reason}; it describes
the actual 10-byte packet side data. Entry is flushed before attempting mux, so a
failed packet remains visible. The probe does not repair packet timestamps.

`packets` stdout is schema_version=1/kind="packets", streams metadata and packets
array with the same packet fields in demux coordinates, stage="demux", without
the encoder-time fields. Video packets also report length-prefixed AVC NAL unit
types and IDR presence when safely parsable. Missing/unsupported AVC framing is
explicit, never interpreted as closed GOP success.

`video` stdout is schema_version=1/kind="video", stream/time-base/start/duration
metadata, frames array, frame_count, first_frame_yuv_patches,
first_frame_neutral_patches. Each frame contains pts,best_effort_pts,duration,
authored_identity,keyframe,type,md5,decode_error_flags,flags,sample_aspect_ratio,
interlaced,color_range,color_space,color_transfer,color_primaries,chroma_location.
Metadata names use FFmpeg's names, with "unknown" for absent interpretations.
Fresh-decoder GOP independence remains explicitly unmeasured in this first probe:
`gop_evidence:{fresh_decoder_tested:false,...}`. Frame types/key flags/NAL IDs are
observations only and must not be promoted to a closed-GOP qualification claim.

`audio` stdout is schema_version=1/kind="audio", mode, time_base,
stream_start_pts,stream_duration,initial_padding,trailing_padding,seek_preroll,
first_sample_pts,decoded_samples,decoded_frames,sample_rate,channels,
channel_layout,frames. Every frame contains pts,best_effort_pts,pkt_dts,duration,
nb_samples,sample_offset (offset into emitted interleaved stereo f32),sample_rate,
channels,channel_layout,skip,discard,decode_error_flags,flags. `manual` enables
AV_CODEC_FLAG2_SKIP_MANUAL before decoder open; `ordinary` uses automatic trimming.
Both drain to EOF. Missing timestamps are JSON null. Neither mode shifts origins,
trims emitted PCM based on events, nor repairs gaps; the oracle owns those checks.

Records are bounded; malformed input, cap violations, codec/mux failures and
unconsumed options exit nonzero with stderr diagnostics. The host must retain
exit status plus complete stdout/stderr and packet sidecars even on failure.
Partial JSON on a failed decode is failure evidence, never a successful report.

The output MP4 is reserved exclusively before AVIO opens it; log and PCM files
also require fresh paths. These are private developer fixtures, not a production
descriptor-only media capability. The process exits on a failed native operation;
no claim of in-process recovery or resource reuse follows from this helper.
Read inputs are nonempty regular files up to512MiB; packet payloads up to16MiB;
at most16384 packet records/reads,240 video frames,8192 audio frames and
5760000+8192 decoded audio samples. Decoded PCM is interleaved native-endian f32
on the qualifying Apple Silicon host. Packet/GOP output is explicitly bounded to
64 NAL types per packet; unsupported framing is an unavailable observation.

The only original-helper changes are JSON output to a selected FILE, optional
encoder/decoder configuration callbacks and an optional cumulative decoder packet
budget. Existing commands pass NULL for the new callbacks/budget and retain their
defaults. Parent should smoke their existing encode/video/audio commands. Source
review is complete; compilation, smoke/media runs and sanitizer checks are unrun.
