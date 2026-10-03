# Semantic frame-cut repeat evidence

See [scope, results and limits](../../../../docs/qualification/semantic-repeat-2026-10-02.md).

Recorded command JSON includes exact arguments, exit status, starting commit,
tracked diff hash and complete before/after source inventories, including
untracked code. Compressed logs preserve failures and warnings. `SHA256SUMS`
covers every evidence file except itself. Source snapshots distinguish code
changes from concurrent documentation updates.

`semantic-initial` compiled and passed 19 of 20 selected tests. One new test
expected an authenticated stale-revision failure inside `Reply::Failed`; the
API correctly returns `LiveError` directly. The corrected assertion checks
`RevisionConflict`, current revision and no committed revision. This required
no runtime change. `format-initial` predates that assertion correction.

Final app, core/store, lint, format and replay runs qualify the same source
inventory. App configurations overlap and their counts must not be added.
`summary.json` counts every run separately. The app test build also produces
the ordinary binary used by the UI replays and headless integration tests.
No separate rebuild occurred between that build and the replays.

`replay-dot-initial` passed on its first run and qualifies the final source.
The three replay reports are complete JSON compressed with gzip. Only the
three inspected named dot-repeat captures are retained; other image paths in
the reports refer to local scratch output. All replays reached the bounded
intermediate screenshot allowance while continuing every semantic frame and
retaining named checkpoint capacity. This warning is retained, not treated as
a failed behavior check.

The router audit compares the live local Kestrel source to the reviewed
reservation fixture. Repeated audits cover the same 172,360 cases and 62
globals. Visual-mode timing is not release-performance qualification.
Input injection does not establish physical layout, OS IME, VoiceOver or
device-output behavior. No ordinary native app window was opened.

`review.txt` records the independent review scope and conclusion. `cleanup.json`
records terminal replay exits and the final process inventory. Scripts are
retained as `.py.txt` so evidence collection does not change the tested source
inventory. Cargo and replay processes ran sequentially, except the initial
read-only format check overlapped the first app build.
