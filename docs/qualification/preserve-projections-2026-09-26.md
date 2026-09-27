# Intrinsic Preserve projection qualification, 2026-09-26

`AudioStageProjection` binds a current scoped Preserve stage to full intrinsic
input and independent output-policy tapes. Parent inputs consume child intrinsic
output, and a separate PointCeil tape can schedule that output around a pause.
The shared `StageAudio` renderer produces actual PCM with a request-local memo.
The [contract](../AUDIO_INPUT_TAPES.md) distinguishes these preparation views from
authored routes and final absolute RoundEven allocation. Core schema 25 and
database schema 31 are unchanged.

## Scope and tests

Fifteen source/test paths changed from the preceding input-tape checkpoint.
Constructors enforce normalized clocks, plan identity, owned descendants,
definition and Repeat scope. Graph validation and copied policy views have
separate bounded admission; each scanned/materialized run is charged before
recursive allocation. Exact policy intervals are remapped before caller-grid
rounding, preserving semantic support separately from routed windows.

Seven new PCM tests and five new plan tests cover nested canonical history,
pause placement, route identity over one stage descriptor, shared residency,
depth, error recovery, source revocation during memo reuse, and complete hidden
ordinary/Bound Preserve input preflight. Plan tests cover foreign scopes,
nonzero source selections on normalized clocks, NTSC intrinsic 3204 versus
root 3203 counts, a Hold with no native point that gains a caller-grid point,
bounded shared-child expansion, and retained silence beyond a visible crop.

The selected-placement decay test independently verifies that raw Source policy
would suppress the interval while canonical Preserve output has nonzero decay.
PCM oracles choose their own source coordinates and canonical lengths, using
the decoded WAV fixture and the pinned resampler/stretch implementation. This
is not an independent DSP implementation or acoustic qualification.

The focused invocation passed 56 tests, zero failed or ignored. Three earlier
attempts stopped at compilation: an ambiguous sample-grid type, child-module
method visibility, then a missing test-module path. Those were corrected.
The first repository gate passed formatting but failed strict Clippy on the
large TapeProvider variant; boxing that variant corrected it without suppression.

## Review

Two independent reviewers checked mapping, ownership, bounded work, cache
identity, admission and tests. Review corrected eager recursive policy expansion,
semantic-support cropping at run seams, a weak initial decay fixture, and hidden
ordinary/Bound stage preflight. Projection Debug and serialization expose only
descriptor/duration, avoiding recursive graph traversal. Final review found no
remaining concrete issue. The implementation peer also reviewed the main
renderer additions. Reviewers ran no Cargo commands.

Per-request source revalidation remains intentional and can exhaust its separate
65,536-observation budget. Frame and stage bounds do not guarantee that every
combination of their maxima is admissible. The memo retains bytes only for one
request; persistent playback preparation and performance qualification remain open.

## Repository verification

Environment: arm64 macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. Commands ran serially. Invocation counts overlap.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test --workspace --locked` | exit 101; 928 passed, 1 failed, 0 ignored |
| `cargo build --workspace --locked` | exit 0 |
| `cargo run -p deadpan-cli -- doctor` | exit 0 |
| `cargo test --locked -p deadpan-store -p deadpan-plan -p deadpan-render` | exit 0; 410 passed, 0 failed, 0 ignored |
| `cargo clippy -p deadpan-app --features ui-harness --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test -p deadpan-app --features ui-harness --all-targets --locked` | exit 0; 196 passed, 0 failed, 0 ignored |

The workspace test run stops at the established sandbox denial in
`deadpan-jobs/tests/artifact.rs:200`: Unix listener creation returns OS 1
PermissionDenied. Later workspace targets and doctests therefore do not all run
in that invocation. Separate store/plan/render and harness-enabled app commands
cover their selected suites. No test was disabled, and the complete workspace
gate remains non-green while this failure persists.

All 513 source/config paths stayed unchanged during the final
gate. The other agent's UI harness remains included and its enabled tests ran.
The checkpoint verifies all ten ImageGen boards and exact prompts.

## Remaining work and delivery

This does not install a route into an authored document or change ordinary
root-plan evaluation. Exact effective owner clocks, final RoundEven placement,
persistent route identity, copy/split/isolation transforms, strict replay and
native nested insertion remain required. No DP requirement or gate changes
status; the full project goal remains active.

No UI, keyboard routing, shader or native lifecycle changed. No native smoke,
Metal replay, GUI aesthetic review or release latency measurement was repeated
for this preparation increment. Existing GUI, accessibility, physical display,
performance, AI integration, export and release findings remain open.

The session cannot write `.git`, so it cannot commit or push. The verified
tracked patch, untracked archive and manifest are retained in
`/tmp/deadpan-preserve-projection-20260926/checkpoint`, including the concurrent
harness and design assets. [Retained evidence](../../tools/media-qualification/evidence/2026-09-26-preserve-projections/)
contains command logs, source hashes, the increment diff and review record.
The increment diff is against the preceding checkpoint, not Git HEAD.
