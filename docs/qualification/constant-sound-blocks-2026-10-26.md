# Constant sound blocks, 2026-10-26

The processed root-sound Split/refinement witness now completes within the
existing preparation allowance and preserves its entire authored, faded and
limited PCM. The full general Split matrix improves from three passes/four
work-limit failures to **four passes/three work-limit failures**. This does not
qualify the complete sound-bearing Split lifecycle or enable native scoped Split.
No acceptance criterion, requirement or delivery gate closes here.

## Implementation

Constant ClipGain records compile on their exact Q32 dB grid at the existing
channel boundaries. Complete route programs retain Repeat escalation, balance,
stage order and independent send gains. Envelopes and mute ranges keep the
scalar evaluator. Checkpoint/stage storage is reserved before PCM allocation.

For all-constant active routes, Components executes one owner/stage across the
block. Each checkpoint is retrieved once, outside the sample loop. A unity
checkpoint removes its complete transfer pass instead of dispatching a no-op
for every sample. Nonidentity arithmetic and its work charges remain. Event
send taps precede owners; owner send taps precede wet returns; returns enter
before parents. Contributions sum in the original member order at every sample.
EQ clocks, permissions and selected-output aliases remain sample-specific.

No new PCM buffer, query cache, dependency, source authority or limit is added.
Finite input is established by admission and every effect boundary; untrusted
wet callback values still pass the normal finite-checked transfer. Send mode
still checks the selected dry sum. Mixed dynamic/constant routes retain scalar
execution. Historical-query ownership and deep metadata accounting remain open.

Implementation: [gain preparation](../../crates/deadpan-audio/src/authored_gain.rs),
[route programs](../../crates/deadpan-audio/src/shared_components/program.rs),
[block execution](../../crates/deadpan-audio/src/shared_components/block.rs).

## Verification

Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1, locked dependencies,
qualified FFmpeg prefix, optimized Cargo test profile with debug/overflow checks.
These are correctness checks, not a calibrated performance comparison.

The scoped patch consists of six Audio files over
`b0d2170647afb5baf12319bf2b192c1377ffadb0`. An isolated source copy contains that
overlay without the unfinished Core/Plan/Audio/Store Split work. Source-only
independent review of the complete six-file optimization found no issue.
Implementation commit: `a735863eac82ff5ce7224b67c5ca5d3a4d17d59f`.

| Check | Result |
| --- | --- |
| Preserved pre-change selected-play executable | Reproduces its preparation-work failure; binary unchanged. |
| Focused route-program/block tests | Six pass, zero ignored. |
| Full current Audio library, first and final checks | 193 pass, zero ignored, each run. |
| Isolated six-file overlay Audio library | 179 pass, zero ignored. |
| Isolated overlay existing real-WAV stage suite | 201 pass, zero ignored. |
| Integrated general Split matrix, copied executable | Four pass, three work-limit failures, zero ignored. |
| Final root-sound continuation after lint fixes | Pass, zero ignored; source and copied executable unchanged during execution. |
| Final `cargo xtask check --no-test` | Formatting and strict all-target Core/Plan/Audio/Store lint pass. Tests are the separate runs above. |

The focused tests compare bitwise f64 PCM with scalar automation through shared
saturation, EQ, wet returns, sends, muted input, skipped/stopped owners,
noncoincident support and selected contributions/aliases. Stable per-sample
callback records retain stage, concrete clock, output voice and permissions.
A 1,024-frame unity block succeeds with only 32 preparation units remaining;
the transfer pass must actually disappear. Inactive routes consume no gain
program, and nonfinite input is refused at admission.

The full matrix's executable is SHA-256
`e60f7026ed08744f34e1281ef1511b9f19431a9d836d580c47c6bcb4a7484891`.
After two Clippy-directed iterator changes preserving channel summation order,
the final clean executable is SHA-256
`c883a179ad921ac6e16b086ec8d17fae5c249d162786d8f0e7c6a8bb0990f658`.
The final root-sound continuation uses that copied executable. The full matrix
is retained against its actual earlier source phase, not relabelled as a new run.

## Remaining failures and diagnostics

The three matrix failures occur before completing their limited PCM comparison:

- All-owner refinement: Source/Sequence, revision `cut-three`, limited `0+511`,
  plan work per read.
- Selected play: revision `prefix-two`, limited `0+4096`, structural query work.
- Nested selected play without root sound: revision `prefix-two`, limited
  `0+4096`, sound placement work.

The root-sound test runs both untreated and shared-processed root sound through
child Split/refinement with complete authored/faded/limited PCM. Its pass must
not be generalized to the unfinished all-owner or selected-play cases.

A temporary current-source trace reproduces the selected baseline failure. At
its last 16-million-unit snapshot, prepared transfer applications account for
4,102,179 charged units and source-occurrence construction for 1,409,368.
Both preparation passes still retrieve clock scopes, and historical binding,
placement, gate and query work repeats. These are executed charges at that
snapshot, not a complete failure attribution or a comparable speedup ratio.
Do not remove charges, increase the 16,777,216-unit ceiling, widen support or
retain unaccounted queries to make the matrix pass.

Tracing is removed byte-for-byte. `stages.rs` remains 164,819 bytes, SHA-256
`87b62882b84f510e96a94801da3e1febdeb894110fd4cee15a1de099f89c6d78`.

Retained setup/check failures: one runner invocation placed options after its
positional label and failed before launch; the first strict check found an
inherited formatting line, and the second found two new range-loop lint
findings. Corrections use no suppressions. Switching an isolated source copy
and the checkout through one Cargo target also reused stale Plan artifacts;
the failed diagnostic build is retained, followed by explicit invalidation and
a fresh successful Core/Plan/Audio build. Never change valid source to match
those stale artifacts. Pin executable checks before another build replaces
their target, and invalidate copied-workspace path artifacts before switching
source roots.

## Scope

The optimization is independently committable. The larger general Split
candidate remains uncommitted. Store/history joins, complete sound transport,
native commands/receipts/macros, emitted media and performance qualification
remain owed. The goal is active, coverage remains about 85%, strict completion
10/24 and delivery gates 0/7.

Raw runs are in `/tmp/deadpan-constant-block-2026-10-26`.
The [published inventory](../../tools/media-qualification/evidence/2026-10-26-constant-sound-blocks/inventory.json)
retains 69 gzip payloads, 1,867,041 stored bytes and 7,482,855 expanded bytes:
source guards, commands, outputs, original failures, exact overlay and executable
identities. Every payload's compressed/expanded hash and size verifies.
Inventory SHA-256:
`b19f84c6e85f3c9eff2dab49d89809523b86bf11d3e45fccf25ecdb5ee19e6e4`.
Executables remain local.
