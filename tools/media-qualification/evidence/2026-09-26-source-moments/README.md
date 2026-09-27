# Original moment verification

This record belongs to [the qualification](../../../../docs/qualification/source-moments-2026-09-26.md)
and the local changes above base `c03a5edde5f28d27074745eb15711cb28b1f2e50`.
The prior captured-framing increment is present. No Git commit or push occurred
because this session makes Git metadata read-only.

`verification.json` and `gate-2/report.json` record the final required gate:

- Formatting, all-target Clippy, locked workspace build and doctor passed.
- Complete workspace tests: 1,408 passed, one failed, zero ignored.
- The sole failure was socket fixture creation in
  `deadpan-jobs/tests/artifact.rs:200`. The sandbox denied `UnixListener::bind`
  with `PermissionDenied`, OS error 1. `--no-fail-fast` continued through all
  other suites, the enabled worker example and doctests. No test was skipped.
- All 433 source/configuration hashes remained unchanged across the run.

The failed test keeps the overall gate failed. It does not indicate that the
socket rejection path was exercised successfully. No live GUI, GPU, device,
listening or export claim is made by these checks.

`gate-1` retains an earlier Clippy failure for a deliberately reversed range
literal. The fixture now constructs the same invalid input from endpoint pairs.
`focused-1` retains three old-schema fixture failures caused by missing required
`overrides` maps. `focused-2` contains the corrected legacy, measured media,
migration and historical audio-context runs. `render-focused` retains the audio
test helper's stale revision failure and its successful corrected rerun.
The final full gate reruns all those tests together.

`review.json` records the independent general, migration and exact-audio reviews.
All returned no findings. `old-binary/source-selection-commands.json` is the
actual core19/database25 fixture-producer command log. The old binary and scratch
media are not distributed here; their provenance and resulting fixture hashes
live beside the store migration fixture.

The [ImageGen moment-reuse board](../../../../docs/design/boards/original-moment-reuse-v1.png)
is a separately inspected future interface target. Its exact prompt, dimensions
and byte hashes live in the design manifest. Native Visual selection, registers
and atomic selected-moment splicing remain open.

`sha256.json` covers every retained evidence file except itself. The runner and
retention scripts preserve the commands and paths used on this host; they are
development evidence, not end-user runtime requirements.
