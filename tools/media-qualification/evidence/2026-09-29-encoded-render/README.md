# Committed SDR encoding evidence

See [qualification](../../../../docs/qualification/encoded-render-2026-09-29.md)
and `summary.json` for measured scope. The source base is
`4db5e58a30448c70af50c5b07d2234fd1758f15c`, plus the changes bound by the source
inventories and final Cargo artifact records here.

## Actual data

`native-artifacts.tar.gz` retains 663 files, 889,881,304 uncompressed bytes:

- `metal-initial-files/`: actual project MP4 candidates, direct I420/PCM inputs,
  preceding direct/raw-worker fixtures and managed Original bytes.
- `decoded-initial/`: the initial failed qualification and its actual observations.
- `decoded-fixed/` and `decoded-sanitized/`: complete independent decoded planes,
  ordinary/manual FFmpeg PCM, AVFoundation PCM, probe binaries and command logs.
- `project-inputs/`: coherent SQLite backup snapshots of all four project
  fixtures, with their retained media and Generated fixture manifest. Main/WAL/SHM
  files from the original packages were not copied into the archive.

`archive-members.json` identifies every member. `archive-audit.json` records a
complete readback of their sizes and SHA-256 hashes. Reports are also retained
separately as gzip JSON for inspection without extracting the full archive.
Recorded absolute paths identify the original run; map them to these directories
when reproducing it elsewhere. No runtime, user destination or completed export
is installed by extracting this evidence.

## Source and binary admission

`final-artifact-binding.json` proves the final Cargo build returns exactly the
worker and example bytes used in the native run. The worker SHA-256 is
`a3a17a7ef81099a29f9b4292de5d696f7cd7c39bf965766f8cc4c1f7628b1e30`.
The final source inventory is `7d916729…`; its full digest appears in `summary.json`.

`source-changes-since-check.json` records the narrow changes after each check:

- Final Rust source matches the full workspace test and strict Clippy runs.
  The subsequently strengthened Python fault fixture passes the seven native
  host integration tests separately.
- Final qualification Python/C sources match the passing normal and sanitized
  decode runs. All 123 compatible Python tests pass on those sources.
- Two allocation-only changes inside Rust protocol tests followed the first
  native build. Final worker/example hashes remain identical to executed bytes.
- Documentation and retained evidence were added after runtime checks.

The initial Clippy/oracle/aggregation failures are preserved. Review files record
their findings and dispositions; reviewers performed source inspection only.
`next-verifier-boundary.md` is a source-only recommendation for unfinished
production verification. Sanitizer scope is limited to independent C/Objective-C
readers in this run; the unchanged encoder C has separate preceding evidence.

`run-metal.py.txt` records executable selection and before/after hashes.
`package-evidence.py.txt` records archive construction and readback. Check journals
were written by the existing `2026-09-29-generated-pictures/run-native.py` helper.
