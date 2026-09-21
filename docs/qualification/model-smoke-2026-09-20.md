# First LTX MLX generated-file probe

On 2026-09-20, the pinned LTX MLX candidate completed a real local generation on
the reference Apple M5 Max with 128 GiB RAM, macOS 26.5.2 (25F84), arm64. It
produced an H.264 file that decoded to exactly 25 silent 768×320 frames at 24 fps.
This is a single generated-file smoke check. It does not complete DP-12/13 or
Gates A/E, select a shipped model, or establish usable holds per minute.

## Pinned inputs and runtime

- Runtime: `dgrauet/ltx-2-mlx` commit
  `3392d75934120b7e69eefbe55893f7ef82be92a4`, package version 0.15.8.
- LTX-2.3 q4 pack: `56a5866d638ecfe37c54d348e88938235185c2d4`.
- Gemma 3 12B 4-bit text encoder: `86cc6a8dedbc456dd0e4af01a9d09f396f77e558`.
- Private development environment: Python 3.12.13, MLX/MLX Metal 0.32.2,
  mlx-lm 0.31.1, mlx-arsenal 0.2.4, and the captured upstream lockfile.
- Selected files total 36,152,862,913 bytes. All 12 large binary/tokenizer files
  matched the pinned repository SHA-256 values. Small metadata files matched
  their Git blob hashes; their SHA-256 values are recorded too. This file set
  includes audio decoder assets even though this probe disables audio output.

The [evidence directory](../../tools/model-qualification/evidence/2026-09-20-smoke/)
contains exact receipts, source/harness hashes, runtime inventory, and raw
results. The original code uses task-local paths and is retained as executed,
not presented as a portable product tool. Neither runtime nor weights have been
selected for redistribution. Runtime MIT, LTX-2 community terms, and Gemma terms
are separate license layers.

The source is *Tears of Steel*, attributed to Blender Foundation,
`mango.blender.org`, under its [published film license](https://mango.blender.org/about/).
The original movie remains intact locally, including credits. Video-only input
frames 7632 and 7633 were extracted from the 24 fps source. The separate
soundtrack has different terms and is not used in generated clips. Source,
frame, and output hashes are in the evidence. Media binaries are not in Git.

## Executed configuration and results

The probe used two-stage keyframe interpolation, 20 guided first-stage steps,
3 second-stage steps, seed 1, and guidance scale 3.0. The output grid was
768×320 with 25 total model frames and conditioning at indices 0 and 24.
It used the dev transformer plus distilled LoRA, without low-RAM streaming.
Low-memory component cleanup remained enabled. The adapter explicitly disabled
audio output and tokenizer remote-code trust and required local tokenizer files.
Hub/Transformers offline and telemetry-disable flags were set; this was not an
OS network-isolation test.

| Measurement | Observed result |
| --- | --- |
| Generation call, including component loading and encode | 90.968 s |
| Python process work including imports/setup | 95.403 s |
| Entire supervised command, including launcher/exit observation | 97.440 s |
| Process peak RSS, Darwin `ru_maxrss` | 14,506,835,968 bytes |
| Highest 2-second RSS sample | 14,506,819,584 bytes |
| Swap before and after | 0 bytes |
| Decoded output | 25 frames, 768×320, 24/1 fps, yuv420p, H.264 |
| Audio streams | 0 |
| File size | 54,092 bytes |
| Output SHA-256 | `54c06089b7cc8602bb4b93762317aa6540ca3d8a9dec1d3753b2518c93861b57` |

This was a cold process with recently downloaded files; the OS file cache was
not flushed. There is no warm-repeat distribution or p50/p95 estimate. The
64 GiB MLX memory setting is a guideline, not a hard cap. A separate monitor
would terminate the attempt after 30 minutes or observed process RSS above
80 GiB; neither guard fired.

The raw report's `peak_mlx_bytes` value of 8,966,382,168 is **not** a whole-run
GPU or unified-memory peak. The upstream decoder resets that counter before
VAE decode. The normalized validation names it an end-stage counter; neither
that value nor process RSS proves total system memory pressure or thermal load.

All output frames decoded. A contact-sheet review found the face and background
broadly preserved, with a small blink and modest expression changes. This is
an agent's initial visual observation, not human acceptability scoring or a
complete temporal/seam review.

An independent review re-probed the output, inspected the contact sheet, verified
the captured-file hashes, and checked the upstream memory-counter resets. The
contact sheet remains outside Git; its hash is recorded in `validation.json`.

## Failed or unqualified properties

The file has no explicit range, matrix, transfer, or primaries tags. That is an
output-integration defect to resolve before product adoption. The developer
Homebrew FFmpeg 9.0.1 GPL build encoded/probed this sample; it is not the
qualified LGPL application bundle.

The 25 frames include model endpoints. They are not 25 authored interior Hold
frames, and the probe did not test source joins, speech preservation, explicit
acceptance, project history, or the Rust worker/artifact adapters end to end.
Those boundaries have their own tests, but this raw Python probe is not their
application integration.

Still required: the 30-moment rights-cleared corpus and all requested hold
durations, warm/cold repeat measurements including failures, q8/BF16 comparison,
the official distilled 2B MPS baseline, exact interior sampling, media/color
validation, human acceptability, UI/preview interference, thermal/power behavior,
private runtime packaging, model license/install UX, and clean-machine evidence.
The single cold sample cannot be compared directly with the provisional warm
two-second-in-60-seconds target.
