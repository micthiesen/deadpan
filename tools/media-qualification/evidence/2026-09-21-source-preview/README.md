# Source preview evidence

See [the qualification report](../../../../docs/qualification/source-preview-2026-09-21.md)
for measured scope, native interaction results and remaining work.

- `source-manifest.json`: tested source/configuration and fixture hashes on base `86fa1e7`.
- `sources/`: six actual source-index/seek reports and strict JSON index roundtrips.
- `metal.json`: 76 offscreen Metal comparisons, numerical tolerances and limitations.
- `sanitizer/`: final C-adapter ASan/UBSan report and exact compressed log.
- `gate/`: final repository gate plus audio/model harness results.
- `hdr-regression/`: real stream-HDR/SDR-tag fixture inspection and failed-before/passed-after regression.
- `native-ui.json`: native startup, appearance, keyboard/focus, AX and shutdown observations.
- `review.json`: independent review scope, findings, triage and fixes.
- `verification.json`: counts, additional Python checks and initial development failures.

Every `.log.gz` preserves the original log bytes. `source-probe-run.py` records
the exact local probe commands and fixture paths; those paths are developer
scratch locations, not application dependencies. No source video, generated
footage, model weights or user project package is included here. Deterministic
source test fixtures live in `native/deadpan-source/tests/fixtures` with their
own provenance manifest.
