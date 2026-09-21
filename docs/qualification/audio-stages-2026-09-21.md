# Continuous audio stage preparation, 2026-09-21

This increment connects exact audio plans to continuous pitch-preserving PCM
preparation. Each Preserve occurrence processes its selected child signal across
cuts with one canonical history; nested retimes retain their order and intrinsic
history through outer crops. The result identifies
`time_mapped_pcm_before_effects`. It does not qualify a final mix, native playback
or export. Base revision: `9bc3bb90ae45d7d4f1cacee1be230f0c1824038d`; its
[macOS CI run](https://github.com/micthiesen/deadpan/actions/runs/35667228967) passed.
The normative specification, persisted schemas and native DSP implementation
are unchanged.

## Environment and admission

Apple M5 Max, 128 GiB unified memory; macOS 26.5.2 (25F84); Rust 1.97.1;
Apple Clang 21.0.0; pinned LGPL FFmpeg 8.0.3. Tests use development/test profiles
and the existing native DSP adapter's optimized build. Existing pinned SHA-256
and JSON dependencies now also compute streaming source-provenance fingerprints.
No external dependency version changed.

The [stage contract](../AUDIO_STAGE_PREPARATION.md) records the separate final
root and virtual point grids, exact speed, complete-history crops, output-policy
validation, source/layout cache identity, PCM residency and shared work limits.
The deadline is cooperative between bounded operations. Source opening and
native initialization cannot be preempted at an exact wall-clock instant. This
is preparation-worker work, with no realtime latency or total-memory claim.

## Verification

Fourteen new plan tests cover point-grid ownership, fractional phase and storage,
opaque Preserve boundaries, nested history, mixed policy order, sparse overrides,
stable play identity, bounded billion-play lookup, partition parity and Holds
with no input samples. Twelve real-PCM stage tests compare independently composed
DSP/resampling references for:

- Continuous processing across two source cuts, distinguished from separate
  per-cut stretchers.
- Fractional NTSC input phase and exact 3/2 speed, distinguished from the ratio
  of rounded buffer lengths.
- Ordered mixed Preserve/FollowSpeed stages, nested Preserve and outer-crop
  seeks that retain earlier transient history.
- Silent Holds with no input-grid point and positive output allocation,
  repeated occurrences and sparse overrides.
- Cancellation, expired deadlines, native size/depth/residency admission,
  cache eviction and exact replay.
- Shared preparation quotas across many tiny repeated stages, retaining only
  complete child caches and completing through bounded retries.
- Explicit speaker-interpretation changes invalidating direct and nested
  caches, with exact parity against fresh renderers.
- Room-tone/tail Holds that disappear from an inner sample grid but gain output
  samples under slowing, rejected before source I/O.

The host integration adds a real qualified AAC Preserve read, stable JSON failure
codes and unchanged snapshots, history and source receipts. Its initial JSON
comparison failed on `f64` decimal parsing, while every PCM `f32` bit matched.
The corrected assertion compares all 512 sample bit patterns and all remaining
metadata. No PCM tolerance was introduced. Early compile/Clippy failures were
corrected before the final gate.

The exact repository gate passed: formatting, strict workspace Clippy,
**719 Rust tests, zero failures and zero ignored**, workspace build and doctor.
The final gate required no corrective rerun.

The actual-process probe retained and registered the repository's synthetic AAC
fixture at 30000/1001 fps, then inserted a Preserve with a three-frame child
selection and two-frame output. The exact rate remained 3/2; the root emitted
3203 samples. All samples matched between 256-frame reads and irregular
1/73/256/17/127-frame partitions across fresh processes. Standalone CLI and
`deadpan-app --headless` matched at the beginning, middle and end. Source-only
inspection returned the expected `AudioOperationUnsupported` error. The authored
snapshot and complete logical SQLite dump remained identical, including three
history rows, four revisions and the source receipt. All 71 process commands
met their expected exit status, including the intentional rejection.

Raw gates, commands, PCM, source/binary/fixture hashes, environment and review
records are retained in
[the evidence directory](../../tools/audio-qualification/evidence/2026-09-21-stages/).

## Review and remaining scope

Independent focused review found three defects in the initial renderer: output
policies could evade input-grid preflight, aggregate preparation work lacked a
shared cap, and descriptor-only cache hits ignored changed speaker layouts.
All three received production fixes and regression tests. Follow-up review found
no further defects, including reservation cleanup, reusable complete children
after errors and transitive dependency validation. The plan author reviewed the
separately authored renderer; the PCM test author did not write its production
implementation. Separate general review covered the new plan traversal,
renderer/source integration, CLI mapping and regression tests, with no
actionable findings. No review findings were left unresolved.

No GUI, keyboard, focus or startup smoke was repeated because this slice changes
only headless audio preparation. Earlier native aesthetics and navigation
evidence remains separate. Listening, device-clock behavior, full effects,
gain/fades, room tone/tails, true-peak mastering, long-input preparation,
background cache scheduling and preview/export equivalence remain open. DP-09
and Gates A through C stay partial.
