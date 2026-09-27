# Nested Sequence pause qualification, 2026-09-26

Core 26/database 32 extend `InsertTime` into ordinary Sequence groups. The Hold
stays under its actual owner, inherited framing stays live, and later siblings
at every enclosing Sequence level retain independent audio resume entries.
See the [command contract](../INSERT_TIME.md). Repeat and Retime ancestor
insertion, rational effective clocks and selected Original-moment payloads
remain required. No DP requirement or release gate changes status.

## Behavior and tests

The shared target query returns the insertion Sequence, child slot and exact
Split identity count. A strict Sequence interior descends; an existing seam
stays at that level. Physical Source/Hold fragments retain the existing
pre-Split lattice and separate post-Split placement clocks. Root paths retain
their previous transaction output. Identity shortages, unsupported ancestors
and timing overflow refuse the edit without publishing intermediate state.

Tests cover three Sequence levels, a compact billion-play suffix and gaps,
empty siblings, boundary-biased marks, pinned marks, exact inverse patches and
JSON round trips. Native service tests use registered measured video and inspect
the exact left frame, retained child crop, each live ancestor once, later
group/Hold framing changes and reopened storage. The latter framing changes
use typed store commands; they do not establish a native nested inspector.

Decoded PCM tests use a known WAV and independent NTSC sample arithmetic. With
a one-frame prefix, a Source beginning at sample 1601.6 resumes old sample 3203
at new 4805 with phase 8007/5. Later sibling domains have their own entries and
phases. Splitting a previously resumed fragment at current sample 6406 gives
phase 16012/5, which resumes at new 8008. Tests also cover a second pause,
Hold resizing, implicit RoomTone gaps and unchanged full Preserve preparation
in a shifted sibling. Oracles share the pinned reconstruction/DSP implementation;
these are not independent DSP or acoustic measurements.

The CLI regression exercises actual subprocess dry-run, commit, stale rejection,
reopen, exact node/binding restoration through undo and redo, and never-reused
revisions. The authentic database-31 fixture was authored with the preserved
core-25 executable and retains 26 revisions, 12 commands and pending redo.
Migration compares every snapshot and transaction. Modern nested transactions
forged as old history are rejected while preserving both original and backup.

Native completion now carries its exact committed cursor. The root-only
timeline selects the visible group containing a nested Hold; it does not claim
to expose that Hold's inspector. A pure selection regression covers hidden and
visible identities, explicit clear, missing cursor and out-of-range positions.
The `nested-pause` harness scenario uses real keyboard events and real project,
decoder and picture services after typed fixture setup. It checks the same
ownership, selection, frame and one-step history behavior in the actual UI.

## Review

An independent timing reviewer checked core/audio semantics. The migration
implementer independently reviewed core admission, root transaction equivalence
and native capture/completion/harness code, which they did not implement. Main
review checked the migration adapter and fixture provenance. Review corrected
stale help wording, caught the hidden-Hold selection
mismatch before delivery and added direct coverage for resplitting a previously
shifted nested Source. A proposed missing first-split test was dismissed after
confirming that the NTSC prefix fixture already performs that exact split.

## Verification

Environment: arm64 macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg
prefix `/tmp/deadpan-ui-ffmpeg/prefix`. Commands ran serially. Invocation counts
overlap and do not represent distinct tests.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test --workspace --locked` | exit 101; 941 passed, 1 failed, 0 ignored |
| `cargo build --workspace --locked` | exit 0 |
| `cargo run -p deadpan-cli -- doctor` | exit 0 |
| `cargo test --locked -p deadpan-store -p deadpan-plan -p deadpan-render` | exit 0; 417 passed, 0 failed, 0 ignored |
| `cargo clippy -p deadpan-app --features ui-harness --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test -p deadpan-app --features ui-harness --all-targets --locked` | exit 0; 198 passed, 0 failed, 0 ignored |

All 527 source/config files stayed unchanged through the gate.
The workspace invocation stopped at the existing sandbox denial in
`deadpan-jobs/tests/artifact.rs:200`: Unix listener creation returned OS 1
PermissionDenied. Later suites did not all execute in that invocation. The
separate store/plan/render and harness-enabled app commands cover their selected
suites. No test was disabled. The full workspace gate remains non-green.

Focused checks passed: 23 core insertion tests, 9 decoded-audio insertion tests,
92 migration tests, 17 CLI command tests and 6 native pause service tests.
The additional nested re-split PCM test ran in the final workspace invocation.
An earlier CLI test attempt stopped at a test assertion type mismatch, corrected
before the passing run. Harness-enabled strict lint also passed independently
before the final gate.

The visual command was attempted once for the new native completion behavior:
`cargo run --locked -p deadpan-app --features ui-harness -- --ui-check --output
/tmp/deadpan-nested-sequence-20260926/gui --scenario nested-pause`.

It exited 1 because Metal adapter creation failed before app
construction (`No adapter found`). The nested scenario ran zero steps and zero
assertions and produced no UI captures. The separate shortcut audit passed
3472 routing cases against 62
reservations with no conflicts. That audit and the compiled scenario do not
qualify the new keyboard presentation or aesthetics.

Native startup/shutdown smoke and release latency replay were not repeated:
startup/lifecycle did not change, and Metal startup prevents this environment's
new visual replay. Physical display, VoiceOver and native IME delivery remain
unverified. No new performance claim is made.

## Scope and delivery

The full goal remains active. Nested inspector navigation, Repeat/Retime
interiors, fractional timing ownership, full audio graph, AI application
integration, export, recovery and release qualification are still open. Earlier
GUI aesthetics, accessibility, physical display and latency findings remain open.

Git metadata is read-only in this session; no commit or push is possible here.
The verified checkpoint at `/tmp/deadpan-nested-sequence-20260926/checkpoint`
retains all pending source, the concurrent UI harness and ImageGen design boards.
[Evidence](../../tools/media-qualification/evidence/2026-09-26-nested-sequence/)
includes logs, source hashes, reviews, fixture provenance and the increment diff
against the preceding verified checkpoint, not Git HEAD.
