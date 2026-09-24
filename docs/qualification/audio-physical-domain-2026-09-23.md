# Physical audio domain qualification, 2026-09-23

This increment builds on `3a8feafa85d7986b735015c4e65d42baccd8ddf4`, whose
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35950591253) passed.
It renders full physical audio context outside visible Partitions and transfers
that signal onto an explicit point grid without resetting its captured phase.
[The contract](../AUDIO_PHYSICAL_DOMAINS.md) records identity, range, admission and
remaining authored-binding requirements. Core 15/database 21 and retained-context
schema 1 remain unchanged.

## Behavior exercised

Eight plan tests cover the hidden-sibling counterexample, negative and NTSC
coordinates, actual Edit and Source-placement constraints, inherited boundary
owners, Preserve input history and flattened silent-Hold/Source-absence policies.
RoomTone retains its full duration and original local zero. A billion-play Repeat
keeps sparse overrides and stable complete occurrence paths. A cropped nested
Repeat gap retains its outer play and gap-after identity. Out-of-support and
exhausted query budgets fail. Existing root plan tests remain in the gate.

Twelve audio integration tests use actual qualified WAV decoding and canonical
DSP. Hidden Source reads differ from the overlapping root sibling and match the
intended source, including negative coordinates and irregular access. The NTSC
44.1 kHz fixture checks the exact `147/400` source-sample origin against an
independent resampler recipe. Nested Preserve matches its full canonical history;
absent Source input does not become a silent Hold. Ordinary Edit crops retain
their restricted filter support. RoomTone gaps preserve loop phase and recheck
cached context admission.

A one-sample read from physical support `[-6e18,6e18)` reproduced a `Range` failure
before the review fix. Ordered allocation validation now preserves the wide
domain without requiring its length to fit `i64`. The regression reads its
first, middle and final samples with no media-provider access. An additional
unit test checks raw PCM and endpoint fades over `i64::MIN..i64::MAX`.

Domain transfer tests compare the streaming path with materialized, premasked
physical-domain PCM through the existing resampler and another canonical Preserve
operation. Fractional signed positions, nonzero signal anchors, explicit silence
and out-of-support suppression remain exact. A changed source layout between
halo callbacks is rejected by the shared provenance observations. Foreign plans,
bad ranges, cancellation and stage limits fail. A delayed provider demonstrates
that callbacks share one cooperative deadline. Retained-envelope exhaustion
continues to use the existing tested envelope path; new authored binding cases
are not claimed by these unbound-domain integration tests.

The headless host test registers actual qualified managed media, performs Split,
deletes the left fragment, and reads the right fragment's hidden original samples
at negative root coordinates. The command returns typed PCM and physical-domain
metadata. It rejects invalid probes, oversized/empty intervals, out-of-domain
reads and checked-arithmetic overflow without changing the snapshot. After the
right fragment is deleted, a historical `FrozenAudioContext` still supplies the
original samples through `open_context`. Existing changed/missing linked-original
tests also exercise the domain reader's byte-admission failure path.

## Repository gate

The final gate passed on macOS 26.5.2 (25F84), Apple M5 Max, Rust 1.97.1, with
the pinned compatible LGPL FFmpeg prefix. Formatting, locked workspace/all-target
Clippy with warnings denied, locked workspace tests/build, CLI doctor and native
Metal startup/shutdown all passed. The suite reported **1,084 passed, zero failed,
zero ignored**, including 22 new tests. All 351 source and fixture hashes remained
unchanged through the gate.

[Evidence](../../tools/audio-qualification/evidence/2026-09-23-physical-domain/summary.json)
retains the counts, [command results](../../tools/audio-qualification/evidence/2026-09-23-physical-domain/report.json),
[source hashes](../../tools/audio-qualification/evidence/2026-09-23-physical-domain/source-hashes.json),
six compressed logs and the gate script. Changed Markdown links, all eight saved
design image/prompt records and all five unchanged original specification files
passed verification. No source changes followed this gate.

## Review

Three reviewers covered the complete change, physical-domain consumers and exact
clock arithmetic, and media admission/cache provenance respectively. The plan
author reviewed only the separately authored consumer changes; the general
reviewer covered the plan changes independently. Two findings were applied:
wide physical allocations now use ordered endpoint validation, and both nested
headless plan-limit errors retain the `AudioQueryLimit` protocol code. The wide
allocation failure was reproduced before its fix. Follow-up review found no
remaining issue in either change. No findings were left unapplied.

## Remaining work

The command is `inspect-audio-domain`, and the result is raw context PCM before
creative fades, voice effects and mastering. It is not an authored Hold edit,
application playback or an exported mix. Live binding ownership, exact phase
composition, compact Repeat birth/edit semantics, policy replacement, a bounded
prior-binding dependency graph and multi-context preparation remain required.
Later Preserve processing must query retained policy on both input and output
grids, including silent intervals that have no input-grid sample.

No GUI behavior changed. Native smoke testing checks startup and shutdown only;
the saved single-Original ImageGen boards and earlier native aesthetics/keyboard
reviews remain the design evidence. No product requirement or delivery gate is
complete.
