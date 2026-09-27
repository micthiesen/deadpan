# Repeat-gap clock verification

This record belongs to [the qualification](../../../../docs/qualification/gap-clocks-2026-09-26.md)
and the local changes above base `c03a5edde5f28d27074745eb15711cb28b1f2e50`.
The earlier captured-framing and exact Original-moment changes remain present.
Git metadata is read-only in this session, so no commit or push occurred.

`verification.json` and `gate-1/report.json` record the final required checks:

- Formatting, all-target Clippy with warnings denied, locked workspace build
  and CLI doctor passed.
- Complete workspace tests: 1,430 passed, one failed, zero ignored.
- The failure was socket fixture creation at
  `deadpan-jobs/tests/artifact.rs:200`. The sandbox denied `UnixListener::bind`
  with `PermissionDenied`, OS error 1. Its socket rejection assertion therefore
  was not exercised. `--no-fail-fast` continued through every other suite and
  documentation test. No test was skipped or suppressed.
- All 437 source/configuration hashes remained unchanged across the run.

The failed test keeps the overall gate failed. Headless checks here do not
qualify live GUI behavior, GPU output, device audio, listening or export.
Core 20/database 26 and audio-context schema 2 are unchanged by this increment.

Focused final results comprise 110 unique cases: 34 core, 34 plan, 29 audio and
13 CLI audio-inspection tests. `focused-1` retains a failed fixture which tried
to construct a forbidden zero-duration Hold. The corrected test in `focused-2`
asserts its rejection at document admission; production validation was not
relaxed. The final workspace run repeats all these corrected cases together.

`review.json` records independent general and exact-audio reviews. Both returned
no findings. The root review also checked actual-media historical reads, source
admission and the absence of an invented preceding-play identity for a gap
definition.

The [moment-reuse ImageGen target](../../../../docs/design/boards/original-moment-reuse-v1.png)
remains the intended interface reference. This increment has no native UI
changes. Authored gap bindings, compact resume dispatch, atomic selected-moment
reuse and its native Visual/register workflow remain open.

`sha256.json` covers every retained evidence file except itself. Runner and
retention scripts preserve the commands and paths used on this host. They are
development evidence, not end-user runtime requirements.
