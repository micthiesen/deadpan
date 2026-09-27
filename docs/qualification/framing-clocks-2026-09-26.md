# Exact owner-clock framing qualification, 2026-09-26

`Framing::evaluate_exact` evaluates an envelope over an exact positive derived
owner extent. The existing integer evaluator delegates to it, preserving exact
endpoint selection and Q32 ties-to-even interpolation. This supplies numerical
support for the [nested splice design](../STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks).
It does not implement structural clock adapters or arbitrary nested insertion.
Core schema 25, database schema 31 and all edit reducers remain unchanged.

## Scope and numerical evidence

Six source/test/fixture paths changed relative to the preceding boundary-location
checkpoint. The implementation compares products without first materializing an
overflowing quotient or envelope endpoint. Five 64-bit limbs cover the largest
294-bit product; Q32 remainder division needs at most 295 bits. Invalid owner
extents and out-of-range positions are rejected. Authored integer frame counts
and exact static/endpoint poses retain their existing meaning.

The checked-in Fraction generator produces 300 independent reference cases,
including 58 quotients and 78 endpoints outside the `ExactRatio` representation.
Rust tests compare actual outputs with those cases, exercise all four curve
types under exact clock scaling, check step boundaries and invalid ranges, and
retain the previous integer-clock reference fixture. All 13 framing tests pass.

The initial focused run passed all 86 core library tests and every arithmetic
comparison, but one new test incorrectly required at least 70 overflowing
quotients. The generator contains 58 after exact cancellation. Its independently
calculated overflow counts now accompany the fixture, and the test checks those
counts. The focused retry passed all 13 framing tests.

## Independent review

The general reviewer found no defects in validation, endpoint selection, API
compatibility or test coverage. The numerical reviewer found no defects in the
limb arithmetic, bounds or interpolation formula. Its separate Python model
checked 58,259 multiplication cases with 9,489 correct overflow refusals,
6,064 rational segment cases reaching 294-bit products and 295-bit shifts, and
6,000 comparisons with the frozen integer formula. Fixture regeneration matched
byte-for-byte. These model checks are additional review evidence, not Rust test
executions. Scripts, hashes and summaries are retained.

The structural design review rejected ordinary subtree duration dilation as a
PCM-preserving shortcut. Preserve depends on physical input grids and processing
history, even when affine picture mappings cancel. The preferred separate
effective-clock representation still needs a complete authored/DSP ownership
contract, strict schema integration, mark/Repeat handling and actual PCM proof.

## Repository verification

Environment: Apple Silicon macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. Commands ran serially with locked dependencies.
Counts below belong to each invocation and overlap; they are not a count of
distinct tests across the entire table.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test --workspace --locked` | exit 101; 913 passed, 1 failed, 0 ignored |
| `cargo build --workspace --locked` | exit 0 |
| `cargo run -p deadpan-cli -- doctor` | exit 0 |
| `cargo test --locked -p deadpan-store -p deadpan-plan -p deadpan-render` | exit 0; 398 passed, 0 failed, 0 ignored |
| `cargo clippy -p deadpan-app --features ui-harness --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test -p deadpan-app --features ui-harness --all-targets --locked` | exit 0; 196 passed, 0 failed, 0 ignored |

The workspace test invocation stops at the already established sandbox refusal
in `deadpan-jobs/tests/artifact.rs:200`: creating its Unix listener returns
`OS 1 PermissionDenied (Operation not permitted)`. That invocation therefore
does not run all later targets or doctests. The separate store/plan/render and
feature-enabled app invocations cover their complete selected suites. No test
was disabled to hide the socket failure. The full workspace gate is not green.

The final gate verifies all 504 source/config paths were unchanged
during execution. The existing harness is preserved and exercised through its
feature-enabled tests. All ten ImageGen boards and exact prompts are preserved
and hash-checked by the checkpoint.

## Boundaries and delivery

No UI, input routing, native lifecycle, shader or project format changed. No
native smoke test, offscreen Metal replay, physical-display review or release
latency run was performed for this increment. Existing integer picture-plan and
CPU geometry regressions run headlessly; rational structural clocks are not yet
admitted to those plans. The previous missing-Metal limitation and aesthetic,
keyboard, accessibility and latency findings remain open. This is not new GUI
qualification and no requirement or release gate changes status.

The current session cannot write `.git`, so it cannot commit or push. The complete
tracked patch, untracked archive and verified manifest are preserved in
`/tmp/deadpan-splice-representation-20260926/checkpoint`, including concurrent
harness changes and design assets. The project goal remains active.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-26-framing-clocks/)
contains source hashes, the increment diff, command logs, independent review
scripts and exact results. Its diff is against the preceding source checkpoint,
not Git HEAD. Logs are compressed; `sha256.json` covers every retained payload.
