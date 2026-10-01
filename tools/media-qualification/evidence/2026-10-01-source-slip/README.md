# Exact Source windows and Slip evidence

Implementation base: `1ea3e4f4b70a805992a56455f8db8d95bf401a7e`.
See [the command contract](../../../../docs/SOURCE_SLIP.md),
[window contract](../../../../docs/SOURCE_EDIT_WINDOWS.md) and
[qualification](../../../../docs/qualification/source-slip-2026-10-01.md).

All 3,114 workspace unit/integration tests, both compile-fail documentation
tests, formatting and strict all-target Clippy with `ui-harness` pass. No tests
failed or were ignored in the final workspace run. Commands use Rust 1.97.1 and
locked dependencies. The final source manifest is
`1fc905d263ff3f486524d866d696a22aab0fc63e401b7ed55f26426fa3bee87b`.

Command reports retain exact commands, durations, exit status and source
manifests. Original failures remain alongside corrected runs; focused runs are
not summed into the workspace result. `runs.json` is derived by `summarize.py`.
`host.json` records hardware, OS, fixture hashes and the no-app process check.
`SHA256SUMS.json` seals the retained evidence files.

The window and Slip reviews are read-only static evidence. Picture/index tests
use known synthetic VFR intervals; PCM tests use the real decoded 48 kHz stereo
fixture with independently authored sample-phase and selected-support oracles.
Store/headless tests use real registered MP4 receipts. Those checks do not
qualify native interaction, display, device output, export or release packaging.
