# Semantic frame-cut repeat, 2026-10-02

Plain `.` repeats the last committed `x` or `:delete-frames` operation at the
current Edit cursor. The retained count is the requested count, even if the
first cut stopped early at a group boundary. Each repeat saves one atomic cut,
one Undo entry and a newly captured editable copy. Its register is retained
unless the user chooses another destination, including the default register.

This increment starts at `55a4e5ff45e057916e2906fd170cb661071b6f29`.
The [contract](../SEMANTIC_REPEAT.md) records the exact scope and refusal rules.
SQLite schema 54 and core document schema 43 are unchanged.

## Saved state and review

The service observes the actual SQLite head before publishing and before
admitting a repeat. An exact before/after revision proof replaces the candidate
after a supported cut or preserves it after direct marks and Undo/Redo. Other
saved edits clear it, including headless changes and saves whose workspace
refresh fails. Unsupported edits followed by Undo cannot revive an older cut.
Versioned requests and UI feedback reject stale state. Session replacement,
Close and reopen clear the candidate; a failed Open preserves the current one.

Tests cover fresh cursor resolution, clamping, nested ordinary Sequence bounds,
unsupported ancestry/endpoints, checked overflow, strict serialization, atomic
register/history updates, saved refresh failure, duplicate replies, stale
versions, head-query errors and version exhaustion. Authenticated headless
marks/history preserve intent; dry runs and failures leave saved state intact.
Unknown Compound commands clear it, including mark-only compounds. Broader
semantic classification remains required.

An independent read-only reviewer examined saved-head authority, asynchronous
feedback, refresh failure, history/marks and fresh cursor resolution. The review
found no actionable correctness issue in that scope. Root reviewed the source
and rendered checkpoints.

## Verification

The [retained evidence](../../tools/media-qualification/evidence/2026-10-02-semantic-repeat/README.md)
includes exact commands, source inventories, logs, complete compressed replay
reports and three inspected captures. Checks use Rust 1.97.1, locked dependencies
and `/tmp/deadpan-ui-ffmpeg/prefix` on Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84).

The UI-enabled app suite passes 664 tests plus three headless tests; the default
app build passes 628 plus the same three headless tests. These configurations
overlap and must not be added together. Core and store library suites pass 133
and 75 tests respectively. These runs have no failed or ignored tests. Strict
all-target lint passes for core, store and UI-enabled app; workspace formatting
also passes.

Every final run retained the same complete before/after source inventory:
`ab40b573bd77ff1e277ec11231e51751388d7b58e0204bc4301298bc91203037`.
The initial read-only formatting check overlapped the first app build; later
Cargo and replay commands ran sequentially. Documentation updates during checks
did not change the source inventory.

Three debug visual replays pass 637 checks:

| Scenario | Checks | Evidence |
| --- | ---: | --- |
| `dot-repeat` | 159 | Requested length, new cursor, saved nodes, register overrides, marks/history, unsupported-edit clearing, counts/held keys, focus, text/IME ownership and minimum-size footer paint |
| `delete-range` | 309 | Existing linked range, beat and frame-cut behavior through the changed service path |
| `named-registers` | 169 | Existing register selection, saved copies and placement |

Each replay also passes the same 172,360-case production-router audit over
62 Kestrel globals. Live Swift and the reviewed reservation fixture both hash to
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
These repeated audit counts describe one coverage set, not three independent
sets. All scenarios report their bounded intermediate screenshot allowance;
semantic frames continue and named checkpoints retain reserved image capacity.

The inspected 960×640 captures show the seven-frame hint after a two-frame
terminal cut, the fresh cut at Edit 20 displaying Original frame 27, and a new
three-frame command repeated once despite a held key. Picture, separate clocks,
focus, complete repeat key/label and saved notice remain visible. The final
sequence of Undos restores the original 120-frame baseline.

The initial focused test run compiled and passed 19 tests, with one failure in
a new assertion. A stale authenticated request returns `LiveError` directly;
the test incorrectly expected `Reply::Failed`. The corrected assertion checks
`RevisionConflict`, the actual current revision and absence of a committed
revision. No runtime change was needed. Initial and final source inventories
are retained separately. Existing nonfatal macOS debug linker warnings about
the 16 MB `__eh_frame` limit remain in the logs.

## Limits

Only frame-cut repetition is implemented. Other edit kinds, semantic range/text
selectors, Macro register content, recording and bounded call expansion remain
required for DP-06. Counts before `.` are refused in this increment.

The visual replays use real project commits, decoding and Metal composition,
with injected input. They do not qualify physical keyboard layouts, OS IME
delivery, VoiceOver, device listening, physical display presentation or release
performance. The full workspace suite was not rerun for this scoped change.
No ordinary native window was opened. Replay processes exited after testing;
the QA app remains closed. No DP requirement or Gate A through G is complete.
