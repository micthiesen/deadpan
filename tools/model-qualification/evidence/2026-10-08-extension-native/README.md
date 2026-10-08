# Native Extension controls and packaging evidence

This collection retains completed checks from
`/private/tmp/deadpan-extension-native-20261008`. All eight production Extension
cases reached Ready, passed the independent sampling oracle, and completed
explicit acceptance, undo/redo, portable-copy and offline-export verification.
Earlier failed checks remain alongside their corrected passes.

## Production result

- [`production/results.json`](production/results.json) records eight Ready
  cases: 12, 24, 48 and 72 output frames at 24 fps in both directions, with
  nine context frames at 768 × 320. The 12-frame cases sample eight generated
  frames; the other durations use 24, 48 and 72 generated frames.
- [`production/measurement-summary.json`](production/measurement-summary.json)
  retains timings, worker RSS, system samples, latent preservation, timing
  conversions and measured/unavailable quality fields. Every Ready result left
  the authored document unchanged. The opposite seam is absent in these edge
  fixtures. Each duration/direction has one attempt.
- [`production/oracle-results.json`](production/oracle-results.json) records
  exact RGB-channel comparisons for every sampled picture against an independent
  integer implementation of the interpolation rule, with zero context fetches.
  Per-case `oracle.json.gz` files include every sampling position.
- [`acceptance/results.json`](acceptance/results.json) is a compact extract of
  all eight passing acceptance cases. Each has 27 successful steps, including
  explicit acceptance, undo/redo, cold validation, portable copy, automatic
  render, sampled picture comparisons and silent-audio checks. The full batch
  summary remains in `acceptance/summary.json.gz`.
- Each portable export ran with the source project, bundled AI runtime, default
  model root, development model cache and outbound IP denied. Positive and
  negative probes establish that the denials were effective. The emitted-file
  receipts retain movie/report hashes and report `contains_generated_pictures`.
  Silence does not establish an audible synchronization offset.
- `production/runtime-fix-results.json` is the complete ten-command batch,
  including the matrix, oracle and acceptance/export checks, all exit code 0.
  Its logs and the individual production receipts are retained unchanged.
- [`native-ax.json`](native-ax.json) records the packaged app's Accessibility API
  check on the accepted 72-frame Extend-right project: accepted/saved operation
  and moderate motion, focused Generation controls prefilled with
  `generate mode=extend-right motion=moderate target=none`, Escape restoring
  Normal, and Command-Q exiting 0. The document still matched the retained
  acceptance receipt. The timing disclosure was below the returned viewport
  and was not inspected. No screenshot or physical-input claim is made.

## Completed evidence

- `verification-results.json` records the first check batch: formatting,
  strict Clippy, catalog fuzzing, 1,080 passing native tests (2 skipped and
  1 reported leaky), doctests, and 123 passing Python tests. Its replay batch
  failed and stopped the runner. Raw logs are in `logs/`.
- `replays/` preserves the initial five-scenario batch, including passing
  `ai-variants` and `ai-compare`, and failing `ai-pause`, `ai-extension`, and
  `model-packs`. `replays-2/` records all three corrected scenarios passing.
  Layout retry warnings remain in the records. The Extension replay uses a
  scripted V3 worker with the real qualifier; it proves native lifecycle and
  controls, not model quality.
- Each replay `*.checks.json.gz` retains the original metadata, every check,
  findings, skips, and timings. Only the large per-frame `steps` arrays are
  omitted. Full report paths, byte counts and SHA-256 hashes are retained.
  Both batch `summary.json` files are exact copies.
- `remaining-results.json` preserves the corrected replay run and the first
  release bundle's failure to admit `adapter_sources_sha256` in its strict
  runtime-check report. Both model imports and the resulting doctor check
  failed. The original failure log is retained.
- `completed-runtime-checks.json` preserves the seven-command extract collected
  before the matrix: formatting, CLI Clippy, 18 runtime/model tests, the gain
  regression, the second release bundle, successful positive and negative
  bundle verification, and production Extension pack import. This is explicitly
  a completed-command extract; the complete final batch is under `production/`.
- The two `retained-mode-test` logs preserve a transitional compile failure
  and the corrected single-test pass. Other targeted test skips remain skips.

The final bundle is ad hoc signed. Its build provenance retains the AI runtime's
GPL distribution limitation. These receipts do not establish a clean second
Mac, a new licensing decision, human output quality, or a latency distribution.
The footage is synthetic. Face geometry and mouth motion were unavailable;
motion coverage is partial or unavailable. There was no interactive editing
workload during the serial matrix. Separate worker processes and uncontrolled
OS caches do not establish warm-runtime latency. Worker RSS and sampled system
state do not establish peak unified-memory pressure.

## Source and artifact identity

- `source/bundle-2-source.json.gz` expands byte-for-byte to the final pre-bundle
  source inventory: base commit and hashes of 5,907 source/configuration files.
- `bundle/build-provenance.json` is the exact final bundle build record.
- `executed-binary.json` identifies the packaged CLI used by the model runner.
  Each replay's metadata and summary identify its separately executed binary.
  The earlier automated checks and replays must not be described as a rerun
  against the final release executable.
- `environment.json` extracts the recorded Mac/toolchain/fixture information.
- `inventory.json` lists exact copy origins, raw hashes, compressed encodings,
  and local-only artifact references. Source patches, full replay traces,
  binaries, models and media are not copied. No partial live result is retained.
- `acceptance/archive-inventory.json` maps every archive member to its original
  source path, raw byte count and SHA-256. Each deterministic case archive
  contains the original uncompressed text bytes of 85 receipts. Compression
  preserves empty stderr files, command logs, sandbox profiles and export reports.
  Archives were reconstructed twice and every member was verified before removing
  only the redundant per-case gzip copies. Production per-case receipts remain
  individually compressed.
- `manifest.json` hashes every retained file except itself.

## Reproducible scripts

`scripts/qualify.py`, `oracle.py`, `verify-accepted.py`, and
`summarize-results.py` are exact scratch snapshots. The three verification
orchestrators are also retained. Their completed outputs are retained above.
`cases.json` fixes the eight duration/direction inputs; results are separate.

The scripts resolve their input/output root from their own location. To rerun,
copy them into a fresh scratch qualification directory beside the case inputs;
do not execute them in this evidence directory. `qualify.py` refuses to replace
an existing generation. `verify-accepted.py` requires all eight completed results
and mutates only the owned fixture projects through explicit acceptance,
undo/redo, portable-copy and offline export checks.

`scripts/collect-final.py` records the original final collection;
`scripts/compact-final-evidence.py` records deterministic archive construction,
roundtrip verification and inventory updates. Do not rerun either script in this
finished evidence directory. Extract a case archive with, for example,
`tar -xzf acceptance/12f-from_left.tar.gz -C <empty-directory>`.

`validation.json` records JSON, gzip, tar-member, Python syntax, original-copy
hash and credential-marker checks. Packaging used only file reads/writes,
extraction, compression, hashes and syntax parsing. It launched no native app,
model, Cargo or inference process. No private signing key or unrelated research
was copied.
