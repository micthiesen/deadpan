# Read-only control evidence

See the [qualification record](../../../../docs/qualification/readonly-controls-2026-10-08.md).

- `results-*.json` and logs retain every command and exit, including failed
  early assertions and the full app suite's runner leak warning.
- `replay-1-report.json.gz` and `replay-2-report.json.gz` retain failed replays.
  `replay-6-report.json.gz` is the final 50-check pass, with actual controls,
  semantic frames, timings and verification limits. Replay 5's passing summary
  is retained; replay 6 adds fresh-worker proof for both cleanup previews.
- `source-sha256.json` records app source identity; the replay reports and
  summaries identify the exact executed binaries independently of Git HEAD.
- `verify.py.txt` is the final command runner. Its `/tmp` output directory must
  be changed before a rerun because evidence files use exclusive creation.

Large raw reports and the package-byte assertion failure are gzip-compressed
without altering their contents. Intermediate PNG captures are not retained.
