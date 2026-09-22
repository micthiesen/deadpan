# Exact room-tone audio

`StageAudio` renders an authored `HoldAudio::RoomTone` from its explicit original
source range. It does not choose a range, classify it as non-speech, or replace
silence automatically. The selected source remains ordinary retained media with
revision-bound qualification and an explicit speaker interpretation. The current
engineering command path inserts that Hold as an ordinary subtree. Native range
selection, audition, and an audio-policy inspector remain open.

```sh
cargo run --locked -p deadpan-cli -- inspect-audio /tmp/example.deadpan --samples 0 256 --time-mapped
```

The output remains `time_mapped_pcm_before_effects`. The source-only inspector
still rejects looping, and effect tails remain unsupported by both inspectors.
Room tone and digital silence retain distinct authored and rendered behavior.

## Loop construction

First prepare the exact selected original interval on a 48 kHz stereo grid using
the qualified resampler and matrix. Original selection endpoints must be on
original sample boundaries. Let `L` be its exact length in 48 kHz samples. Store
`ceil(L)` points without changing `L`. Crossfade length is `F=min(96,L/2)` samples,
normally 2 ms and shortened for small selections. The overlap period is `P=L-F`.
These values remain rational even when the source rate is 44.1 kHz.

For absolute integer Hold-local sample `n`, compute `k=floor(n/P)` and
`x=n-k*P` exactly. The first pass uses the selected head at `x`. Later passes
with `x<F` combine the preceding tail at `P+x` with the new head at `x`, using
weights `1-x/F` and `x/F`. Other points use the head at `x`. Fractional positions
reconstruct through the qualified sinc sampler using the selected source's full
retained context. The complementary linear weights do not normalize source
loudness, add makeup gain, or clip. Engine identity:
`deadpan-room-tone-exact-linear-overlap-v1`.

Every point derives from the absolute Hold origin. No rounded loop durations are
accumulated, and empty cycles between integer samples are skipped without being
enumerated. Integer modular multiplication stays exact even when its direct
intermediate would overflow. Only normalized weights and fractional filter phase
convert to floating point.

## Duration, retimes and cache

The full Hold's canonical output stores `ceil(C*duration)` points, where
`C=48000*fps_den/fps_num`. Final project allocation still uses the exact absolute
round-even frame boundaries. Loop overlaps never add or remove authored time.
Each new Hold or repeat gap starts from its own local origin. Plan spans retain
the full intrinsic duration, including when a query or ancestor crops the Hold.

FollowSpeed reconstructs this prepared signal; Preserve consumes it with its
continuous canonical history. Mixed stage order stays explicit. Outer crops and
random queries keep the full intrinsic loop phase and filter context. Silent
Holds continue to suppress incoming energy after time mapping; room-tone Holds
do not acquire silence suppression merely because they are Holds.

Room-tone and Preserve entries share one bounded cache, residency accounting,
preparation quota and cooperative deadline. Keys distinguish node occurrences,
preceding repeat-play identities for gaps, selected sources, and intrinsic
durations. Transitive source/index/layout fingerprints are revalidated on hits.
The cache is private to one immutable plan and one executable's engine versions.

The current limits admit up to 1,048,576 prepared source frames and 8,388,608
Hold output frames, with the shared depth, memory and per-read budgets in
[stage preparation](AUDIO_STAGE_PREPARATION.md). This prepares the complete Hold
on a worker. It is not a callback algorithm or a long-input cache scheduler.
Failed preparation publishes no partial PCM or cache entry.

## Verification and remaining work

Pure loop tests check exact phase, overlap weights, constant-level preservation,
fractional periods, tiny ranges, cancellation, input validation and replay.
Real PCM tests cover a 44.1 kHz mono source, repeats/overrides/gaps, nested retimes,
crop history and layout changes. The host test checks an explicit AAC range,
separate silent time, untouched following source audio, and unchanged history.
See [qualification](qualification/room-tone-audio-2026-09-21.md).

Listening across a representative ambience corpus, authored policy/range editing
and native audition remain required. Ordinary edge fades, effect tails, gain,
effects, mastering, native playback and export are separate unfinished work.
