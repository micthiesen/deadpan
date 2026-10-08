# One-second video extension

Both Extension directions now run from actual project-context capture through
the real local model and complete host qualification for a one-second Hold.
The independent pixel oracle passes, cancellation exits cleanly and both saved
projects retain their original silent freeze. This is a development measurement;
the approved model catalog and normal app generation remain Bridge-only.

## Implemented contract

`prepare_extension_scoped_with_provider` takes an explicit plan and independently
selected capability. It checks the plan before opening the project and checks
the saved Hold's exact duration and project rate before decoding. It uses the
same immutable project pictures, full structural support, conservative shot
guard, captured geometry and retained signature evidence as the earlier capture.
The existing 768×320 raster, 64-context-picture bound, 16 MiB aggregate image
budget and shared 120-second deadline remain. The ordinary development entry
point retains its K9/E8, at-most-one-third-second contract.

The new `prepare_extension_context` example writes a private, new workspace
with relative input references and a completion report written last. It does
not approve a provider, allocate a job or change the project.

Adapter `0.15.8+deadpan-extension-dev2` permits K9/E24 at 768×320/24 fps, with
an authored interval no longer than one second. It derives the source, added
and total latent counts as 2, 3 and 5. Dev1 retains counts 2, 1 and 3. The
request and selected model manifest must agree on the one runtime identity;
cross-identity counts fail before loading MLX. Both directions retain the
source latents exactly and discard generated audio.

Pack schema 4 now has a separate `extension_hold` operation, chronological
video conditioning and `ExtensionConstraints`. The exact duration ceiling is
floored onto each project frame grid and combined with the independent output
allocation bound. An out-of-range request fails without changing authored time.
The operation promises both directions and requires Metal. Existing approved
manifest values and serialized bytes are unchanged. An Extension data update
requires a compiled baseline and preserves the complete runtime, capability,
resource and file contract. No Extension catalog entry was added.

## Actual model measurement

Hardware: this Apple M5 Max, 128 GiB unified memory, macOS 26.5.2. The pinned
LTX source revision is `3392d75934120b7e69eefbe55893f7ef82be92a4`, using the
existing verified q4 model and Gemma weights. Their revisions, licenses and
file hashes remain in the retained manifests and worker provenance. Both runs
used seed 42107, Still, 30 steps and a subprocess denied all network access.

Each fresh project contains a qualified synthetic moving-square Original and
an adjacent 30-frame silent Freeze Hold at 30 fps. Production capture supplies
nine chronological inputs from that project, including exact source identities
and continuity signatures. Each real inference emits 33 native pictures:
nine context plus 24 generated pictures. Generated-only sampling produces
exactly 30 output pictures and a one-second sampled movie. The opposite
neighbor is absent in both fixtures.

| Measurement | FromLeft | FromRight |
| --- | ---: | ---: |
| Supervisor command wall time | 229.258 s | 293.128 s |
| Backend wall time | 216.729 s | 280.718 s |
| Full host qualification command | 2.985 s | 2.905 s |
| Process peak RSS | 14,497,464,320 bytes | 14,498,725,888 bytes |
| MLX peak allocation counter | 17,022,794,766 bytes | 17,022,794,766 bytes |
| Exact RGB channel comparisons | 68,567,040 | 68,567,040 |

RSS is for the Python worker; the MLX counter is a separate allocation measure.
Do not add them. The nominal native interval is 33/24 = 1.375 seconds; its
canonical Matroska span measures 1.374 seconds on the 1 ms mux clock. The
sampled span measures exactly 1.000 second.

The host qualifier verifies the immutable request/provider/input binding,
strict worker provenance, canonical native and sampled movies, pixel and
geometry reports, then round-trips the retained envelope. The separate oracle
decodes the worker and host outputs and uses independent integer frame-center
arithmetic with half-up channel rounding. Every compared channel agrees;
sampling fetches zero context pictures. Retained inputs are byte-identical
after generation. Exact before/after project dumps also agree.

All 23 generated transitions were inspected in each run. Lighting and the
conditioned join pass their pixel checks. Motion is unavailable for all 23
pairs; face geometry and mouth checks report `no_face_detected`. No region
target was selected, and the opposite seam is absent. These unavailable checks
do not count as positive quality evidence.

A third real job was cancelled at Inference entry. It acknowledged cancellation
about 1.096 seconds after that stage report and exited with status 0, without
escalation, a candidate or output files. Total supervisor time was about 14.093
seconds, including loading and conditioning. Neither successful generation nor
cancellation accepted or changed a Hold.

## Checks and retained evidence

The focused capture/saved-picture run passes all 18 tests after correcting an
old test fixture that supplied a Bridge runtime identity to an Extension reader.
The model-pack constraint/update run passes all 23 tests, including eight new
Extension cases. All 117 Python tests pass. Independent reviews of the adapter,
capture/example and pack constraints found no remaining defects.

The repository gate completed in parts. Formatting and strict workspace/UI
Clippy pass, and all 5,412 workspace tests pass. The UI run passed 1,075 of
1,076 tests; one cancellation test consumed a terminal command reply and then
waited for another update. Independent review confirmed the harness race.
The corrected helper checks the initial reply first; both cancellation tests
retain their state assertions and explicitly require `Outcome::Cancelled`.
No runtime behavior or timeout changed. All 35 generation-service tests then
pass in each of the ordinary and UI configurations, as do repeated formatting,
strict app/UI Clippy and both workspace doctests. The full runs skip 10 workspace
and two UI tests. Passing tests emit inherited-pipe warnings whose cause is
unproven; the exact warnings and initial failure are retained.

Retained [evidence](../../tools/model-qualification/evidence/2026-10-08-extension-one-second/results.json)
includes exact commands, source/helper hashes, synthetic Originals, requests, project dumps, captured
inputs, host/worker reports, full-pixel oracles, cancellation and compressed
check logs. Large movie bytes remain at the recorded local scratch paths with
their hashes. The executed helpers were built from `87cca82f` plus the capture
changes before the pack-schema changes. Python sources were frozen before
inference. The final gate covers the integrated source; it does not retroactively
attribute the real runs to later pack-schema code.

## Remaining work

Normal CLI/app worker allocation, runtime selection and smoke checks, an
independently approved Extension pack, native controls and longer 2/3-second
measurements remain. This single one-second point does not qualify every legal
count, motion setting or frame rate, and the measured latency is not a Fast
claim. This increment does not run a packaged app, inspect a native window,
accept these real outputs or verify their encoded export. Saved acceptance and
model-free reopening have separate prior evidence. Real-person quality and
threshold calibration remain on To verify (owner). DP-12 remains Partial.
