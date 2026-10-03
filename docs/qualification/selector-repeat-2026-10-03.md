# Cut selector repeat qualification, 2026-10-03

## Scope

Plain `.` now repeats the last supported picture cut: frame count, frame/beat/
group-boundary motion, explicit selected beat or Visual range. It captures a
fresh editing context. An active or finished Visual selection overrides the
retained selector; empty selections refuse, and saved Visual cuts require a new
selection. Requested counts survive group-end clipping. Whole-beat repeats use
the selected child's identity, including adjacent empty children at one boundary.

Repeat retains the destination register unless the user supplies a one-shot
override, including the unnamed copy. Failed attempts consume that override
without changing saved contents or the repeat candidate. Repeated cuts save one
atomic history entry and exact new historical capture. Recording dot stores its
effective instruction. Recording `:delete` now distinguishes captured whole
children from Visual ranges.

Native dot uses the shared semantic Apply planner with a captured repeat version.
The service re-observes the actual saved head and compares the predicted complete
instruction before planning. Direct supported Apply cuts prove intent after
commit and before refresh. Exact retries return their saved receipt without
reinstalling old intent. Named Run transitions remain unproved; bank-only work
preserves the candidate. Legacy cuts retain their independent saved-cut receipts
and validate their exact selector/capture pairs.

Core schema remains 43 and SQLite remains 55. This advances DP-06. Other edit
kinds, semantic text/role/occurrence selectors and full product acceptance remain
open. No DP requirement or product gate is complete.

## Environment and evidence

- Base commit: `16da6ff6c0c6e1042103f5a77bef7af112367801`.
- Apple M5 Max, 128 GiB RAM, macOS 26.5.2 (25F84), arm64.
- Rust and Cargo 1.97.1, locked dependencies.
- FFmpeg development prefix: `/tmp/deadpan-ui-ffmpeg/prefix`.
- Final source inventory: `e78b89d445034b861df961f78db173acf91523407625af444af9b44857426282`.
- Final binary SHA-256: `ba1fd5b00c30bf57e60cbcd6fcc434331d76b2fc2049e053236479f62bbb8149`.

The [evidence directory](../../tools/media-qualification/evidence/2026-10-03-selector-repeat)
retains command receipts/logs, full source inventories, binary identity, review
notes, rendered reports and representative screenshots. Test project packages
remain in task scratch space. Sources were unchanged during each recorded check.

## Verification

The app suite passed 714 tests and its headless integration suite passed 4.
Eight new service tests cover fresh targets, Visual precedence, empty children,
register overrides, stale or forged intent, exact retries, bank-only preservation,
Run invalidation, refresh failure and full document Undo/Redo comparisons.

The initial rendered runs passed 382 dot-repeat, 372 Macro and 309 range-deletion
checks. They use real projects, the production router, service transactions and
the Metal picture pipeline. Dot coverage includes the genuine deferred receipt,
the full 1,024-instruction recording limit, requested counts after clipping,
active/finished Visual selection, named/default overrides, exact durable copies,
new child identities, recording and Undo.

The initial runs used source inventory
`400bcb7c30acb189082a1290b1c1b195244adb1bfca2a62dd9ec05211c8e6c76`.
Final source changes only local missing/empty Visual feedback, the exact error
assertions in that replay, and history labels for frame and whole-beat Apply
cuts. Macro and range-deletion semantics are unchanged.

| Final check | Result |
| --- | --- |
| App with `ui-harness`, including headless integration | 718 tests passed |
| All-target Clippy, default and `ui-harness` | Both passed with `-D warnings` |
| Formatting and UI build | Passed |
| Final dot-repeat replay | 381 checks passed across 1,030 steps |
| Production Kestrel compatibility | 2,748,336 routing cases, 62 bindings, no conflicts or live-source drift |
| Native key delivery, Visual refusal and Undo | Passed; app closed cleanly |

The final dot run omits the initial run's optional retained-project check; no
behavior check was removed. Macro, range-deletion and final dot runs total
1,062 workflow checks. Each audit matched live Kestrel source SHA-256
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
The replays retain the documented intermediate-screenshot allowance warning;
semantic checks and named captures continued.

## Rendered and native observations

The inspected [960 by 640 replay capture](../../tools/media-qualification/evidence/2026-10-03-selector-repeat/screenshots/dot-repeat-122.png)
shows the exact `[35,40)` copy, Edit join 35, 115-frame edit, Original picture 40
and complete “repeat cut 5f backward” footer hint. Its text paint and footer
anchoring checks pass.

Native CUA used a private copy of the closed replay project, including its two
empty-child fixtures. Keyboard `20l`, `"a`, `d5h`, Undo, `gg40l` and `.` produced
the same [repeat result](../../tools/media-qualification/evidence/2026-10-03-selector-repeat/screenshots/native-repeat-final.png).
Undo followed by `gg60l`, `v3h` and `.` cut `[57,60)`, leaving 117 frames and
Original picture 60 at the join. Pressing dot without another range preserved
that edit and showed the [direct selection prompt](../../tools/media-qualification/evidence/2026-10-03-selector-repeat/screenshots/native-refusal-final.png).
Undo restored 120 frames. The final native run used the final source/binary above;
Cmd+Q exited successfully and `pgrep -x deadpan-app` found no running instance.
The developer wrapper still depends on build-host libraries and is not a release
qualification.

## Review and retained failures

An independent reviewer checked UI context capture, register consumption,
recording, receipt ownership, service admission, retry behavior and proof timing.
Two UI findings were fixed: the dot hint was hidden for current Visual selections,
and early refusal behind pending macro work or a full recording left the register
override armed. The replay now checks both production-action refusals, including
delivery of the original pending Run receipt and the exact saved full draft.

Native review found missing-selection feedback exposed the shared planner's
phrase “using this macro instruction.” Dot now asks directly for a new Visual
range or a nonempty selection. The service still performs authoritative admission.
The reviewer checked this final change and found no actionable issue.

The initial build failed because the new harness imported `RegisterBank` from
the store crate root instead of `deadpan_store::registers`. After that correction,
713 tests passed and one existing teaching assertion failed because it expected
frame-cut-only wording. The assertion now requires the current Visual/beat/motion
scope and remaining-edit limitation. Its custom-path, count, held-input and
shortcut-audit checks remain. Both failing logs are retained.

The debug build reports the existing large `__eh_frame` linker warning. This
increment does not qualify release packaging, physical layouts, CJK IME,
VoiceOver, performance or repeat for other edit kinds.
