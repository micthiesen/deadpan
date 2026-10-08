# One-second Extension evidence review

Reviewed 2026-10-08. No inference, decoding, builds or tests were rerun. The review read the saved reports/scripts and recomputed file hashes and equality checks.

## Result and metrics

Both directions completed the separate development experiment and full host qualification. Configuration: Apple M5 Max, 128 GiB, pinned LTX source `3392d75934120b7e69eefbe55893f7ef82be92a4`, `0.15.8+deadpan-extension-dev2`, seed 42107, 30 steps, 768×320, K9/E24, 33 native pictures at 24 fps, 30 authored pictures at 30 fps. The generated and authored intervals are each exactly one nominal second; context is excluded from output.

| Measurement | From left | From right |
| --- | ---: | ---: |
| Supervisor elapsed, seconds | 229.205637 | 293.077061 |
| Whole supervisor command, seconds | 229.258102 | 293.127798 |
| Backend elapsed, seconds | 216.729442 | 280.718117 |
| Host qualification elapsed, seconds | 2.902830 | 2.824220 |
| Whole qualification command, seconds | 2.985456 | 2.904634 |
| Python worker peak RSS, bytes | 14,497,464,320 | 14,498,725,888 |
| MLX peak allocation counter, bytes | 17,022,794,766 | 17,022,794,766 |
| Conditioned join mean absolute RGB8 difference | 1.472220 | 1.305937 |
| Conditioned join gross-cell fraction | 0 | 0 |
| Exact RGB channel comparisons | 68,567,040 | 68,567,040 |

Timing scopes differ deliberately. Backend timing starts before runtime/model loading and ends after encoding/verification, but excludes earlier worker preflight and asset hashing. Supervisor timing includes the worker lifecycle; whole-command timing additionally includes command startup/return. RSS comes from Darwin `getrusage(RUSAGE_SELF).ru_maxrss`, in bytes, for the Python process, not a process-tree sum. `mlx_counter_at_end_bytes` is populated with `mx.get_peak_memory()`, so it is a peak counter read at the end, not current resident memory. RSS and MLX numbers must not be added. The 64 GiB MLX guideline and 80 GiB cooperative RSS check are not proven hard memory containment.

The complete native plan duration is 33/24 = 1.375 seconds. The canonical native Matroska receipt records a measured span of 1.374 seconds on its 1 ms mux time base (last PTS 1333 ms plus 41 ms duration). The sampled master has 30 pictures and measured span exactly 1.000 second. Preserve this nominal-versus-measured distinction.

## What the host actually checked

- Production project-picture capture used saved project/revision/definition clocks and qualified Original inputs. Each capture retained nine context PNGs, a context manifest and continuity signatures. Both cases are at a definition edge with no opposite neighbor.
- Prelaunch retained receipts were compared again during media qualification. `qualify_extension_media` calls the complete `qualify_extension` path: explicit provider/capability binding, worker provenance, canonical native and sampled decoding, generated-only pixel checks, actual conditioned sampled join, Apple Vision observations, pure policy validation and strict stored-envelope round trip against retained context bytes.
- All 23 adjacent generated pairs had lighting measurements. Motion was unavailable for all 23 pairs in each run. The conditioned join also had unavailable motion. Face and mouth checks ran but reported no detected faces and zero measured face frames. The raw landmark batch contains the real anchor observation and all 24 generated observations. Region tracking was not requested. The opposite join is absent. These runs therefore do not prove face, mouth, region or measured motion quality.
- The independent oracle imports neither production sampler nor backend. It independently decodes native, canonical native, worker sampled and host sampled media, then compares frame-center interpolation with integer half-up quantization. Its `exact_pixel_comparisons` field counts RGB channels: `(33 + 30 + 30) × 768 × 320 × 3 = 68,567,040`. Report this as channel comparisons. The generated fetch interval is [9,33) from left and [0,24) from right.
- Source-latent preservation is checked inside the frozen worker with exact array equality and finite checks. The oracle confirms that worker report's shape/offset claim; it does not independently recover model latents. Reported source/native shapes are [1,128,2,10,24] and [1,128,5,10,24], with source starts 0 and 3 respectively.
- The supervisor ends at `Validating`, with `media_validated:false`; the separate media command supplies the full qualification. No Ready persistence, selected candidate acceptance or project mutation occurred. Both before/after documents compare equal and retain their freeze providers.

## Cancellation

The separate real run cancelled at inference entry: stage at 12.860862 seconds, acknowledgement at 13.957145 seconds, exit event at 14.077592 seconds, supervisor completion at 14.092856 seconds. Acknowledgement latency from the observed stage is 1.096283 seconds. Report state is `Cancelled`, clean exit true, no escalation/faults/candidate, and the outputs directory is empty. This does not establish interruption during every GPU kernel or during a later denoising step.

## Attribution and integrity

- All eight frozen adapter/support files match `adapter-sources.json` and the current corresponding source bytes. Both worker provenance source claims match its seven executable/source-manifest entries. Each host envelope embeds the exact worker provenance snapshot bytes.
- Runtime manifest SHA-256 matches `development-manifest.json` and both worker reports. Native/provenance snapshot lengths and SHA-256 hashes match their declarations. All 11 input declarations match their bytes across the worker workspace, prelaunch retained copy and qualified bundle copy in both directions. Qualified receipts equal prelaunch receipts.
- `run_models.py` records seven executable hashes before launching the model/retention/qualification commands. Build commands and successful results are retained in `checks-results.json`. These binaries predate the concurrent pack-schema edits; this experiment cannot validate those later edits. All six `pack-prechange.json` entries match the stated `87cca82f2f023d47a14882fc5c9de4ff1735c6d7` Git bytes/absence.
- At audit time, `retain_extension_conditioning` and `qualify_extension_media` had been rebuilt and no longer matched the recorded run hashes. The capture helper, supervisor, media worker, tracker and CLI still matched. Keep the original executed hashes, rather than replacing them with gate-build hashes. For the archive, retain the Rust capture/source patch against the base plus the pre-pack overlay; `pack-prechange.json` alone describes only the pack files, not every uncommitted source change used by the run.

No contradictory success/failure claims were found. This is one seed and one synthetic fixture per direction, with a separate cancellation run. It supports the explicit K9/E24 one-second development point on this Mac. It does not approve an installable provider, qualify intermediate/longer generated counts, demonstrate normal app allocation, or supply perceptual face/identity quality evidence.
