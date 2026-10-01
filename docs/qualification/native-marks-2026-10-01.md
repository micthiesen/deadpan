# Native marks and jump history, 2026-10-01

This increment adds persistent letter marks, exact Original/Edit jumps, a native
Marks list and bounded back/forward navigation. The implementation base is
`291853145eb434864cdf84dcbbab92c81881ff73`. See
[the behavior contract](../MARK_NAVIGATION.md) for identity, clocks, asynchronous
admission and the current history limits. Core 34/database 43 remain unchanged.
The [retained evidence](../../tools/media-qualification/evidence/2026-10-01-native-marks/README.md)
includes source manifests, corrected failures, rendered captures, native database
backups and complete verification scripts.

## Review and corrections

Independent service and UI reviews covered identity, receipt delivery, routing,
focus and revision handling. The accepted corrections are:

- Every mark intent clears an older generic edit receipt, including Jump and
  rejected Set. The independent durable mark and cut receipts remain separate.
- A native letter ID with a different label rejects Set, Jump and Delete.
  Exact ID plus exact letter remains an intentional headless/native address.
- Equal-time bound fragments must resolve to the same accessible group/child.
  A distinct target rejects rather than choosing the first binding's context.
- Expired Edit history entries are removed from both branches so older valid
  Original positions remain reachable. Mark-only revisions explicitly rebase.
- Exact fractional positions survive pane-only changes. Actual movement clears
  that retained position.
- Ctrl-O/Ctrl-I work inside the Marks list as its controls advertise.

Meaningful regressions cover each service correction. The new source fixture
checks nonuniform PTS and the exact terminal Original boundary without a timeline
occurrence. Other tests cover nested/empty scopes, Split, Move, Repeat wrapping,
lost content, fractional Retime, stale identity, request deduplication, failed
post-save refresh, reopening and reversible deletion. Pure state tests cover
bounded history, branching, expiration and mark-only rebasing.

## Production UI replay

The final focused debug run passes **156 mark checks** plus the production
**16,368-case Kestrel audit** against all 62 current reservations. The local
Kestrel source SHA-256 is
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.

The replay uses actual project writes, exact anchor resolution, source decoding
and Metal rendering. It exercises visible pending prefixes, case-sensitive
letters, preserved Visual selection, back/forward branching, changed letters,
Original/Edit clocks, missing marks, delayed replies, held keys, undoable removal,
marks following a ripple cut and persistence after reopening. The complete
52-letter list is populated through production command entry. Native Tab reveals
the final off-screen row at 960×640 and wraps in both directions without leaving
the modal. Tab/Enter saves and removes a focused letter. Ctrl-O/Ctrl-I work in
the modal after unrelated revisions expire Edit history.

Inspected captures at 960×640 and 1280×820 show the full instructions, captured
clock, separate lowercase/uppercase rows, readable errors and focused controls.
The Marks entry button sits beside the focus status; it does not crowd the
editing shortcut row. The default pending-prefix capture retains a 257-point
picture. Stable modal frames require one layout pass. The final-row capture
shows the focused Remove Z button completely visible after keyboard scrolling.

The debug capture allowance reaches its bounded intermediate-image limit;
semantic frames continue and named checkpoints remain retained. This is the
only finding in the passing visual report. Delayed service delivery and IME
events are explicitly injected. This does not qualify physical IME, non-US
layouts, VoiceOver or all editor workflows.

## Corrected diagnostic failures

All attempts retain their commands, source manifests, exit codes and logs.

- The first replay build failed because its paint check called a nonexistent
  helper. It now uses the existing complete text-visibility inspection.
- The first visual attempt checked a cursor after the service became idle but
  before the UI consumed its reply. It now waits for the matched UI request to
  finish as well.
- The second attempt injected a repeated key without first holding it down.
  egui correctly treated it as an initial press. The corrected witness presses
  and holds h before entering m, then repeats h and verifies that m remains pending.
- The third attempt expected an Original history entry that its scripted
  sequence had never created. It now actually departs Original through a mark
  jump before testing revision expiration. History semantics were unchanged.
- One integrated test's unscoped-Repeat fixture used a descriptive label at a
  reserved native ID and consequently hit the new collision check first. Its
  exact letter label now reaches and verifies the intended occurrence rejection.

## Automated gates

All commands use Rust 1.97.1, locked dependencies and the pinned qualified FFmpeg
prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

| Gate | Result |
| --- | --- |
| UI-feature app and headless tests | 445 + 3 passed; none failed or ignored. |
| Default app and headless tests | 409 + 3 passed; none failed or ignored. |
| Workspace/all-target Clippy with UI harness | Passed with `-D warnings`. |
| Workspace formatting | Passed. |
| Focused rendered marks | 156 checks passed plus Kestrel audit. |
| Release build | Passed in 103.96 seconds. |
| Full release replay | 3,589 checks passed across 21 app scenarios plus Kestrel audit; 19.63 seconds. |

The UI-feature test source manifest is
`58728a6de59f5bd27cac771e0cba00c34cc5fb220625bdc3056bc359e50cfbe2`.
The later default tests, lint, formatting and focused rendered run use
`9f81d48542fa16934f3aabafe0af4f2d5b1a3511d47b343c704936442c786ea8`.
Only `preview/harness/marks.rs` changed between these manifests: it adds the
52-letter traversal and stable-layout checks. The later replay executes that
changed code. Unchanged backend crates retain the preceding 2,883-test workspace
gate; this increment does not claim a new full-workspace test run.

The full release run has no findings, failed samples or timed-out samples. Warm
navigation, cached Repeat and Hold picture completion p95 is respectively
**1.523 / 5.886 / 5.914 ms** on Apple M5 Max, 128 GiB memory, macOS 26.5.2
(25F84). These are application replay measurements, not physical display latency.
The accepted-generated-picture scenario needs its separate explicit fixture and
is not qualified by this run. Native audio/IME/accessibility limits remain.

## Native keyboard and saved-state verification

The isolated developer bundle contains exactly the release binary, SHA-256
`800b72e9dee6b3e5a53e384466a0e08b61ab5dcbcdc9cd63ec0914e074c72959`.
Native keys saved q at Edit 20 in a fragment, moved to Edit 40, and returned with
`'q`, Ctrl-O and Ctrl-I. The decoded picture at Edit 20 showed source slate 030,
matching the fixture's ten-frame cut. `mQ` saved Original ordinal 5 independently,
with exact PTS 5005 at time base 1/30000. Both marks selected the correct clock.

Native Tab focused the modal's Save button. Ctrl-O inside the list closed it and
returned to Edit 20; Ctrl-I returned to Original 5. Opening and cancelling the
list, and both navigation directions, changed none of the 20 database tables.
`:unmark q` saved exactly one removal, and one Undo restored all mark fields.
All nonmark authored fields and 16 unrelated tables remained unchanged. The seven
consistent SQLite backups record revision/history counts of 13/10 initially,
14/11 after q, 15/12 after Q, 16/13 after removal and 17/13 after Undo.

After a native quit/restart, both marks returned to their saved positions without
changing any table. Both QA processes exited 0. The final native inventory and
process scan found no Deadpan app, and a nonblocking exclusive lock acquisition
confirmed that the project writer lock was released. The developer wrapper
retains external build-host libraries and is not release packaging.

Debug linking retains the existing `__eh_frame` size warning. The passing Clippy
run has no warnings. No mark feature changes audio, picture composition or export.
Full editing grammar, transient-history rebasing through structural changes,
occurrence-level navigation and complete physical-input/accessibility acceptance
remain open. DP-05, DP-20 and all release gates remain partial or open.
