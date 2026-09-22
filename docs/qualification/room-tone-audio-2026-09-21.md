# Exact room-tone preparation, 2026-09-21

This increment renders an explicitly authored room-tone source range through
short exact crossfades, including repeated gaps and nested retimes. It preserves
Hold duration and keeps room tone distinct from silence. The result is still
`time_mapped_pcm_before_effects`; native audition, full effects and export are
not qualified. Base revision: `320ea14b9c416ffc963d0d6501f8ff66901e39e0`; its
[macOS CI run](https://github.com/micthiesen/deadpan/actions/runs/35669940432) passed.
No persisted schema, normative specification, dependency version or native
unsafe code changed.

## Contract and environment

The [room-tone contract](../ROOM_TONE_AUDIO.md) specifies the exact source
extent, 2 ms overlap shortened for small fragments, unity-sum linear weights,
first-pass behavior, absolute modulo phase, and full intrinsic Hold history.
Loop timing never derives from rounded storage length. One worker cache shares
residency, depth, aggregate preparation limits, cooperative deadlines and
transitive source/layout identity with Preserve stages. Ordinary source-only
inspection retains its previous explicit rejection of room-tone processing.

Apple M5 Max, 128 GiB unified memory; macOS 26.5.2 (25F84); Rust 1.97.1;
Apple Clang 21.0.0; pinned LGPL FFmpeg 8.0.3. Tests use the workspace development
and test profiles. OS cache and power were not controlled for performance
qualification. No latency, total-memory, listening or device claim follows from
these functional checks.

## Tests

Six pure loop tests cover constant PCM level and exact integer crossfades,
fractional 44.1 kHz-derived periods, an intentionally incorrect rounded-period
reference, irregular partitions and random seeks, tiny fragments, skipped empty
cycles, coordinates above `2^53`, checked modular overflow fallback, cancellation
and invalid PCM/recipes. Absolute coordinates never convert to floating point.

A new plan test checks full Hold duration and repeat-gap duration through crops
in root, processing and virtual-signal queries, retaining distinct gap identities.
Five real-PCM integration tests independently compose scalar loop coordinates,
qualified resampling and canonical stretching:

- A 221-sample selection at 44.1 kHz has exact 48 kHz extent `35360/147` and
  overlap period `21248/147`, rather than a rounded 241-frame period recipe.
- Three plays with a distinct middle override and two room-tone gaps retain
  five cache identities and exactly 1257 output samples, with no trailing gap.
- Ordered Preserve/FollowSpeed operations and outer crops retain loop history.
- Changed explicit speaker interpretation invalidates direct and nested caches.
- A sub-sample room-tone Hold becomes actual PCM through Preserve; effect-tail
  policies are still rejected directly and through nested stages.

The project-host integration reads a selected 256-sample AAC interval through
an independently calculated 96-sample crossfade and 160-sample loop period. A
separate silent Hold remains zero and carries suppression ranges; subsequent
original speech is unchanged. CLI samples match host samples, and inspection
does not change snapshots, history or source qualification rows. The first test
compile used an unavailable `Subtree::single` helper; the test was corrected to
use the actual typed constructor before verification.

Fractional resampling/DSP reference comparisons admit at most `2e-6` absolute
error per channel. Partition, cached-replay and fresh-render comparisons remain
exact; the integer-phase AAC reference also compares exactly.

The exact repository gate passed: formatting, strict workspace Clippy,
**732 Rust tests, zero failures and zero ignored**, workspace build and doctor.
The final gate required no corrective rerun.

The actual-process probe registered the repository's synthetic AAC fixture in a
30000/1001 fps project. A three-frame room-tone Hold allocated samples
`0..4805`; a one-frame silent Hold occupied `4805..6406`. All 4805 looped samples
exactly matched an independent scalar 96-sample overlap of the actual decoded
256-sample selection. Fresh-process irregular partitions and native-app headless
queries matched at the start, loop seam and end. Silence remained exact zero
with explicit suppression. Following source audio retained its authored range
and resumed at frame four, with the correct first-sample phase `-2/5`; it matched
source-only sampling at that same project origin. Snapshot and complete logical
SQLite contents were unchanged, including two history rows and three revisions.
All 100 process commands met their expected exit status, including the deliberate
source-only rejection.

Independent numerical review inspected exact modulo, fractional/tiny loops,
crossfade segmentation, sampling bounds, crop history, provenance and reservation
cleanup, finding no actionable defects. Separate general review inspected the
complete production and test change and found no actionable defects. Its first
focused test invocation omitted the required FFmpeg prefix and failed setup;
the corrected invocation passed all five filtered integration tests. No findings
were left unresolved. Raw gates, PCM, commands, environment, source/binary/fixture
hashes and review records are retained in
[the evidence directory](../../tools/audio-qualification/evidence/2026-09-21-room-tone/).

No GUI, focus/keyboard or native startup smoke was repeated because this change
does not affect those surfaces. No native sanitizer rerun was needed: the native
adapters and unsafe code are unchanged, and the new loop implementation is safe
Rust.

## Remaining acceptance

Native source-range selection, audio-policy editing, audition and representative
ambience listening remain open. An explicitly authored range is a user choice,
not evidence that its content contains no speech. Small crossfade construction
does not establish perceived quality across every source. Effect tails,
ordinary edge fades, gain/effects, true-peak mastering, long-input preparation,
background scheduling, devices and final preview/export equivalence remain
unfinished. DP-09 and Gates A through C remain partial.
