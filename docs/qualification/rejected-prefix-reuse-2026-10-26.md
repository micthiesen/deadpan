# Rejected prefix PCM reuse, 2026-10-26

The owned-prefix PCM cache and sibling-tile prototype is removed. Its final
real-WAV Split matrix has **three passes and four work-limit failures**, while
the [retained constant-block version](constant-sound-blocks-2026-10-26.md) has
four passes and three failures. In particular, the prototype loses the complete
processed root-sound refinement pass. No prototype code is committed, and no
acceptance criterion, requirement or gate receives credit.

## Experiment and review

The prototype retains owned ordered contribution PCM after shared constant
processing, with final gain applied only for a selected output. Exact keys
retain graph scope, parent/context/stop identity, sample range, permissions and
depth. Dependencies are re-admitted on reuse, including contextual contracts
for retained sound processing. Family ordinals stay within their graph scope.

A second version evaluates up to four sibling temporal outputs in input-chunk
order. Each extra output reads only an exact retained prefix; dynamic or evicted
prefixes discard that sibling. Unused completed tiles share the controller's
evictable residency and slot limits. Required allocation can discard unfinished
sibling histories. Existing work, depth, source, preparation and memory ceilings
remain unchanged.

Independent review corrected source-error suppression, optional inventory work,
later-chunk eligibility, unevictable sibling storage, delayed history release,
and required final-workspace starvation. The final review found no further
source issue. Those corrections do not establish a useful optimization: the
full workload still regresses.

## Actual results

Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies and the qualified
FFmpeg prefix. Cargo uses optimization level 2 with debug and overflow checks.
Every recorded invocation retains source hashes, command, environment, output
and exit. Executed copied binaries remain unchanged during each run. These are
correctness observations, not a calibrated timing comparison.

| Check | Result |
| --- | --- |
| Initial prefix-cache Audio library | 208 pass, zero ignored. |
| Initial full real-WAV Split matrix | Three pass, four work-limit failures. |
| Revised cache/batching focused tests | 48 pass, zero ignored. |
| Final revised Audio library | 214 pass, zero ignored. |
| Final revised strict check | Formatting and all-target Core/Plan/Audio/Store lint pass. |
| Final revised real-WAV Split matrix | Three pass, four work-limit failures. |
| Prototype removal | All 1,018 inputs covered by the preceding constant-block qualification match its final hashes exactly. Twelve files restored; six prototype files removed. |
| Restored root-sound witness | Pass, zero ignored, using the preceding copied executable and byte-identical qualified inputs. Sources and executable unchanged during the run. |
| Restored strict check | Formatting and all-target Core/Plan/Audio/Store lint pass. |

The final revised matrix fails at:

- All-owner Source/Sequence refinement: `cut-three`, limited `0+511`, plan work.
- Nested selected play without root sound: `prefix-two`, limited `0+4096`, plan work.
- Selected play: `prefix-two`, limited `0+4096`, plan work.
- Processed root-sound refinement: `root-sound-refinement`, limited `0+511`, plan work.

The initial version failed the root-sound case even earlier, at `stage-revision`.
Its original failures remain in the evidence. The final revised executable is
SHA-256 `56b3ffad0d665b176442af60feb0c4809c84e66c959f5c5a878a4602ab7346a2`.

The retained implementation and inherited Core/Plan/Audio/Store Split work are
unchanged. Foreign `fuzz/Cargo.lock` and `.claude/` entries are preserved. The
earlier four-pass/three-failure matrix remains the current qualification.

## Continuation

Target repeated historical sound-query construction. A safe owned dependent
query can retain its immutable plan through locked `self_cell` 1.3.0, but the
current routed reader consumes its query. A cache needs borrowed consumption
and a complete heap-metadata bound before implementation. Keep live occurrence
overlap, policy, gates, gain and source admission outside that raw-input cache.
Do not retain all historical queries, weaken overlap selection or reduce work
charges without removing their corresponding work.

Raw records are in `/tmp/deadpan-prefix-reuse-2026-10-26`. The
[evidence inventory](../../tools/media-qualification/evidence/2026-10-26-rejected-prefix-reuse/inventory.json)
retains both failed matrices, final library/lint results, review corrections,
the rejected source overlay and exact removal checks. Executables remain local.
All 56 compressed payloads verify their stored and expanded hashes: 2,314,835
stored bytes, 9,293,234 expanded bytes. Inventory SHA-256:
`08263cf0abe8318df04e9be09ff4759e4ce74a4044f606afe1f5ae2c56e1c61b`.
