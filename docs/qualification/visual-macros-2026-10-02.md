# Visual macro qualification, 2026-10-02

## Scope

Semantic macros now carry oriented Visual selection state alongside the cursor
and selected direct child. Native recording supports `v`, frame and beat motion,
group start/end, range copy/cut and Original or Edited replacement. Programs can
start with an existing selection or build a new one from the invocation cursor.

Empty, absent, forward, backward, extending and finished selections remain
distinct. Yank finishes extension and retains the endpoints. Cut and paste clear
the selection; replacement uses one ordinary `ReplaceSlice` or `ReplaceSource`
leaf. Counted authored runs commit one Compound and one Undo. Copy-only runs
preserve the authored revision, Undo and Redo.

The native service and CLI use the same planner, exact split/import identity
pools and store admission. Final Visual state follows an owned visible receipt;
delayed completion cannot reclaim a selection after context changes. Empty
Visual fast paste and its command aliases now refuse without beat fallback.
The separate `:splice` destination chooser is unchanged.

Core schema remains 43 and database schema remains 55. This advances DP-05,
DP-06 and DP-21. No requirement or product gate is complete.

## Environment and evidence

- Base commit: `6765452f1cbc08031846679be8871a8de80aee62`.
- Apple M5 Max, 128 GiB RAM, macOS 26.5.2 (25F84), arm64.
- Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1 (`c980f4866`), locked dependencies.
- FFmpeg development prefix: `/tmp/deadpan-ui-ffmpeg/prefix`.
- Final source inventory SHA-256: `a5954d5c9edc66a84b10e46850dda3bf6c132e64cdc45df35089d978cf9278d7`.
- Executed UI binary SHA-256: `f36f25abcf726be4dfc21141dcd73cede6ef6af111703ab78fc955d12e93a364`.

The [evidence directory](../../tools/media-qualification/evidence/2026-10-02-visual-macros)
retains commands, complete compressed logs, before/after source inventories,
peer review notes and replay results. Failed attempts are retained. Scratch
project packages and authenticated discovery data are excluded.

## Automated checks

| Check | Result |
| --- | --- |
| Core and store suites | 1,470 passed across 81 suites; no failures or ignored tests |
| CLI suites after fixture correction | 360 passed across 18 suites; no failures or ignored tests |
| Default-feature app portion of the affected-crate run | 664 unit and 4 headless tests passed |
| App with `ui-harness` | 700 unit and 4 headless tests passed |
| Strict all-target Clippy across affected crates | Passed |
| Strict app Clippy with `ui-harness` | Passed |
| Workspace formatting and final UI build | Passed |
| Rendered Macro workflow | 288 checks passed across 1,317 steps |
| Production Kestrel compatibility | 487,568 routing cases, 62 reserved bindings, no conflicts or source drift |

Coverage includes:

- Oriented, empty, absent and invalid selection contexts; finished ranges
  independent of the cursor; native-equivalent beat and scope-boundary motion,
  including empty siblings.
- Staged range capture, replacement, cut and paste; exact source/capture
  provenance, disjoint identity pools and complete document inverse equality.
- Frozen counted call bodies, late failure, work limits and strict wire parsing.
- Bank-only copies preserve Redo and survive checkpoint/reopen. Authored range
  edits restore completely through one Undo and fresh-revision Redo.
- Qualified Original replacement retains the exact measured Source mapping.
- Native and live-owner dry-run/commit paths retain exact receipts and copies
  through preview-refresh failure without inventing cursor ownership.

## Rendered workflow and visual review

The replay uses the qualified `cfr-bframes.mp4` fixture, real project service,
native event routing and Metal picture pipeline. It verifies recording and
counted execution of forward copy/replacement, backward cuts and qualified
Original replacement; exact displayed source frames; one Undo; existing and
empty selections; beat/group motions; and selection-only results without writes.
Both active and finished empty selections refuse `p`, `P`, `:paste` and
`:paste-before` without changing document, bank or history.

One scenario withholds a genuine bank-only result, changes only the Visual
extension state, then returns it to its entry value. The saved copy is published,
but its delayed completion cannot finish the independently retained selection.
Only delivery timing and UI selection observations are injected.

Root inspected these 960 by 640 captures:

- [Recorded range copy](../../tools/media-qualification/evidence/2026-10-02-visual-macros/screenshots/macros-127.png):
  `[20,24)` remains visibly selected; three instructions are recorded; Escape
  teaches clearing the selection, and the picture shows Original frame 24.
- [Recorded replacement](../../tools/media-qualification/evidence/2026-10-02-visual-macros/screenshots/macros-128.png):
  the inserted four-frame group is selected at Edit 30, the picture shows
  Original frame 20, and nine recorded instructions remain visible.
- [Counted Edited replacement](../../tools/media-qualification/evidence/2026-10-02-visual-macros/screenshots/macros-129.png):
  the final group is at Edit 60 with Original frame 40; five editable beats span
  124 frames and the completion message reports one Undo.
- [Counted Original replacement](../../tools/media-qualification/evidence/2026-10-02-visual-macros/screenshots/macros-130.png):
  Edit 40 displays Original frame 10, with the inserted three-frame Source
  selected and its retained suffix visible.
- [Selection-only result](../../tools/media-qualification/evidence/2026-10-02-visual-macros/screenshots/macros-131.png):
  the backward selection spans `[0,50)`, the cursor is at zero and the full
  120-frame baseline remains unchanged.

Recording hints, selected ranges, both clocks and footer controls remain visible.
The replay reached its intermediate screenshot allowance; semantic checks and
reserved named captures continued. Kestrel source SHA-256 was
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
The final process check found no `deadpan-app` running.

## Review and corrected failures

Peers cross-reviewed core planning, CLI/native propagation, staged copy
provenance, input capture, delayed receipt ownership and rendered expectations.
Root reviewed and integrated the diff. Peer review covered the assigned
boundaries within the existing team.

Review found that Serde internally tagged unit variants can ignore unknown
fields despite `deny_unknown_fields`. The three new instructions without fields
use the existing explicit `deserialize_empty` pattern. Tests reject unknown
fields on all three.

Initial compilation found an ambiguous `.into()` error type and a missing
`json!` qualification in a headless test. It also reported an unnecessary
mutable test binding. These were fixed explicitly, without suppressions.
No tests are counted from that failed build.

The next affected-crate run passed the app suites, then failed the new CLI
range test. After replacement, the imported Sequence occupied `[14,20)`, but
the test tried to capture `[14,16)`. The existing reducer correctly refused an
endpoint inside that composite. The corrected fixture captures the complete
wrapper and explicitly checks its `[14,20)` range. Its later intentional call
failure now exercises whole-run rollback. Production endpoint rules did not
change for this correction.

The core/store run used source inventory
`1c3021450b5e4153545af8047bb06cf9a98ee2ecbee50fc438831ff5b9d8f31c`.
The subsequent CLI and UI-feature runs used
`a5954d5c9edc66a84b10e46850dda3bf6c132e64cdc45df35089d978cf9278d7`.
Only the CLI fixture and the native late-completion message changed between
those inventories. Core/store sources stayed identical. The message now names
both cursor and selection when an abandoned completion is refused.

## Limits

Ordinary Sequence range reducers still refuse endpoints inside composite
children. Enter their group or select complete boundaries. Full operator
grammar, semantic text/role selectors, temporal occurrence editing, additional
Macro instructions and broader dot-repeat remain required.

Scripted native event replay and Metal rendering do not qualify physical
keyboard layouts, OS IME, VoiceOver, physical display color or acoustic output.
Debug linking retains the existing `__eh_frame` size warning. This increment
makes no release-build, packaging or performance claim.
