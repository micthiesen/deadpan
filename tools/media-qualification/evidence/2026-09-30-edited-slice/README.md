# Edited slice evidence, 2026-09-30

See [qualification](../../../../docs/qualification/edited-slice-2026-09-30.md)
and [contract](../../../../docs/EDITED_SLICES.md).

`summary.json` records measured test counts, host and limits. The fmt, Clippy
and workspace reports retain exact pinned commands, exit codes, times and source
manifests. Logs larger than 10 KB and source manifests use gzip. `manifest.json`
contains SHA-256 hashes of every other retained file.

The focused core/picture/store/CLI runs preceded the final root-sound test.
The complete workspace gate includes that test and all eight decoded-PCM cases.
The collector checked every source hash after the gate. Documentation edits
explain differing diff hashes while the gate source manifest remains identical.

The two early failures are retained: `core-command-wire-failure.log` shows the
RawValue command-buffer failure; `20261001T005612.199721Z-focused.log` shows the
invalid PCM fixture. Their corrections and subsequent passes are documented in
the qualification record and `pcm-verification-summary.txt`. The independent
review result and resolved historical-admission finding are in
`review-summary.md`.

No new UI replay, native window interaction, acoustic measurement or export
qualification was performed for this backend checkpoint.
