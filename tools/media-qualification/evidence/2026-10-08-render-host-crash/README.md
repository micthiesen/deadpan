# Real Render host-kill evidence

See the [qualification record](../../../../docs/qualification/render-host-crash-2026-10-08.md).

- `run-4/report.json` records both passing host SIGKILL cases and the retry.
  Case directories retain exact progress, identities, process observations,
  checkpoint and publication evidence. The published `retried.mp4` is retained.
- `run-1/` is the failed fixture setup: production admission rejected runtime
  environment overrides before starting a worker.
- Logs/results retain all four attempts, including two compile failures in the
  evidence harness. Final results: strict Clippy, three gate tests, five example
  tests, build, fresh project initialization and two real crash cases passed.
- `original.mp4` is the exact generated input. `inputs-4.json`,
  `source-sha256.json` and case reports identify source and executed binaries.
- `verify.py.txt` is the command runner; replace its private fixture/output paths
  for a new run. Its evidence files use exclusive creation.

The large executed binaries and scratch project database are omitted. Their
hashes and before/after authored database cells are retained. No performance,
physical power-loss or native-window result is claimed.
