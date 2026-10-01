# Atomic MoveRange evidence

Core/headless relocation of a current linked range within or between ordinary
Sequence parents. [Qualification](../../../../docs/qualification/atomic-move-2026-09-30.md)
describes behavior, review corrections and limits. Native Move controls and
two-site picture/audition interaction are not implemented by this checkpoint.

All 2,792 locked workspace tests pass with none failed or ignored, in 1,399.00
seconds. Final formatting passes in 1.43 seconds and strict all-target workspace
Clippy with the UI harness passes in 629.35 seconds. This directory retains:

- Per-command JSON with the exact invocation, base commit, tracked diff hash,
  source manifest, start time, process exit and monotonic duration.
- Full command logs, compressed where large, including the initial compile,
  invalid-fixture and module-order failures.
- Source manifests covering tracked and untracked Rust/native/config inputs.
- Independent design/implementation reviews and the three owners' handoffs.
  Execution addenda distinguish the parent's checks from agent-only inspection.
- `summary.json` with measured test totals and host/toolchain, and `manifest.json`
  with hashes of every retained evidence file.

The stable final source manifest is
`6609d6d091868ae4b71a87a039419bf61209ed4de868f28faf5754e27975e484`.
It lists 1,338 inputs; the collector rechecked each byte identity before delivery.
The source set remained frozen during the final gate. Documentation changes are
not source inputs. Earlier commands keep their distinct manifests and failure exits.

The shared command recorder is
[`run-native.py`](../2026-09-29-generated-pictures/run-native.py), with
`DEADPAN_CHECK_OUTPUT` set to the task's scratch directory. Every Cargo command
uses `rustup run 1.97.1`; native media uses the pinned development FFmpeg prefix.

Picture tests assert exact canonical plan coordinates. Audio tests decode the
retained WAV fixture and check canonical PCM. The accepted-generated fixture is
synthetic acceptance/history evidence. No decoded generated-picture, GPU,
physical window, audio device, acoustic or export qualification is claimed.
The user's separate Deadpan app/window/Space was untouched.
