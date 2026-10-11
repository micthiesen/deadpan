# Shared group treatments after retained-clock moves, 2026-10-11

A one-frame insertion before a sound owner at 30000/1001 fps could make the
new group mixer reject constant saturation with `inconsistent nonlinear mix
clock`. The Original retains its previous sample phase while the independent
sound's automation uses its current owner clock. Those clocks legitimately
differ by a fraction of a sample, even when they describe the same constant
transfer function.

The mixer now accepts a shared memoryless bus when the evaluated treatment
programs agree at every overlapping sample. Equal affine clocks retain the
fast path. Different clocks require matching Q32 gains and ordered channel
stages, or both programs must mute, over the entire overlap. This runs under
the existing work/deadline
budget and before media access. Checking only endpoint gains would miss an
interior envelope or mute boundary.

A second case had no Original audio at all: a picture-only Source's retained
clock incorrectly constrained automation on its independent sound. The owner
query now proves zero Original PCM for silent Source/Hold leaves reached
without a stateful processor. Such a leaf supplies no mixing clock. The mixer
still reads and checks its PCM, rejecting nonzero or nonfinite results rather
than dropping them. Preserve history remains inconclusive, even when its
current flattened leaf is silent. Source admission and silence permissions
are unchanged.

## Verification

Apple M5 Max, 18 physical cores, 128 GiB; macOS 26.5.2 (25F84), Rust 1.97.1.
Native media uses the qualified FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4 prefix.

- Complete locked suites: **490 audio, 399 planning and 72 playback tests
  pass**, with none ignored.
- New decoded-WAV regressions cover constant saturation with and without
  audible Original audio, a picture-only owner's linear gain envelope,
  cold/partial reads, mute admission and an interior-only clock disagreement
  that rejects before source access.
- The planner test distinguishes silent transparent/identity retimes from
  stateful Preserve processing.
- Strict workspace/all-target Clippy with `ui-harness` passes.

The final four audio regressions were overlaid on archived commit
`3ded82d09dd31237476529195f50e76e1cb5fe5e` and run in a separate Cargo target.
All four fail there: three supported operations refuse with the old clock
error, and the fourth still gives that generic invalid-plan error instead of
the explicit unsupported-automation diagnostic. Source/archive, overlaid test
and executed binary hashes are retained.

The complete run exposed an obsolete residency fixture that allocated a bus
for a silent leaf. It now uses an audible Original and still proves the same
pre-media limit rejection, reservation cleanup after repeated provider errors,
and treatment-free inspection. Test-construction failures and all initial
regression logs are retained, including corrected test enum/API names and the
replacement of an invalid FitBeat audio fixture with explicit natural-rate
mapping. No numerical tolerance was relaxed.

The existing 54-picture nested group fixture also passes with the rebuilt
release CLI. Its 256-sample limited-preview window agrees with the independent
tone/WAV sum oracle within **1.451007e-8**. Render and `verify-export` check all
54 pictures and both full audio windows with verified zero sample offset:
82 compared blocks, minimum block SNR 20.945 dB and maximum level difference
0.17457 dB. This rechecks the shared export path; the moved-clock cases above
are verified through decoded-WAV tests.

Movie SHA-256:
`792acbc8f9555ed972a16832514c89d248ba8b0d2beb8177aab99c70b8676c5a`.
Release CLI SHA-256:
`752be5249cb1a10d4273d3f2e96f79e534ada35cb091ad9fca1708e734742c18`.

[Retained evidence](../../tools/audio-qualification/evidence/2026-10-11-group-clocks)
contains the source and binary pins, command receipts, initial failures,
complete passing logs, archived-baseline witness, actual MP4, complete fixture,
verifier report and review notes. Formatting and diff checks pass. Recheck with:

```sh
python3 tools/audio-qualification/evidence/2026-10-11-group-clocks/check.py
```

## Scope and remaining work

Review was performed by the main session without subagents. It covered exact
clock mapping, whole-interval agreement, conservative silence proof, bounded
work and memory, dependency admission, finite PCM checks and read invariance.

Genuinely different automation on audible inputs to the same nonlinear owner
still reports `different automation on independently timed inputs to one
nonlinear owner`. The worker does not select one input's clock silently. Full
group automation, EQ/filtering, arbitrary sends, oversampled saturation and
complete mastering remain open; DP-09 and DP-16 remain partial. This change
adds no live GUI, physical-device or listening qualification.
