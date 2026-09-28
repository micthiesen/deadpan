# FFmpeg 8.0.3 AAC priming and H.264 packet framing

Read-only source inspection on 2026-09-28. Source root:
`/tmp/deadpan-ui-ffmpeg/ffmpeg-8.0.3`.
No compiler, decoder, encoder, test, or GUI was run for this inspection.
Line numbers below refer to the files with these SHA-256 hashes.

| File | SHA-256 |
| --- | --- |
| `libavcodec/aacenc.c` | `d8c269567146aed7253eba7d3e8a093d867c8ad61d9a4e0b1fa739cc2e9f2421` |
| `libavcodec/audio_frame_queue.c` | `72305a0a7b9864985a50e814a19fab72977b5bbb70608746caf24193bf97396e` |
| `libavcodec/audiotoolboxenc.c` | `ff2c20bde63ce053e7a40faac59ed71983cbc1cb8042758cb8934c52a61cb85b` |
| `libavcodec/decode.c` | `a152974a940c20ba42f4952f526c1d83abd56a7092014948c2d07c0621acdfdb` |
| `libavformat/movenc.c` | `10eb297eb6e3a3e54167f0da1b76482a130ea2fc876533ff4004152e03558754` |
| `libavformat/mov.c` | `579c72c8fbabc040bc7c0f673de3f6325da765bd45c3c0fd05ea4c181f052ef1` |
| `libavformat/demux.c` | `ef781b57ef67f34d809247653be6bc53488ccb15554a80f3b915af9c0033d74f` |
| `libavformat/nal.c` | `c60ea8de7e69d723ed01e1a16c972147e30525063a564b3e7d5c9d098ec33d9a` |

## Native AAC input and packet clock

- `libavcodec/aacenc.c`, `aac_encode_init` (1181), lines 1191-1192:
  `frame_size=1024`, `initial_padding=1024`.
- `libavcodec/audio_frame_queue.c`, `ff_af_queue_init` (28): initializes
  remaining delay and remaining samples from `initial_padding`.
- The same file, `ff_af_queue_add` (44): the first queued frame includes the
  delay in its duration and subtracts it from its rescaled PTS. Zero-origin
  input therefore queues PTS -1024 for native AAC at 48 kHz.
- The same file, `ff_af_queue_remove` (75): supplies packet PTS and the
  consumed duration. The final packet may have a shorter nominal duration
  than a full AAC decoded block.
- `aacenc.c`, `aac_encode_frame` (824), lines 840 and 1123-1124: uses that
  queue for input and emitted packet timestamps/duration.
- Exact searches of `aacenc.c` found no `AV_PKT_DATA_SKIP_SAMPLES` or
  `trailing_padding` writer. These observations do not imply a universal
  impossibility of other container signalling.

## MOV edit lists and the loss of the negative start

- `libavformat/movenc.c`, `mov_init` (7817), lines 7889-7907: ordinary
  automatic edit-list selection enables edits; disabling edits with automatic
  negative-timestamp policy selects `AVFMT_AVOID_NEG_TS_MAKE_ZERO` unless
  negative CTS offsets are enabled. The probe explicitly selects DISABLED,
  so it does not use that automatic timestamp shift.
- `movenc.c`, `ff_mov_write_packet` (6668), lines 6995-7004: the no-edit/MAKE_ZERO branch
  moves the first sample DTS to zero and can increase its duration. Otherwise
  the first packet DTS is retained internally as `start_dts`.
- `movenc.c`, `mov_write_edts_tag` (4014), lines 4080-4095: for the negative
  start path, `start_ct=-FFMIN(start_dts,0)` identifies the media start and
  the edit duration accounts for the delay.
- `movenc.c`, `mov_write_trak_tag` (4264), lines 4283-4289: emits `edts`
  only when enabled, otherwise warns if a nonzero first DTS needed one.
- `libavformat/mov.c`, `mov_build_index` (4673), lines 4678 and 4690-4736:
  begins the media index at DTS zero; an edit list supplies the offset.
  The simple AAC edit-list path also sets `start_pad` from media start.
- `mov.c`, `mov_fix_index` (4222), lines 4313-4433: the advanced edit-list
  path retains decode preroll, marks out-of-edit samples discarded, and
  accumulates leading sample skips. Lines 4513-4517 set stream start to the
  empty-edit duration, constrain stream duration, and retain `start_pad`.

Inference limited to these implemented paths: omitting the edit list removes
the mapping between the AAC negative packet start and zero presentation time.
It does not remove encoder priming. This matches the parent's measured
default-versus-disabled 60 fps observations, but this note is source evidence,
not an independent execution of that matrix.

## Skip/discard propagation and trailing duration

- `mov.c`, `mov_read_header` (10701), lines 10813-10816: copies AAC `start_pad`
  into demuxer `skip_samples`.
- `libavformat/demux.c`, `read_frame_internal` (1373), lines 1519-1544:
  turns leading skip and, when supplied, terminal discard state into
  `AV_PKT_DATA_SKIP_SAMPLES`. Thus demuxer side data is not evidence that
  the encoder originally supplied packet side data.
- `libavcodec/decode.c`, `ff_decode_frame_props_from_pkt` (1443), lines
  1451 and 1472: propagates skip side data and discard flags to frames.
- `decode.c`, `discard_samples` (316), lines 323-404: ordinary decoding
  applies discard flags, leading skip, and terminal discard counts.
  `AV_CODEC_FLAG2_SKIP_MANUAL` returns early, retaining data and skip metadata.
- `movenc.c`, `get_cluster_duration` (1221) and `mov_write_stts_tag` (3131):
  the final sample duration comes from `track_duration + start_dts` minus
  final packet DTS. A shortened nominal final duration can therefore remain
  in `stts`. It is not equivalent to physically shortening an AAC decoded
  block: the generic decoder's trim path uses the explicit skip/discard
  metadata, not a rule that clamps `nb_samples` to packet duration.
- Exact searches found no `first_discard_sample` or `last_discard_sample`
  assignments in `mov.c`, and no `initial_padding`, `trailing_padding`, or
  `AV_PKT_DATA_SKIP_SAMPLES` serialization in `movenc.c`.

## Actually implemented non-edit-list signalling

- `movenc.c`, `mov_preroll_write_stbl_atoms` (3195), lines 3250-3287:
  AAC gets one `sgpd`/`sbgp` group of type `roll`, distance -1, covering its
  samples. `mov_write_stbl_tag` calls this for AAC and Opus at 3323-3324.
  This is implemented sample-group preroll information, not an arbitrary
  leading/trailing sample-count field. It is independent of `use_editlist`.
- `mov.c`, `mov_read_sgpd` (3773), lines 3793-3801: accepts only `sync`.
  `mov_read_sbgp` (3829), lines 3850-3858: accepts only `rap ` and `sync`.
  This demuxer does not turn its muxer's AAC `roll` grouping into skip state.
- `mov.c`, `mov_read_custom` (5366), lines 5422-5429: recognizes private
  iTunes `iTunSMPB`; parses priming, remainder, and samples, but only assigns
  bounded positive priming to `start_pad`. The parsed remainder is unused.
  No matching `iTunSMPB` writer was found in `movenc.c`.

No sample-exact, non-edit-list AAC priming-and-trailing implementation was
found in these encoder/MOV paths. That is a scoped source finding, not a claim
about every ISO BMFF mechanism or every other player.

## AudioToolbox encoder difference

- `libavcodec/audiotoolboxenc.c`, `ffat_update_ctx` (96), lines 110-115:
  queries `kAudioConverterPrimeInfo` and assigns
  `avctx->initial_padding=prime_info.leadingFrames`.
- Lines 119-131 obtain `mFramesPerPacket`, with 1024 fallback.
- The adapter initializes the same frame queue at 465; `ffat_encode` (516)
  adds input at 546 and removes packet PTS/duration at 574-578.
- Exact searches found no `trailingFrames`, `kAudioConverterPrimeMethod`,
  `trailing_padding`, or skip-side-data handling in the adapter.

AAC_AT can have a different platform-reported leading delay, but these source
paths give no basis for expecting it alone to solve the no-edit-list contract.

## H.264 raw packet hashes

- `libavformat/movenc.c`, `ff_mov_write_packet` (6668), H.264 branch at 6821-6841: non-avcC
  extradata (first byte not 1), with the documented AVC-Intra exception,
  selects NAL reformatting. Ordinary output calls `ff_nal_parse_units` at 6839.
- `libavformat/nal.c`, `nal_parse_units` (74), lines 82-96: finds Annex-B
  start codes, writes a four-byte big-endian NAL size and then the NAL bytes.
  `ff_nal_parse_units` (110) calls that implementation.

Consequently a pre-mux packet SHA and a demuxed MP4 packet SHA need not match
even when their ordered NAL payloads match. Preserve both raw hashes as
observations. A future equivalence assertion would need independently bounded
framing normalization and exact ordered-NAL byte comparison, not an assumption
that every hash difference is harmless.

## Best next consumer experiment

Read the existing `matrix/impulse-60-default.mp4` and
`matrix/impulse-60-disabled.mp4` with headless AVFoundation. Retain unmodified
PCM, exact raw and output buffer timing, trim attachments, and successful full
asset-range reader completion. This changes the consumer without changing the
encoded fixture. Do not shift sample origins, drop packets, infer a trim from
the impulse locations, or replace the retained FFmpeg failure. Native success
would qualify only the measured native consumer path.
