# Supervised local model adapter qualification

This slice connects a real, pinned MLX keyframe pipeline to Deadpan's Rust
process supervisor, typed lifecycle, exact bridge planner, and contained artifact
snapshot. It is a developer harness. No GUI action, project acceptance,
job recovery, managed model install, or durable generated-media promotion is
implemented by this work. DP-12/13 and Gates A/E remain unfinished.

## Implemented boundary

[`generation_plan.rs`](../../crates/deadpan-jobs/src/generation_plan.rs) chooses
the nearest legal native frame count using exact boundary duration, rounding ties
upward. Requests outside the provider envelope fail before model loading. A
deserialized plan must pass `validate_for` against the chosen capability,
including recomputed nearest-count selection. Sampling is constant-space: output
frame `j` samples native position `(j+1)*(M-1)/(N+1)`. Neither conditioning endpoint
is emitted as an additional output frame. The plan records exact actual/requested
boundary durations, their signed difference, and linear interpolation.

[`qualify_model_worker.rs`](../../crates/deadpan-jobs/examples/qualify_model_worker.rs)
starts one private interpreter with a cleared environment and `-I`. The Python
side checks the same bounded framed protocol, keeps native-library output off
protocol stdout, verifies the pinned model files, and cooperates with
cancellation between denoising steps. The Rust host owns the bounded grace period
and process-group cleanup. Faults are recorded as failed lifecycle states;
cancellation is finalized only after the process is reaped.

After clean exit, Rust snapshots the declared candidate beneath the retained
workspace descriptor. The independent verifier decodes that frozen copy and
checks its full RGB digest, frame count, each presentation timestamp and frame
duration, dimensions, rational rate, stream count, and color metadata. The Rust
harness remains in `Validating`; these measurements do not authorize acceptance.
The native video and provenance sidecar are retained as development outputs.
They do not yet have the candidate's host snapshot/promotion path.

## Runtime and input interpretation

The runtime, q4 pack, and Gemma pins are the same as the
[first generated-file probe](model-smoke-2026-09-20.md). The host-selected private
environment uses Python 3.12.13 and MLX 0.32.2 on Apple M5 Max, 128 GiB, macOS
26.5.2 (25F84), arm64. The selected model file set totals 36,152,862,913 bytes;
the development worker rehashes all selected files on each attempt. This is not
a model manager's verified-install cache or a private runtime distribution.

The [source manifest](../../tools/model-qualification/ltx-source-manifest.json)
records all 139 package Python files (1,305,081 bytes) after comparison with the
upstream pinned Git tree's blob hashes. Runtime preflight verifies every source
file and the installed package roots. Before generation and before completion,
the adapter checks all imported `ltx_*` module files and package search paths
against the exact source-package subtrees, including hashes for lazily imported
modules. A different installation under the checkout's `.venv` is rejected too.
The final run records 102 imported LTX source files. The checked-in manifest is
trusted adapter data, with its own startup hash captured alongside the five
adapter modules. A request or runtime configuration cannot replace it. Runtime
verification checks its SHA-256 entries; the Git blob/tree fields record the
independent comparison made when constructing the manifest. Signed distribution
manifests remain packaging work.

Inputs are frames 7632 and 7633 from *Tears of Steel*, credited to Blender
Foundation, `mango.blender.org`, under the
[published film license](https://mango.blender.org/about/). The source and hashes
are in the earlier evidence. Aspect-preserving Lanczos resize plus black padding
prepares 768×320 PNGs. The adapter retains the pinned model's CRF-33 H.264
conditioning round trip and stage-specific resize/crop. The model never consumes
the prepared PNG values verbatim; that preprocessing is part of provenance.

The source movie has no color tags. This fixture explicitly interprets its
extracted RGB PNGs as sRGB; no validated source color transform is claimed.
Model output is likewise interpreted as full-range SDR sRGB RGB with BT.709
primaries. Native and sampled sequences are encoded as lossless RGB H.264 using
the developer's GPL FFmpeg 9.0.1. Explicit frame parameters preserve range,
matrix, transfer, and primaries tags. This is an adapter serialization policy,
not metadata supplied by the model or a selected shipping/export encoder. See
the [FFmpeg filter documentation](https://ffmpeg.org/ffmpeg-filters.html#setparams-1).

The adapter supports only Bridge + Still at this grid and native 24 fps, with
`8*k+1` native frames in [9,97]. The authored project count remains exact. Native
RGB8 follows upstream clipping/quantization; temporal interpolation uses exact
linear weights in encoded sRGB and half-up RGB8 rounding. No generated audio is
retained. Audio denoising computation remains as provided by the joint model.

## Measured attempts

The final [captured run](../../tools/model-qualification/evidence/2026-09-21-supervised/)
requested 30 project frames at 30000/1001 fps. The model generated 25 native
frames at 24 fps. The retained candidate has exactly 30 frames and 30,030 ticks
at a 1/30,000 time base, or 1.001 seconds. Every frame starts at `j*1001` ticks
and lasts 1001 ticks. There is one video stream and no audio. The prepared
source, full native sequence, and generated media remain outside Git; reports
bind them by hash.

| Measurement | Observed result |
| --- | --- |
| Worker preflight to runtime-loading event | 38.977 s |
| Backend call including imports, generation, encoding, worker RGB checks | 74.991 s |
| Host lifetime through exit/log capture, before artifact snapshot | 114.511 s |
| Independent frozen-file verification | 0.518 s |
| Native file | 25 frames, 4,911,319 bytes |
| Candidate file | 30 frames, 5,023,208 bytes |
| Process peak RSS, Darwin bytes | 14,492,975,104 |
| MLX counter at backend end | 17,767,443,772 bytes |
| Candidate SHA-256 | `d7fcf04e2bbe213d0352153443eb77c4a1534855ab8c19c0ff5dde4f2ddf85d9` |

The sampler chose a one-second native boundary interval for a requested
31,031/30,000-second boundary interval. Its signed deviation is
−1,031/30,000 seconds; only the generated interval is sampled into the requested
duration. The serialized plan retains these exact values.

A separate real attempt cancelled at the first `inference` event. It emitted
`Cancelled` about 2.27 seconds later and exited about 2.65 seconds after the
request, without escalation or an output candidate. A deliberately incorrect
context hash returned `Failed` before runtime/model loading in 0.570 seconds.
That is an expected rejection fixture, not a model-generation failure. These two
fixtures predate the complete LTX source audit and exercise the same process,
protocol, and cancellation code. Earlier development iterations (one preflight
cancellation, one successful 24-frame candidate, and one successful fractional-rate
candidate) are retained separately.

All five adapter-module hashes and the source-manifest hash captured at worker
startup match the recorded implementation. The same native sequence hash occurred
in the three completed development attempts on this machine; that is not a cross-machine determinism
promise. All used cold processes with an unflushed, recently populated file
cache. These few runs do not establish warm p50/p95 or usable holds per minute.

An offline contact-sheet review of all 30 output frames shows preserved scene
composition with a blink and modest expression changes. Its external hash is
recorded. This is initial agent visual evidence, not human acceptability or a
complete temporal/source-seam review.

## Tests and review

Rust tests cover exact and fractional rates, legal-count rounding/ties, bounds,
one-frame holds, overflow, malformed serialized plans, unsupported conditioning,
and consistent-but-non-nearest forged plans. Python standard-library tests cover
cross-language wire shapes, full-size fragmented IO, huge/deep malformed JSON,
input containment, strict numeric typing, exact sampling, and frame timing that
cannot be inferred from average rate alone. The example's tests cover
cancellation/exit reconciliation and terminal failure reporting.

Independent review found and resolved a non-nearest deserialized plan, an empty
dimension grid, Python boolean/integer equivalence, invalid exact-ratio
denominators, a directory-descriptor leak, cancellation at process exit,
nonterminal fault reporting, and avoidable fragmented-read allocation growth.

The repository gate passes: formatting, workspace Clippy with warnings denied,
210 Rust tests, workspace build, and `deadpan-cli doctor`. Python checks pass
20 audio-harness tests and 47 model-harness tests, with no skipped tests. CI also
runs the model protocol/input/sampling tests without a model, GPU, or FFmpeg.

## Remaining qualification

This is one source moment and a narrow development envelope. Full corpus quality,
human acceptability, entry/exit seams, speech preservation, warm distributions,
other precisions and the official 2B baseline, system memory pressure/swap/thermal
behavior, UI interference, private runtime packaging, and model license/install
UX remain open. RSS and the MLX counter are distinct process/runtime measures;
neither is total system unified-memory pressure. Offline flags are not an OS
network-isolation test. Cancellation cannot preempt an arbitrary GPU kernel.
Blocking FFmpeg pipe reads/writes do not poll cancellation; supervisor
termination supplies the bound at those points.

No native UI or keyboard behavior changed, so no GUI test was repeated. The
full editor still needs explicit visual, focus/IME, accessibility, and natural
keyboard navigation review as it becomes interactive. Reproduction commands and
the developer boundary are documented in the
[model qualification directory](../../tools/model-qualification/README.md).
