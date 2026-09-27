# Repaint waits and worker timing, 2026-09-26

The optional UI harness now waits on egui repaint notifications instead of an
unconditional 1 ms sleep. Worker timestamps separate queueing, execution,
publication and UI delivery. Final feature lint and 193 unit plus 2 integration
tests pass. This establishes diagnostic behavior, not a new application latency
or aesthetic result. No DP requirement or delivery gate changes status.

## Scope and identity

The increment follows the [composite-insertion qualification](composite-insertion-2026-09-26.md)
and its retained timer diagnostic. Git HEAD remains
`c03a5edde5f28d27074745eb15711cb28b1f2e50`; the prior uncommitted implementation,
the other agent's UI harness, and the ImageGen boards are preserved. Git metadata
is read-only in this session, so no commit or push was performed.

[Evidence](../../tools/ui-feedback/evidence/2026-09-26-repaint-wake/README.md)
contains the incremental source patch, original and final source manifests,
commands, logs, blocked replay reports, review findings and checksums. Each
validation group held all 491 source/config paths unchanged. Only `harness.rs`
and the new `harness/wake.rs` changed between groups, for the review fix and lint
correction; default-feature sources stayed unchanged.

Environment: arm64 macOS 26.5.2, build 25F84, Rust 1.97.1; pinned egui and
egui_kittest 0.36.2; developer FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. The harness could not read the hardware brand.
No physical display, power or thermal measurement was made.

## Behavior and tests

The scheduler belongs only to the fresh kittest context. Native eframe retains
its own callback. Two bounded slots coalesce deadlines for eligible egui passes;
stale passes expire, due notifications are serviced, and future one-shot timers
survive an early input step. Requests arriving during a step remain pending.
The driver checks readiness again before waiting and preserves its 15-second
deadline and visual capture-time exclusion. Virtual replay time remains separate
from monotonic wall time.

Feature-gated worker timing brackets `perform`, then records accepted mailbox
publication before repaint notification. The UI retains these timestamps through
held delivery. First matching successful receipts with complete ordered timing
produce four phase samples; stale, failed, repeated or invalid timing does not.
Worker events retain actual timestamps, even though they arrive in the report
with the later receipt. Existing picture and input endpoints remain separate.

Thirteen added tests cover wake handoff, delayed/coalesced/stale deadlines,
finite timeout, a real egui background repaint, exact phase decomposition,
held-delivery accounting, outcome exclusions and invalid timestamps. A real
B-frame fixture test verifies publication before notification, exact retained
picture bytes across a revision and ticket-source change, and an explicit
failure after clearing the decoder with revoked original handles. It uses
notifications rather than polling and makes no millisecond performance claim.

## Review and verification

The general review found no actionable issues. The concurrency review found
that clearing every pending notification at step entry discarded eligible future
timers. The final code consumes only due deadlines, and a deterministic
regression covers an intervening input step and subsequent pass expiry. Focused
re-review found no remaining actionable concurrency defect.

| Check | Actual result |
| --- | --- |
| Repository formatting, workspace all-target Clippy | Pass in the original gate. |
| `cargo test --workspace --locked` | 887 passed, 1 failed, 0 ignored across 63 completed test binaries. The invocation stopped at the artifact socket test; later crates and doc tests did not run in this invocation. |
| Workspace build and CLI doctor | Pass; core schema 24, database schema 30. |
| Initial optional-feature Clippy | Failed on `manual_is_multiple_of`; corrected without suppression. |
| Initial optional-feature tests | 192 unit and 2 integration tests passed. |
| Final formatting, optional-feature all-target Clippy and tests | Pass; 193 unit and 2 integration tests, 0 failed or ignored. |
| Visual editing replay | Failed before app construction: Metal adapter unavailable. No scenario steps, GUI assertions or captures ran. |
| Release edit-latency replay | Optimized build succeeded; replay failed at the same adapter boundary. No latency samples ran. |
| Shortcut audit in both blocked replays | Pass: 3,472 routing cases, 62 reservations, no conflicts. |

The workspace failure is
`deadpan-jobs/tests/artifact.rs:200`, where `UnixListener::bind` returns OS error 1,
`PermissionDenied`, in this sandbox. It remains a failed check. The original
feature compile attempt also retains two corrected calls to APIs absent from
pinned egui (`Context::run` and `PlatformOutput::take_commands`).

Both GPU attempts preceded the final delayed-timer fix and failed before reaching
the affected code. They were not repeated after that fix because Metal access
had not changed. Final feature tests and lint cover the corrected source. The
native lifecycle smoke test was not repeated: native startup, shutdown and
callback ownership did not change.

## Remaining acceptance

A host with Metal must run the corrected visual and release performance suites,
inspect the actual frames against the design boards, and retain comparable
reports. Reports now identify `egui_repaint_callback_v1` waiting and
`request_start_finish_publication_receipt_v1` timing. Do not compare them with
older polling reports as though only application speed changed.

The earlier Repeat/Hold p95 misses of 75.26/138.65 ms remain open against their
50/100 ms targets. The new instrumentation does not establish which part of those
historical samples was decoder work, nor qualify full-size media, physical
presentation, VoiceOver, native IME or the complete editor.
