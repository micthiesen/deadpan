# Live audio input tape qualification, 2026-09-26

`AudioSignalTape` projects exact windows of current scoped audio signals onto
one intrinsic PointCeil grid. `StageAudio::read_tape` produces real PCM through
the existing source, binding, RoomTone and Preserve renderer. The
[input contract](../AUDIO_INPUT_TAPES.md) keeps global meaningful input selection
separate from per-run allocation. Core schema 25, database schema 31 and all
authored edit commands remain unchanged.

## Scope and evidence

Ten source/test paths changed relative to the previous framing checkpoint.
The constructor bounds runs, validates exact coverage and plan identity, and
retains current definition and Repeat scope. Binary-search comparisons and
underlying queries share a work allowance. Reads share source observations,
cancellation, deadline, stage admission and cache revalidation across runs.
Run seams do not reset phase, crop filter support or rebase a Bound offset.

Seven new plan tests and eight new PCM tests cover the fractional NTSC cut at
sample 2402.4, zero-point runs, global versus internal support edges, hidden
transparent-Partition context, genuine Bound anchors, live raw mapping changes,
RepeatDefault versus occurrence scope, existing nested Preserve history,
suppression, source revocation, cancellation and shared preparation limits.
PCM comparisons use the decoded WAV fixture, independently selected exact
source coordinates and the pinned resampler/canonical stretch implementation.
They do not claim an independent implementation of the DSP kernel.

The focused invocation passed 44 tests across the audio-definition and tape
targets. Its first attempt did not reach tests because two new test helpers
had a missing lifetime and a shadowed function name; both were corrected. The
earlier library compile also required an explicit sample-grid type annotation.
The corrected library check passed. The final gate covers the reviewed search
budget refinement from 3 to 4 policy work units.

## Independent review

A reviewer who did not implement this increment checked exact remapping,
filter support, Repeat/binding scope, cache identity, shared admission and tests.
Review caught uncharged dispatch comparisons, order-dependent policy limits,
an allocation-versus-support ambiguity and insufficient regression cases.
Those findings were fixed and covered. Global support is explicitly a meaningful
input selection, so its outer crop is intentional; a proposed objection to that
crop was withdrawn after the contract was clarified. Final review found no
remaining production defects. The reviewer ran no Cargo commands.

## Repository verification

Environment: Apple Silicon macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg
prefix `/tmp/deadpan-ui-ffmpeg/prefix`. Commands ran serially. Counts belong to
individual invocations and overlap.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test --workspace --locked` | exit 101; 921 passed, 1 failed, 0 ignored |
| `cargo build --workspace --locked` | exit 0 |
| `cargo run -p deadpan-cli -- doctor` | exit 0 |
| `cargo test --locked -p deadpan-store -p deadpan-plan -p deadpan-render` | exit 0; 405 passed, 0 failed, 0 ignored |
| `cargo clippy -p deadpan-app --features ui-harness --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test -p deadpan-app --features ui-harness --all-targets --locked` | exit 0; 196 passed, 0 failed, 0 ignored |

The workspace test invocation stops at the established sandbox refusal in
`deadpan-jobs/tests/artifact.rs:200`: Unix listener creation returns OS 1
PermissionDenied. It therefore does not run every later target or doctest.
The separate store/plan/render and harness-enabled app invocations cover their
complete selected suites. No test was disabled. The full workspace gate is
not green while this sandbox failure remains.

All 509 source/config paths were unchanged during the final
gate. The concurrent UI harness is preserved and exercised by its enabled
tests. All ten ImageGen boards and exact prompts are retained and hash-checked
by the delivery checkpoint.

## Remaining work and delivery

This input reader is not a persisted splice route or a redefinition of an
existing Preserve operand. Arbitrary nested pauses still require exact
effective clocks, validated current-tree route ownership, intrinsic stage
input/output views, a separate output-placement map, cache identity for changed
operands, command lifecycle and strict migration. An inner stage's scheduled
pause must never become an outer stage's historical input. Silence still needs
each output grid's policy, including pauses that own no input point.

No UI, keyboard routing, shader or native lifecycle changed. No smoke test,
Metal replay, GUI aesthetic review or release latency run was performed for
this pure preparation increment. Prior GUI, physical display, accessibility,
performance and release findings remain open. No DP requirement or gate changes
status, and the full project goal remains active.

The session cannot write `.git`, so it cannot commit or push. The complete
tracked patch, untracked archive and verified manifest are retained in
`/tmp/deadpan-audio-projection-20260926/checkpoint`, including the concurrent
harness and design assets. [Retained evidence](../../tools/media-qualification/evidence/2026-09-26-audio-input-tapes/)
includes command logs, source hashes, the increment diff and independent review.
The increment diff is against the previous checkpoint, not Git HEAD.
