# Named project takes

Source base: `886a2a79`, with the named-takes changes. Host: Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1 and the pinned FFmpeg 8.0.3 prefix at
`~/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix`. This advances DP-01;
native recovery of an unreadable database and the remaining recovery audit
are separate work.

## Implemented behavior

The native `:takes` panel and `project takes` / `project take` API share the
same store operations. Save, update, rename and delete change a versioned
catalog without changing the current edit, cursors, registers or Redo.
Opening a take restores its complete authored document as one fresh,
undoable revision. Editing it does not automatically advance the saved take.
Names are unique, case sensitive, trimmed single-line Unicode, with a
128-byte limit and at most 256 entries.

SQLite schema 75 stores the catalog, permanently reserved identities and
immutable restore proofs. Replay checks a restore against its earlier
retained revision independently of the mutable label. Direct generic
`RestoreSnapshot` requests and compound injection are refused. The immutable
Original baseline and qualified-media checks still apply. Restore retires
current generation requests and automatic preparation intentions; accepted
media stays accepted and no old generation job is restarted. Backups,
checkpoints and portable copies retain takes and their history proofs.

## Review and tests

Three independent reviews covered the general change, store/history and
native service/UI boundaries. The general review found duplicate names were
accepted. The store now checks uniqueness during preview/apply and catalog
reading, with a SQLite unique constraint and two regression tests.

Focused workspace tests with the UI feature passed 28 selected tests before
the two uniqueness regressions were added. Coverage includes abandoned
history, deleting a restored take before Redo, stale project/head/catalog/
snapshot requests, transactional rollback, read-only files, immutable
Original identity, catalog capacity, backup replacement, checkpoint/copy,
accepted AI providers, cancelled automatic work, and qualified historical
sound assets absent from the current edit.

The full repository gate passed formatting and strict workspace/UI Clippy.
Its workspace run passed 5,464 of 5,466 tests, with ten existing skips. Two
failures were corrected: the generated command reference lacked `:takes`,
and a pinned corruption test used numeric table selectors whose meaning
changed when the catalog table was added. The reference was regenerated
from the registry and its check passed. The corruption regression now pins
the intended SQL operations and checks that exactly one row changes; the
broader mutation fixture also saves a take and compares the catalog after
validation. All three corruption tests then passed, along with the strict
targeted lint check. All 1,088 UI-feature tests passed (two existing skips),
and both compile-fail documentation tests passed. The gate was completed in
these stages rather than rerunning its already-passing workspace tests.

Final recovery review found that a successful metadata-only take save did
not clear an earlier storage alert. Both native and live-CLI mutations now
record a durable save only when the catalog changed; listing, dry runs and
no-ops do not acknowledge a failure. The actual APFS disk-full test now
verifies that listing keeps the warning and saving a take clears it without
moving the edit head. All 18 focused take/recovery tests and strict UI lint
pass after this correction.

The initial gate also marked one unrelated pure-plan test as leaky:
`projection::intrinsic_policy_is_reclocked_before_rounding_a_hold_with_no_native_point`.
Its assertion passed and its isolated rerun passed without the leak mark.
The final focused run marked three passing tests leaky, including the pure
caption parser; all three passed a serial rerun without leak marks
(`/tmp/deadpan-takes-serial-tests-20261008.log`).
Logs: `/tmp/deadpan-takes-gate-20261008.log` and
`/tmp/deadpan-takes-gate-followup-20261008.log`; the final recovery checks and
visual replay use `/tmp/deadpan-takes-final-checks-20261008.log`.

## Replay and native observations

Three initial visual runs are retained under
`/tmp/deadpan-takes-visual-20261008`, `-2` and `-3`. The first reproduced
duplicate-name acceptance. The second failed a test that required the
native horizontally scrolling name field to show all 128 characters, even
though the separate saved row was fully visible. The assertion now checks
the complete saved row and its selected identity. The third observed
Shift-Tab focus one frame early; it now uses the same settling frame as the
other focus checks. All three passed the production shortcut audit's
21,884,016 cases against 62 Kestrel reservations. The final debug run at
`/tmp/deadpan-takes-visual-final-20261008` passed all 40 take checks and the
shortcut audit. Compact and long-name captures were inspected.

Visual review then changed the native list from durable-ID ordering to
alphabetical names, retaining row identity by take ID. The release rerun
at `/tmp/deadpan-takes-sorted-visual-20261008` caught a real focus-scroll
failure: Tab reached the final long name and Space selected it, but the row
remained outside the scroll viewport. A newly focused row now explicitly
scrolls into view without animation. The corrective release replay at
`/tmp/deadpan-takes-scroll-visual-20261008` passes all 40 take checks and the
shortcut audit. Inspected captures show all controls at 960×640 and the entire
selected 128-byte name in the scrolled alphabetical list at 1280×820.
Formatting and strict UI-feature Clippy pass after this final correction.

The separate release performance run at
`/tmp/deadpan-takes-performance-20261008` passes the same 40 checks and the
shortcut audit without PNG capture. Input-frame CPU p95 is 0.773 ms across
218 inputs (maximum 1.649 ms); complete UI-frame CPU p95 is 0.655 ms across
613 frames. The 23 waits for take-owner replies have p95 6.254 ms and maximum
8.219 ms; those waits start after input dispatch and are not end-to-end save
latency. The two general commit samples are 6.524 and 88.073 ms, too few to
qualify cached-edit latency. No sample failed or timed out. Three independent
debug replay workers were running on this Mac, so this is a small-fixture
measurement under that load, not an idle-machine or physical-display claim.

The full debug replay suite also found a stale Help assertion for `:hold`:
the UI correctly showed its current freeze/black/AI and silence options,
but the test still expected the old black-only example. The expected literal
was corrected and the release `menus` replay passed at
`/tmp/deadpan-takes-menus-visual-20261008`. The initial failure is retained
with the full-suite results.

The full-suite binary has SHA-256
`369ada9d4ae4bf7073b953189b785fb0d1990cbf1b18a1db98f6fbbb294b88ac`;
it precedes the alphabetic-list/focus-scroll correction and the Help assertion
update, which the focused release runs cover. The final 54 changed/new Rust
files have canonical path-to-SHA-256 map digest
`c34caaa2e2ed1c09ff965ae7c0fbaa03811426c0dab4a64565c4e40a2208774c`;
the map is `/tmp/deadpan-takes-source-20261008.json`. The full replay suite
completed in 2,537.760 seconds: 71 scenarios passed, the stale `menus`
assertion failed, and `ai-pause-ready` / `generated-picture` were skipped
because they require explicit project fixtures. Its 7,328 checks and the
initial failure are retained in
`/tmp/deadpan-takes-full-20261008/summary.json`. Together with the corrective
release `menus` and `takes` runs above, this completes the general replay
verification in stages; it does not qualify the two fixture-only scenarios.

Native QA uses a disposable real-media project at
`/tmp/deadpan-takes-native-20261008/session.deadpan`, initialized from
`native/deadpan-source/tests/fixtures/cfr-bframes.mp4` (120 frames,
30000/1001 fps, 320x180). The normal debug executable has SHA-256
`d97e4180fb79e4b606167eb1fdd13f26941ab368a9c7dba288f02949354f0b8b`.
It was copied into a developer wrapper, not a release-qualified bundle.

Accessibility inspection confirmed project opening, Saved status and a
live authenticated CLI take save with the native message
`Updated named takes from the command line.` The current revision stayed
unchanged. Native keyboard/button verification was inconclusive: after
opening the project, the automation did not observe responses to ordinary
editor keys or the Keys button. The same Keys control responded on the
start screen. Normal and UI-harness builds, direct project launch and a
fresh automation binding gave the same result. The event-loop sample was
idle rather than deadlocked, and the live service remained responsive.
Those debug attempts do not establish native take-panel input. Their QA app
and temporary Finder window were closed.

The release retry used executable SHA-256
`fe6adc87f5d60976162cfc496cffc112c9a66bb00504575a7cf7fed38a77fc7e`
in `/tmp/deadpan-takes-native-20261008/release/Deadpan.app`, also a developer
wrapper. After an explicit native text-field focus, command entry opened
`:takes`; typing `Native release first`, Tab and Enter saved it. Accessibility
showed the new row and the saved message. Escape returned to the editor,
Tab then `10l` moved to boundary 10, and `s` split the edit into two beats.
The authenticated catalog confirmed version 2, with that take still naming
the unchanged baseline `ef594984-c045-46a1-a1cf-597c4fa10db4`, while the current
head was `79f52e08-0c90-4bcf-9376-d3862a312b2a`. The retained catalog is
`/tmp/deadpan-takes-native-20261008/release-catalog.json`.

Reopening `:takes` disabled the background, but subsequent Accessibility
observations did not expose the modal or its focus. Native restore is not
claimed; its complete history workflow passes the deterministic replay.
Command-Q closed the owned release app, confirmed by process inspection.
Physical input and VoiceOver speech remain separate owner checks.
