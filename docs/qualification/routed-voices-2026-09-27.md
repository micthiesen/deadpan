# Retained sound routes into PCM, 2026-09-27

This increment connects retained sound sample routes to the existing PCM engine.
It keeps the complete source or processed provider while edits select, move or
silence its old output. Core 28 and database 34 are unchanged. Persisted sound
events, current scoped Hold allowances, voice effects and the final voice bus
remain required. No DP requirement or delivery gate is promoted.

## Provider capture and reading

`AudioRoutedSignal` accepts a complete independent source input or shared Preserve
projection on PointCeil. `AudioRoutedRoot` accepts a complete projected output on
its captured RoundEven grid. Constructors compare original extent, grid origin,
spacing, rounding rule and allocation. Cropped, constrained or resumed providers
cannot substitute an equal-duration or equal-count capture. Root placement keeps
its signed fractional origin, original sample map and output-policy clock.

The readers resolve exact integral old sample labels, evaluate the retained
provider and copy samples to the destination. They do not derive phase from the
edited frame clock. Every span shares existing deadline, cancellation, work,
depth, source-fingerprint and PCM-residency checks. A fully masked query still
admits its source or complete processing history. Projection identity retains one
full canonical preparation throughout the request.

Route gaps and old selected audible masks remain separate from complete source
filter support and processing history. Captured provider policy follows the old
samples. Current consuming Hold gates and creative edges are still separate
requirements; routing an old gate does not establish an allowance.

## Evidence

The pure plan cases check source and projection identities, definition/occurrence
scope, foreign plans, same-count mismatches, constrained source input, cropped and
resumed roots, and positive, negative and tie-position root origins. The existing
sample-route suite retains compact-route and query-budget coverage.

Real PCM tests register the explicit mono-center, 44.1 kHz fixture through the
production original-retention, decode, qualification and receipt path. Its PCM
bytes are unchanged. References use independent 147/160 source-sample phase,
a 137-sample onset, canonical 3/2 Preserve preparation, exact boundary arithmetic
and dense sample-array copying.

At 30000/1001 fps, two inserted frames resume old sample 3203 on PointCeil and
3204 on RoundEven. The latter starts at destination sample 6406. Adjacent source
and processed PCM are explicitly different and nonzero, so silence cannot make
these assertions pass accidentally. Cold second-suffix reads, shuffled blocks
and fractional source and Preserve windows retain the full history. A separate
incorrectly cropped sinc reference differs from the expected source samples.
Prior gaps, source exhaustion and the extra sample allocated by moved rounding
remain silent. Original PCM and project history stay unchanged.

Resource cases exercise hidden nested-depth rejection before source access,
revocation, successful retry and multiple spans sharing two nested preparations.
A source-call baseline additionally distinguishes shared preparation from
rebuilding the full history separately for each audible fragment.

The first real Preserve reference incorrectly allowed filter taps beyond its
declared nine-frame input selection. The correct source support ends at
`ceil((9 * 8008/5 - 137) * 147/160) = 13118`. Correcting that reference preserves
the complete input selection while leaving later output fragments independent.
The failed run is retained alongside the corrected verification. Initial compile
errors in a private helper call and a nested test-module path were also corrected.

## Verification and acceptance limits

Verification used Rust/Cargo 1.97.1 on macOS 26.5.2 (25F84), arm64, with
`DEADPAN_FFMPEG_PREFIX=/tmp/deadpan-ui-ffmpeg/prefix`. All 586 source/config
hashes stayed unchanged through the complete seven-command gate.

| Check | Result |
| --- | --- |
| Routed-provider and existing sample-route plan suites | 19 passed |
| Nested preparation limits, masked admission and recovery | 3 passed |
| Real routed catalog PCM, including corrected full-input reference | 4 passed |
| Workspace formatting and strict Clippy | Passed |
| Locked workspace tests | 1,804 passed, 5 failed, 0 ignored |
| Locked workspace build and CLI doctor | Passed |
| App plus `ui-harness` strict Clippy | Passed |
| App plus `ui-harness` all-target tests | 236 passed, 0 failed |

An independent general reviewer returned no findings. The parent also audited
timing, provider capture and shared resource limits. Two further focused reviewer
launches were unavailable because the collaboration tool reached its thread
limit. The existing 65,536 provenance-check ceiling is separate from the distinct
asset cap; a complex route reaching that explicit work limit is not a permission
to bypass source revalidation. Strict Clippy also led to boxing the larger source
variant without changing its identity or timing behavior.

The jobs test `directories_fifos_and_sockets_are_rejected_without_blocking`
again fails at socket creation with `PermissionDenied` / `Operation not permitted`
in `crates/deadpan-jobs/tests/artifact.rs:200`. Four playback cases hit the existing
ten-second helper at `crates/deadpan-playback/src/tests.rs:258`:

- `tests::canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock`
- `tests::original::original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock`
- `tests::original::original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm`
- `tests::seek_reuses_private_pcm_but_a_new_session_must_reopen_sources`

The first three playback failures match the previous checkpoint; the seek-cache
case is additional in this run. Their cause remains unresolved. Isolated retries
all pass with the unchanged workspace test binary, whose hash was checked before
and after. Whole-test times were 11.811 s, 20.934 s, 12.209 s and 8.133 s in the
order listed. These retries do not erase full-run failures or establish their
cause. The workspace gate remains failed. No deadline or threshold was increased.

The [verification record](../../tools/media-qualification/evidence/2026-09-27-routed-voices/verification.json)
retains exact commands, compressed logs, source hashes, review scope, failed
development runs and checkpoint provenance. The private checkpoint retains the
tracked patch and every untracked file. All 59 app source files and all 11 design
boards with their exact prompts are unchanged from the preceding increment.

No GUI, device or authored editing behavior changes in this increment. The
contributed UI harness is preserved and included in the required feature checks.
These backend checks do not establish GUI aesthetics, physical keyboard/IME or
accessibility behavior, listening, export equivalence or performance. Prior Metal
`NoAdapter` results remain unchanged. This session's `.git` access is read-only;
no commit or push is claimed.
