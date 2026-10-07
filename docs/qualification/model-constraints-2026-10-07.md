# Model manifest constraints, 2026-10-07

This closes the declaration gap found while auditing specification §14.3
after the [packaged update run](followup-2026-10-07.md). It builds on
`25dc67ae`; the implementation is `f0877f9c`. Evidence is
under `/tmp/deadpan-resume-20261006` on the Apple M5 Max, macOS 26.5.2.

## Contract

Both approved packs now require manifest schema 3 and an explicit constraints
record. Signed data updates must preserve the shipped constraints exactly.

| Pack | Declared inputs and limits |
| --- | --- |
| Whisper/Silero | Mono 16 kHz audio; at most 172,800,000 samples; float16/float32 weights; Apple Silicon, macOS 15+, CPU and Metal backends |
| LTX/Gemma bridge | Left/right boundary images and a fixed prompt; 24 fps; `8k+1` native frames, 9–97; 768×320 raster with 64-pixel axis multiples; at most 180 project frames; 4-bit, bfloat16 and float32 weights; Apple Silicon, macOS 26+, Metal |

The values come from the existing worker and preparation contracts. The
LTX safetensors headers contain BF16, F32 and U32 tensor storage; U32 stores
the pinned 4-bit quantized weights. No model files changed. Memory and disk
estimates remain the existing estimates, not newly measured hardware minima.

The host bridge planner reads the declared frame/raster capability. A
packaged runtime also checks the selected pack's minimum macOS. Tests keep
the worker's fixed raster and project-frame cap in agreement with the
manifest. Missing required fields, unknown fields, duplicate declarations,
contradictory operation/conditioning pairs and invalid bounded formulas fail
admission. A valid signature cannot authorize changed constraints without a
new application runtime.

Older development update manifests need regeneration; no migration is added
under the owner's unused-project decision. Baseline file receipts stay valid
because file names, sizes and hashes remain identical. Accepted project media
remains independent of installed packs.

## Verification

- `manifest-constraints-1.log`: 41 focused pack/runtime tests passed in
  3.542 seconds. These include six new tests covering legal plan construction,
  wrong raster refusal, malformed/missing declarations, PCM limits, OS
  boundaries and validly signed incompatible updates without a selection write.
- `model-worker-tests-3.log`: all 66 Python worker tests passed.
- The complete source diff was reviewed for declaration/consumer agreement,
  unchanged file identities, bounded work and refusal before selection.
- `gate-10.log`: strict workspace and UI-harness lint passed, all 4,740
  workspace tests passed in 410.637 seconds, all 1,027 UI-harness tests passed
  in 232.967 seconds, and both doc tests passed. The 10 workspace and two
  UI-harness qualification skips remain explicit. No test failed or reported
  a pipe-close warning.
- `bundle-constraints.log` and `bundle-constraints-verify.log`: a fresh
  722.5 MiB ad hoc signed bundle, with 74 audited Mach-O files, passed every
  positive and negative check from a relocated copy with a scrubbed environment.
  Its CLI imported the baseline AI pack, ran the bundled model smoke test and
  reported AI ready. Tampered/missing helpers, a changed AI worker and missing
  Deno notices were refused. The copied bundle remains at the path printed in
  the log.

## Real signed update and generation

`bridge-constraints-native/summary.json` records a signed schema-3 version-3
qualification update with the baseline's exact model bytes. The packaged app's
SHA-256 is
`6172f8443400efd91c69a382cf79ea12ced0455d5ec61c2b4ce78306fff909c6`.

- Installation and activation passed in 12.717 seconds. Retrying the installed
  version rehashed every file and repeated the real smoke test in 11.771 seconds.
- A 30-frame pause generated a Ready candidate in 78.009 seconds. The stored
  request, host provenance and worker provenance all retained version 3. The
  worker's complete selected-manifest SHA-256 was
  `33d8a85889983da16f091b81fb0bba9808a208968c6857de06f5826d9e8a54e0`.
- The committed fallback was unchanged until explicit acceptance. Rolling the
  model selection back to version 1 preserved the accepted document exactly.
  Asking the old request for another variant refused the changed provider,
  rather than relabeling the request.
- The accepted artifact rendered a fully verified, published movie in
  2.507 seconds after rollback. All run processes exited; no native window
  was opened.

Together with the earlier install, offline archive, failure/resume and native
manager evidence, this closes DP-13 within §29.1. Physical interruption of the
full download and clean-machine acceptance remain on To verify (owner).
