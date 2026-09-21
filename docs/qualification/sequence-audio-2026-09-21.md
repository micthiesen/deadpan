# Plan-driven source audio, 2026-09-21

This milestone connects exact structural audio spans to verified original PCM
through `SequenceAudio` and a read-only historical project host. Its output is
explicitly `source_pcm_before_effects`. It does not establish final mixing,
native playback or export. The base revision is
`70d465a7d385d68710c82db438267da374f18465`; its separate
[macOS CI run](https://github.com/micthiesen/deadpan/actions/runs/35664122044)
passed. The normative specification and persisted schemas are unchanged.

## Environment and contract

Apple M5 Max, 128 GiB unified memory; macOS 26.5.2 (25F84); Rust 1.97.1;
Apple Clang 21.0.0; pinned LGPL FFmpeg 8.0.3 from the compatible-media prefix.
Only dependencies between existing workspace packages were added.

[Source-stage audio](../SOURCE_STAGE_AUDIO.md) documents exact phase origins,
original sample clocks, discrete half-open crop context, supported pitch policy,
bounded source retention and the historical receipt checks. Source-local rate
changes without an authored pitch policy, nonunity Preserve, room tone and tails
fail explicitly. Unknown speaker layouts remain errors. The native DSP adapter
has not yet been integrated into this reader.

## Verification

The exact repository gate passed: formatting, strict workspace Clippy,
**687 Rust tests, zero failures and zero ignored**, workspace build and doctor.
This includes ten new sequence tests and six host/CLI integration tests:

- Signed placement, natural 44.1 kHz source clocks, NTSC fractional phase,
  isolated seeks and bit-equal irregular partitions.
- Silent Hold insertion followed by untouched original speech, three total
  repeat plays with two gaps, and bounded seeking into a billion-play repeat.
- Nested FollowSpeed retimes checked against an independently written affine
  reference, structural crop filtering, one-frame physical reads and exact
  fractional selections containing one or no original sample positions.
- Unsupported processing preflight before provider access, cancellation and
  invalid ranges without source I/O.
- Actual qualified AAC compared with independently reopened original PCM,
  historical receipt binding after undo and asset-alias reuse with a cold reader,
  read-only coexistence with a writer, unchanged authored state, and explicit
  missing/corrupt/unqualified-source and speaker-layout errors.

The real-process probe created a 30000/1001 project, retained and registered the
repository's own synthetic AAC fixture, and compared `deadpan-cli inspect-audio`
with `deadpan-app --headless inspect-audio`. Both returned identical PCM around
the known impulse at sample 100. Wrapping its 120-frame source in three plays
with one-frame silent gaps produced 362 frames and 579,779 samples. Play starts
were samples 0, 193,794 and 387,587. Both gaps were silent, there was no trailing
gap, and splitting the second-play query into 73 and 183 samples preserved its
PCM exactly. Inspection left the authored document unchanged. All 18 process
commands succeeded.

Raw gate/probe logs, fixture and implementation hashes, probe source and review
evidence are retained in
[the evidence directory](../../tools/audio-qualification/evidence/2026-09-21-sequence/).
The initial formatting check found only module ordering; `cargo fmt` corrected
it before the recorded full gate. No failing production test was suppressed.

The focused timing review found no actionable defects and ran four additional
scratch tests covering half-sample ties/zero-allocation leaves, signed fractional
placement under retime and cropping, coordinates adjacent to `i64::MAX`, and
fractional original-trim rejection. That reviewer wrote the sequence integration
tests but reviewed separately authored production code. Its initial standalone
harness needed matching Cargo artifacts and native library search paths; the
corrected harness passed all four tests. Independent general review covered the
production reader, host/CLI, tests, Cargo wiring and coordinate helper with its
callers. It found no substantiated actionable defects in historical identity,
timing, rejection, bounds/cancellation or error mapping.

No GUI, startup smoke or listening session was repeated: no UI, focus, keyboard,
lifecycle or native-device code changed. Earlier
[native workspace observations](native-workspace-2026-09-21.md) remain separate.
This increment introduces no new device, codec or unsafe adapter code; previous
adapter sanitizer qualification is not presented as a new run.

## Remaining work

Full voice preparation must bind pitch-preserving DSP with exact fractional
extents and mixed nested policies, then implement fades, gain, treatments, room
tone/tails and oversampled master limiting. The single retained source currently
reopens on asset switches; this is bounded preparation, not a qualified playback
cache. Long-clip DSP, scheduling, transport/device clock, real listening, full
format/layout/signal corpus, and preview/export equivalence remain open. DP-09,
DP-21 and Gates A through C remain partial.
