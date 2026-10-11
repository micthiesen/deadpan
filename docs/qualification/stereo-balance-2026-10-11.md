# Stereo balance qualification, 2026-10-11

Deadpan now authors stereo balance on a beat or one Repeat play through
`:balance -50%`, `:balance +50%`, `left`, `right` and `center`. The typed
headless `set_audio` instruction uses the same command path. Whole-beat edits
record in macros and repeat with dot; all changes are reversible. The inspector,
command help and edit receipt name the direction and value.

This implements the spatial-balance stage in specification §10.2. DP-09 and
DP-16 remain partial: EQ/filtering, arbitrary sends, nonlinear group processing,
long-source preparation and complete mastering acceptance still remain.

## Signal and ownership contract

The exact signed integer range is -100 through +100 percent. Center preserves
sample bits. At negative values left stays at unity and right is multiplied by
`(100 - abs(percent)) / 100`; positive values do the reverse. There is no
crossfeed, normalization, timing change or inferred speaker layout. The native
center/off/0% command removes the stage and retains every other treatment.

Each owner's serialized balance stage is last, after its gain and saturation.
Owners still process inner to outer, so a child's balance precedes a parent's
saturation. Independent sounds retain their own scope; root balance reaches
root sounds once. Ranged gain and scoped gain transfers preserve balance.
A multi-play replacement refuses differing recipes instead of erasing them.

The shared authored bus feeds limited audition and Render. The stage is
memoryless and bounded to one balance plus one saturation per owner, with the
existing 16-layer limit. Center does not split exact dB accumulation. Frozen
audio contexts capture schema 9; older versions reject a nonempty balance stage.
Cached results retain the normal immutable recipe/dependency identity.

## Verification

On Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1:

| Check | Result |
| --- | --- |
| Complete core unit/integration suite | 1,047 passed |
| Complete audio unit/integration suite | 481 passed |
| App unit suite | 1,117 runnable tests passed across the full run and focused follow-up; nine existing opt-in tests remain ignored |
| Strict workspace/all-target Clippy with `ui-harness` | Passed |
| Normal app compile without `ui-harness` | Passed |
| Locked release app/CLI build and formatting | Passed |
| `audio-treatments` offscreen release replay, 120 Hz requested | 123 checks, 368 frames, passed |
| Gain offscreen release replay, 120 Hz requested | All 296 checks, 5,321 frames, passed |
| Public Render and independent preview/export verification | 54 pictures and two zero-offset audio windows passed |

Real decoded PCM tests cover exact center and endpoints, partial balance,
channel identity, inner/outer saturation order, split partitions, cold shuffled
reads, independent sounds and root ownership. Core tests cover bounded wire
admission, stage order, retained contexts, compound replay and inverse patches.
The native replay covers visible value, treatment preservation, center removal,
dot, macros, invalid values, stale and absent captures, one-play isolation and
Undo. Gain verification retains all waveform, axes, scrolling and keyboard
traversal assertions after adding the inspector teaching text.

Each replay additionally passes 21,884,016 production routing cases against
all 62 pinned Kestrel reservations. No new key chord was added. The live Kestrel
source was unavailable, recorded as `live_source_sha256: null`. The reviewed
routing snapshot adds only three balance parser witnesses. Generated command
help now contains 204 actions and 127 verbs.

The export fixture has two identical -10 dBFS, 1 kHz tone Holds followed by
Original content. The first is full-left; the second is 50% right. Limited
preview independently checks channel ratios and peaks. Public Render's output
has all 54 expected pictures, 58.005 dB minimum luma PSNR, 54.201 dB minimum
chroma PSNR, 82 audio blocks, 29.573 dB minimum block SNR and 0.0854 dB maximum
block-level difference. Both complete signal windows verify zero offset.
Repeated background pictures leave 58 neighbor comparisons unobservable;
those are explicitly reported, not treated as index evidence.

A separate Homebrew FFmpeg decode of the actual AAC produces 86,486 stereo
sample frames. No alignment, trimming, rate conversion or channel conversion
was applied. Two fixed 4,096-sample windows give:

| Start sample | Authored balance | Left/right peaks | Right/left RMS |
| --- | --- | --- | --- |
| 6,406 | Full left | 0.316407 / 0 | 0 |
| 25,626 | 50% right | 0.158177 / 0.316304 | 2.000574 |

The movie and decoded PCM are retained with hashes and FFmpeg version output.
The production path uses the qualified FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4
prefix; the independent decoder is recorded separately.

## Failures, review and limits

Initial checks caught the previous context-version expectation and an incorrect
new record-count expectation, then a missing app description match for balance.
These were corrected. Reference regeneration deliberately exits with failures
to require diff review; the subsequent registry tests pass. Self-review also
added balance to the macro recorder's command permission.

The full app run passed 1,116 tests and failed the named-takes test helper, which
could read a background status immediately before checking that the service was
idle, then unwrap that unrelated update as a take receipt. Its three tests
passed alone; the helper now waits for its exact ticket, and all three pass
again. Production take behavior was unchanged. The original failure is retained.
The nine ignored tests need opt-in real AI/Generated media or large-package
stress fixtures and were not rerun for this audio treatment.

Single-agent review covered the complete diff, ownership, serialization, cache
identity, capture checks and bounds. It was not an independent second-agent
review. Four retained offscreen captures were inspected: balance is readable in
the inspector, compact waveform/curve axes fit, and exact controls and fixed
Apply/Cancel actions remain available. Extra inspector teaching text uses its
existing scroll area. The replays retain screenshot-capacity and converging
layout-retry warnings; neither ignores a retry or reports an oscillation.
No live GUI, physical keyboard, IME, VoiceOver, listening or physical display
measurement was performed. Those retain the §29.1 owner scope.

## Reproduction and provenance

[Evidence](../../tools/audio-qualification/evidence/2026-10-11-stereo-balance/README.md)
contains scripts, complete compressed logs/replay reports, export comparisons,
actual output media, reviewed captures, environment, source/binary pins and a
hash manifest. `check.py` verifies their internal consistency and current source.
The release app SHA-256 is
`d3c39510fd6d8fc3c5316c2501bb70dd8dfac22efc59e57e23efae4565cfa78a`;
the release CLI is
`aad72479805d6f9fa74ddb795e28bfad9010da8c88af39e059c4ca513cfb6a68`.
The source starts at `6601347d517eec6ee6cd66ca90e79a11ab18225b` plus the
26 pinned source/reference files. Scratch and full captures remain at
`/tmp/deadpan-stereo-balance-20261011`. Cargo and replay work ran sequentially.
