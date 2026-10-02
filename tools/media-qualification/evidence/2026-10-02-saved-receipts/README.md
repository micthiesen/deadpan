# Saved receipt evidence

See [scope, results and limits](../../../../docs/qualification/saved-receipts-2026-10-02.md).

Each command JSON records its exact arguments, exit status, starting commit,
tracked diff hash and complete before/after source inventory. Inventories include
untracked code. Logs retain failures and warnings; failed runs do not count as
passes. `SHA256SUMS` checksums this directory's evidence except itself.

`receipts-initial` is the missing test-initializer build failure. `receipts` is
the run with three incorrect fixture-duration assertions. `receipts-corrected`
passes all six new tests. The complete `app-tests`, `app-ui-tests`, `fmt`,
`clippy-base` and `clippy-ui` runs verify the final source inventory.

The implementation and checks use the repository's Rust 1.97.1 toolchain and
`/tmp/deadpan-ui-ffmpeg/prefix`. No ordinary native app was started for this
increment. The reviewer performed read-only source inspection and ran no tests.
