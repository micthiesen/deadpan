# Model manifest constraints, 2026-10-07

This closes the declaration gap found while auditing specification §14.3
after the [packaged update run](followup-2026-10-07.md). It builds on
`25dc67ae`; packaged verification is in progress. Evidence is
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
- A signed schema-3 version-3 qualification update is prepared with the
  baseline's exact model bytes. Its packaged run remains pending.
