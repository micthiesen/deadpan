# Native Sequence navigation qualification, 2026-09-26

This increment adds Enter/Backspace navigation through ordinary Sequence groups,
breadcrumbs and current-depth inspector commands while retaining the absolute
project clock. It extends the existing `nested-pause` UI scenario contributed
through the shared harness. See the [interaction contract](../GROUP_NAVIGATION.md).

The exact source delta is against the previous verified nested-insertion
checkpoint. Git HEAD remains `c03a5edde5f28d27074745eb15711cb28b1f2e50`; this
environment makes `.git` read-only, so these changes have not been committed or
pushed. Core schema 26 and database schema 32 do not change.

## Implemented boundary

The service accepts only a direct child of a validated active Sequence path,
with captured writer session and revision. Cached and prepared reuse preserve
their destination. Completion restores its scope before resolving selection;
history truncates stale paths. Split/Delete choose resulting siblings at that
depth. Camera validates its navigation path and preserves the exact cursor.
Group-edge pauses that resolve above the viewed scope fail without mutation.

Space continues through the full edit. Explicit pause and terminal updates
follow the heard cursor into the nearest containing scope. Command and inspector
stops retain their selected target. Text, IME, native controls, menus, dialogs,
help and Camera keep priority over normal group keys. Pending operators and
counts clear on group navigation.

## Review and checks

Three independent review lenses covered general correctness, scope and
asynchronous completion, and keyboard ownership. The general review found a
cursor jump when leaving a group after audition passed its parent. The fix moves
cursor/selection resolution into the shared window-free selection transition:
entry clamps to the new group; Backspace and breadcrumbs keep the absolute cursor.
A regression covers heard positions before and after the parent, ordinary return
and entry into an empty group. Follow-up review found no remaining material issue.

Keyboard review requested explicit new-key coverage. The nested scenario now
opens the Hold inspector command with Shift-Tab and Enter and checks Camera
Backspace ownership. Existing CPU help/menu replays now include Enter/Backspace.
These additions are compiled; the GPU replay remains unavailable. The invariant
review reported no material issue. A proposed deeper automatic descent on pause
was not adopted: transport follows the nearest containing ancestor of the browsed
path, as specified and tested, rather than inventing a new sibling scope.

Environment: arm64 macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. Final commands ran serially:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test --workspace --locked` | exit 101; 948 passed, 1 failed, 0 ignored |
| `cargo build --workspace --locked` | exit 0 |
| `cargo run -p deadpan-cli -- doctor` | exit 0 |
| `cargo clippy -p deadpan-app --features ui-harness --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test -p deadpan-app --features ui-harness --all-targets --locked` | exit 0; 205 passed, 0 failed, 0 ignored |

All 530 source/config files stayed unchanged through that gate.
The workspace test invocation stopped at the existing sandbox denial in
`deadpan-jobs/tests/artifact.rs:200`: Unix listener creation returned OS 1
PermissionDenied. Later suites did not all execute in that invocation. No test
was disabled and the workspace gate remains non-green.

Before the four preview-only review corrections, the separate locked
store/plan/render invocation passed 417 tests.
Its source and dependencies remain byte-identical in the final gate. It was not
repeated for those UI changes. Invocation counts overlap and are not distinct-test
totals. Earlier compile failures and an incorrect new test boundary assertion
are retained alongside the corrected runs.

The contributed harness was invoked as:

```sh
cargo run -p deadpan-app --features ui-harness --locked -- --ui-check --scenario nested-pause --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift --output /tmp/deadpan-group-navigation-20260926/ui-nested
```

That attempt preceded the review corrections and exited 1 at
`egui_kittest` renderer construction with `No adapter found`. Its nested scenario
had zero steps, checks and captures. The separate shortcut audit passed
3472 routing cases against 62
reservations with no conflicts, matching the current Kestrel source digest.
No final-source GUI pass is claimed. Native startup/shutdown smoke was not repeated
because startup/lifecycle did not change. Release latency, physical key/IME,
VoiceOver and comparison against ImageGen targets remain unverified here.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-26-group-navigation/)
includes exact logs, source identities, incremental diff, review record and the
failed visual report. Local documentation file links were checked separately.

## Limits

The current GPU replay failed before constructing the application because no
Metal adapter was exposed. It ran zero nested UI steps or assertions and produced
no captures. CPU layout checks and compiled replay code do not qualify aesthetics,
hit testing, physical keyboard delivery, native IME or VoiceOver. No fresh image
comparison or latency qualification is claimed. The existing ImageGen boards
and exact prompts remain the design targets and are preserved in the repository.

Repeat plays, gap branches, Retime descendants, group creation/ungroup controls,
range/semantic editing and selected-moment reuse remain required. The full editor,
audio mix, AI workflow, export, recovery, packaging and performance acceptance
remain open or partial. No DP requirement or gate changes status.
