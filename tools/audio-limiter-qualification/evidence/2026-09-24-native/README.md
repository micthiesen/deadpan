# Native finite-limiter evidence

`experiments.tar.gz` retains the finite limiter's candidate runs, compiled PCM
audits, admitted pipeline timing, exact bus-cache comparison, reviews and the
final source snapshot. See the [qualification report](../../../../docs/qualification/audio-limited-2026-09-24.md).
This finite-bank evidence does not establish physical-DAC peaks, listening
quality, a complete future mix graph or device deadlines.

**Compiler correction:** the first native audit used Homebrew Rust 1.98.0, despite
its original README saying 1.97.1. Its environment and embedded binary compiler
path expose that error. Those originals remain unchanged. A distinct replay in
`native-audit-rust1971/provenance-1790247077077703000` explicitly selected and
verified cargo/rustc 1.97.1 and the root release profile. All 25 PCM files and 25
gain files equal the earlier run. Both retained admitted-pipeline binaries were
independently confirmed to use 1.97.1; their measurements need no compiler correction.

The pinned replay passes the same 25-case exact BS4 and pinned BH96/32 audits,
13 cold/shuffled parity checks and 14 unchanged unity controls. All its bytes also
match `candidate/triggered17`. The retained six stronger sinc failures therefore
still apply. Their stored diagnostic upper estimates and separate 80-digit point
witnesses remain explicitly stronger failures, not finite-bank successes. The
archive verifier checks their byte relationships without running a new filter.

Retained directories:

- `candidate`: initial 19/25 finite passes, target17 and triggered17 full 25-case
  runs, exact configuration/source, coefficients, PCM/gains and failures.
- `native-audit`: original 1.98 run, retained compiler misattribution, independent
  audit and an interrupted earlier build that produced no PCM.
- `native-audit-rust1971`: corrected pinned-toolchain replay, exact source/compiler
  checks and 50-file equality comparison.
- `pipeline-timing` and `pipeline-timing-cached`: original signed16 pipeline and
  final exact-range cache run. All 135 matched result hashes join to retained PCM,
  gains, measured indexes and input WAVs; all 15 complete artifact files match.
  The earlier float-WAV admission refusals remain included.
- `kernel-timing` and `kernel-timing-root-profile`: separately labelled default
  and matching-root-profile observations, not admitted full-pipeline measurements.
- `sanitizer-final`: passing ASan/UBSan ABI probe, exact commands/source hashes,
  six stronger point witnesses, and the explicitly failed preliminary witness
  helper attempt (`KeyError: gain_knots`).
- `design-review`, `native-review`, `bus-cache`, `fft-map`: reviews and source
  probes. `compiler-provenance-review.md` records the actual compiler audit.
- `final-source`: exact workspace manifests, lock, Rust/native sources and fixtures
  needed to reproduce the final integration. Retained host paths identify the
  original run; adapt copied scratch manifests when reproducing elsewhere.

The cached hot pipeline's ten batches after the context-alignment transition
averaged 144.925 ms, maximum 150.872 ms, for 170.667 ms of audio. The transition still
took 174.395 ms; cold seeks remain slower. The baseline hot complete-context mean
was 182.343 ms. Machine load differed, so wall times alone do not isolate the cache's
effect. The reduction from 261 to 44 source admissions per steady ordinary/hot batch
directly confirms reuse. Prefill and genuine starvation handling remain required.

Verify without extraction, DSP execution or measurement reruns:

```sh
python3 tools/audio-limiter-qualification/evidence/2026-09-24-native/verify.py
```

`retention.json` inventories original byte lengths, source paths and SHA-256.
The archive uses sorted regular tar entries, fixed metadata and gzip time 0.
`verification.json` records the verifier result; `SHA256SUMS` seals surrounding
files except itself. `retain.py` recreates the same tar from unchanged source
bytes and refuses overwrites. The seal proves retained identity, not a generally
reproducible compiler binary or a fresh numerical certificate.

If retention stops before writing its manifest, the orphan archive is a failed
seal. Preserve that attempt and run a copy of `retain.py` in a new scratch
directory after resolving the failure; the collector intentionally refuses to
overwrite either output.

Extract only into a new scratch directory. Run copied harnesses/scripts with new
output paths, exact recorded dependencies/profile and explicit compiler selection;
several original experimental scripts overwrite their own results by design.
Cargo targets, executables, object files, dSYM bundles and Python caches are
excluded. Executable hashes/compiler paths remain recorded. Reviewed metadata
contains necessary build/hardware context and generated fixtures, with no
credentials or unrelated personal content. Workspace gates and native GUI
evidence are retained separately by the parent session.
