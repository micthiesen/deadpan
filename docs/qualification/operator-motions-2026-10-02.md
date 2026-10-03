# Copy and cut motion qualification, 2026-10-02

## Scope

Normal Edit `y` and `d` accept frame, beat and group-boundary motions. `yy`
copies the explicit selected beat; `dd` cuts it. A motion selects the half-open
interval from the captured cursor to the same destination as native navigation.
Copies preserve cursor and beat selection. Range cuts select the join; whole
beat cuts select the literal following sibling, including empty structures.
Original and Visual `y` remain immediate.

One positive distance count may precede the operator or its frame/beat motion,
such as `5dl` or `d5l`. Conflicting counts, zero, overflow and misplaced counts
refuse without dispatching their remaining keys as fresh navigation. Whole-beat
operators accept only absent or one counts; group-boundary motions refuse
counts. The router composes configured operator and motion aliases, subject to
the existing 16-key complete-path bound.

Native single operations, recorded instructions and headless requests share
typed `SemanticSelector` and `SemanticMotion` planning. Admission recaptures
each trace's exact historical Child or Range target. Copy-only operations save
the register bank without changing authored history. Cuts and counted authored
macros use one reversible Compound transaction.

Operator prefixes capture session, revision, bank, scope, cursor, selected
child, Visual state, pane and register. Context changes permanently invalidate
the pending capture, even after returning to the same position. Escape revokes
pending cursor ownership while an already queued save remains durable.

Core schema remains 43 and database schema remains 55. This advances DP-05,
DP-06 and DP-21. No requirement or product gate is complete.

## Environment and evidence

- Base commit: `3a011e7bab8ebf7093886d2e9b909f61954ba290`.
- Apple M5 Max, 128 GiB RAM, macOS 26.5.2 (25F84), arm64.
- Rust 1.97.1 and Cargo 1.97.1, locked dependencies.
- FFmpeg development prefix: `/tmp/deadpan-ui-ffmpeg/prefix`.
- Final source inventory SHA-256: `ab0f81bbbd48485425a209082aabc4db8486e0b2fce93973578352d68392482d`.
- Final executed binary SHA-256: `30a57f907c33544b3cfa2c839b59f230b0e59aa56f0e5c2519edf1ea7225d585`.

The [evidence directory](../../tools/media-qualification/evidence/2026-10-02-operator-motions)
retains exact source inventories, binary identity, complete command logs,
retained failures, review notes and rendered captures. Scratch project packages
are excluded. Final validation matched all 1,560 source inputs to the recorded
inventory, verified retained evidence hashes and checked 751 local Markdown
links in the changed documents.

## Automated checks

| Check | Result |
| --- | --- |
| Core and store suites | 1,478 passed across 81 suites; no failures or ignored tests |
| App with `ui-harness` | 706 unit and 4 headless tests passed |
| CLI unit and integration suites | 361 passed across 17 suites; no failures or ignored tests |
| Strict all-target Clippy across affected crates | Passed |
| Final app Clippy with default features and `ui-harness` | Both passed |
| Formatting and final UI build | Passed |
| Final Macro replay | 372 checks passed across 1,796 steps |
| Final configurable-keymap replay | 56 checks passed across 89 steps |
| Editing / slice placement / range deletion / named registers | 40 / 819 / 309 / 169 checks passed |
| Production Kestrel compatibility | 2,748,336 routing cases, 62 reserved bindings; no conflicts or source drift |

The CLI results come from the corrected affected-crate run with
source inventory `506caa0a7a7997ca195eb953573ab74579878b4ba9d45dfc8ed29c640b4e6308`.
That run later stopped on the core test-helper failure described below, before
doctests. Core/store passed with inventory
`cdb572a773d09dc2e7aa327d55edd6ab66b0583e4a3c3bfcbb0ef30fb776a3b0`.
Only `crates/deadpan-core/src/semantic/planner/tests/selectors.rs` differs between
those inventories. The four regression replays and initial full lint checks used
the core/store inventory. The final source inventory changes only four arrow
labels in `navigation/editor_map.rs` to readable key names. App tests, both app
lint configurations, formatting, build, Macro/keymap replays and native delivery
were rerun after that display correction. Core/store and CLI sources stayed
identical.

The initial Macro replay passed 373 checks. Its sole additional check verified
the explicitly requested retained project package; the final replay used
temporary project storage. No behavior check was removed. Final Macro and
keymap replays used live Kestrel source SHA-256
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.

## Rendered and native review

The replays use the qualified `cfr-bframes.mp4` fixture, real project service,
production router and Metal picture pipeline. Coverage includes frame, beat
and group-boundary selectors; selected and empty children; recorded and counted
operators; exact copied intervals, cut joins and displayed Original frames;
empty/conflicting-count refusal; latched stale prefixes; Escape with a genuine
saved result withheld; and saved-refresh failure preserving absent selection.

Root inspected the final 960 by 640 captures:

- [Pending copy](../../tools/media-qualification/evidence/2026-10-02-operator-motions/screenshots/macros-132.png):
  cursor 20, unchanged 120-frame edit, named register and valid continuations.
- [Backward cut](../../tools/media-qualification/evidence/2026-10-02-operator-motions/screenshots/macros-133.png):
  copied `[15,20)`, join 15, 115-frame edit and displayed Original frame 20.
- [Counted Macro](../../tools/media-qualification/evidence/2026-10-02-operator-motions/screenshots/macros-134.png):
  final cursor 40, 116-frame edit, Original frame 44 and one-Undo feedback.

The pending replay capture retains egui's debug `request_discard` warning after
three consecutive input frames changed footer layout. The capture itself and
adjacent settled frames use one layout pass. This is retained evidence, not a
performance qualification. The replay's intermediate screenshot allowance also
produced its documented warning; semantic checks and named captures continued.

Native CUA key delivery used a private copy of the closed retained project.
`20l`, `y5l`, `d5h`, `u` and `yy` completed successfully. Copy kept cursor 20;
cut selected boundary 15 in a 115-frame edit; Undo restored 120 frames.
The [final pending window](../../tools/media-qualification/evidence/2026-10-02-operator-motions/screenshots/native-pending-final.png)
shows readable Left/Right/Up/Down hints and both clocks at 2560 by 1704 pixels.
The [native cut result](../../tools/media-qualification/evidence/2026-10-02-operator-motions/screenshots/native-cut-final.png)
shows the exact copied interval and Original frame 20 at the join. The native
captures have no debug warning overlay. Cmd+Q closed both test instances cleanly;
the final process check found no `deadpan-app` running.

## Review and corrected failures

Peers cross-reviewed the core selectors, router and native receipt lifecycle.
Review found that Escape outside recording left pending Apply cursor ownership
intact, and that a failed workspace refresh could infer a selected child where
the captured selection was absent. Both were fixed. Rendered scenarios cover a
genuine withheld cut receipt and a genuine saved copy with an injected refresh
failure observation. Service tests independently exercise actual refresh
failures. Root also corrected whole-child cut continuation to use the literal
sibling slot rather than a time-only lookup that skipped empty siblings.

The initial affected-crate run passed 705 native tests and failed one footer
fixture before rendering. Its configured 16-key motion became a 17-key path
when combined with the operator. The fixture now uses 15-key motion aliases
and retains 16-key non-motion aliases. Production path limits did not change.

The next run passed the native and CLI suites, then failed a new core test
helper. It compared generic bank-only replay's supplied outer revision with
the planner's intentionally unchanged authored revision. The helper now checks
that the planned document is unchanged and normalizes only the revision for
that replay comparison, only when every resolved step has no edit. Register
writes and exact inverse equality remain checked. Production behavior did not
change, and authored cuts, including empty child cuts, cannot use this branch.
The final core/store run covers this correction. Both failed logs are retained.

The initial smoke command used nonexistent `--scenario release` and failed
before running a scenario. Its receipt remains retained. The supported editing,
keymap, place-slice, delete-range and named-registers scenarios replaced it.
This required no product change.

Native visual review found missing arrow glyphs in the new continuation hints.
The final label function uses Left/Right/Up/Down while preserving key identities,
bindings and routing. The original screenshot is retained, and final rendered
and native review confirms the correction.

## Limits

Ordinary Sequence range reducers still refuse endpoints inside composite
children. Enter their group or select complete boundaries. Text and role
objects, analysis-dependent motions, range Repeat, temporal occurrence editing,
additional Macro instructions and broader dot-repeat remain required.

Scripted replay, native automated key delivery and Metal rendering do not
qualify the physical keyboard-layout matrix, OS IME, VoiceOver, physical display
color or acoustic output.
Debug linking retains the existing `__eh_frame` size warning. This increment
makes no release-build, packaging or performance claim. The native wrapper
retains build-host library dependencies and does not establish signing or
standalone distribution.
