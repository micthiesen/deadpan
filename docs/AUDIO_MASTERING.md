# Mastering qualification boundary

Deadpan's shared `LimitedAudio` reader applies a bounded finite oversampled
limiter to the current edge-faded bus. [Sequence audition](PLAYBACK.md) and
headless `inspect-audio --limited` use that reader. This is an implemented
limited audition path, with voice effects, sends, the full group mix, listening
qualification and encoded output still required before it becomes a final master.
The [limited-audition qualification](qualification/audio-limited-2026-09-24.md)
records finite checks, exact read parity, preparation costs and remaining limits.
The earlier [gain-search experiments](qualification/audio-limiter-gain-search-2026-09-24.md)
remain unadopted research. No full-product requirement is complete.

## Product behavior

[Specification Sections 10.2–10.4](spec/DEADPAN_SPEC.md#102-fixed-default-effect-order)
remain authoritative. Apply the limiter to the final policy-resolved stereo bus
after voice treatments, sends and group processing, before output conversion.
Default to a measured −1 dBTP ceiling. Preserve source level and authored dynamics;
do not normalize words, lift quiet reactions, insert a blanket fade, or replace
limiting with constant attenuation of an entire program. Report reduction and
integrated loudness informationally. Monitoring volume follows mastering and
does not alter exported gain.

The input retains exact 48 kHz allocation, authored silence and permitted tails.
Mastering cannot add samples, fill a silent Hold, erase a tiny fragment merely
to pass a peak test, or restart an envelope at every beat. The existing
`StageAudio::read_edge_faded` inspection result is explicitly before voice effects.
It cannot be relabeled as a final mix when those stages are still absent.

## What the ceiling claim means

The required limiter uses a tested oversampled true-peak implementation. Pin its
filter coefficients, phases, rate, guard, gain law, output rounding and algorithm
identity. Independently measure the **final emitted PCM**, including complete
filter context, using the published BS.1770 estimator and a separately specified
reconstruction path. Testing only the detector's intermediate signal or the
limiter's own coefficient family is insufficient coverage. The eventual decoded
export also needs its own peak, timing and channel verification.

[ITU-R BS.1770-5 Annex 2](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf)
describes true-peak estimation, including finite filtering and oversampling
under-read. [EBU Tech 3341](https://tech.ebu.ch/docs/tech/tech3341.pdf) supplies
meter requirements and test tolerances. Neither prescribes Deadpan's gain law
or guarantees every possible physical converter. Meter conformance, PCM
limiting, encoded-file behavior and audible quality are distinct measurements.

Retain complete, zero-extended sinc reconstruction as a broader stress diagnostic.
It includes the entire finite signal and may expose a failure missed by finite
meters. It is not a normative requirement to prove an arbitrary continuous-time
reconstruction for every input or converter. Known failures must stay named,
measured and assessed against the declared output paths and dynamics tests.
A failure of a required path or quality test blocks adoption; a stronger
diagnostic alone must not silently become an unbounded new product requirement.
Passing that diagnostic also cannot excuse unwanted muting, excessive gain
reduction, slow seeking or missing encoded-output qualification.

The current numerical sinc oracle uses FFT scans, direct refinements and
curvature/tail allowances. Its global upper values are numerical evidence,
not formal interval certificates. Independent high-precision point checks
confirm witnesses, not the entire real-time maximum.

## Bounded preparation and shared output

Implement mastering on a preparation worker in `deadpan-audio`, consuming the
policy-resolved stereo bus. An adapter from the current `StageAudio` path would
not complete the missing voice/mix stages. Preview and export must consume the
same canonical master samples. Keep source decoding, gain preparation, metering,
allocation and cache work off the device callback. Reuse the existing cancellation, cumulative work
and residency admission patterns; report failure when admitted work cannot
produce a verified result. Never substitute an unverified pass or a different
export algorithm after a budget expires.

Define gain history and detector context on an absolute project sample grid.
Prove the read halo for the chosen implementation, including numerical block
dependence, or retain exact checkpoints and replay from them. A request boundary,
seek, split, Repeat or Hold is not an analysis reset. Compare continuous, irregular,
shuffled and cold reads, including real source and Preserve preparation. Cache
identity must cover the immutable revision, transitive media/layout/policy
dependencies, gain settings and algorithm. An old envelope is not valid after a
relevant edit merely because the requested PCM range looks unchanged.

Independent local gain reduction is not monotone for signed reconstruction:
removing one contribution can remove cancellation and increase a peak. Thus a
post-solve clamp, rounding pass or envelope minimum needs its own final-output
check. The experiments retain a two-sample counterexample. A finite gain
recurrence's history bound does not bound a separate global sinc detector.

## Current limited reader

`LimitedTile` pins `deadpan-master-kaiser64-bs4-trigger17-q32-four-pass-fft1280-v1`.
Its detector combines the published four-phase BS estimator, original sample
magnitudes and 15 fractional phases of a radius-64 Kaiser-windowed sinc bank.
The exact coefficient table lives beside the kernel; the pinned native
Signalsmith FFT evaluates fixed 1024-frame tiles using 1280-point transforms.
The final checker uses direct FIR arithmetic over the actual emitted f32 PCM.

Four fixed correction passes use linked stereo gain, a radius-64 cap minimum,
Q32 integer attack/release recurrences and an accumulated f64 multiplier applied
to the original f32 bus. A peak above −1 dBTP triggers a −1.7 dB target; otherwise
its cap remains unity. This guard is a measured design choice, not a standardized
gain law. A fully unity correction is an exact fixed point, so further identical
passes are skipped. Validated all-zero context preserves its signed-zero bytes
directly. Neither optimization changes the finite filter or gain recipe.

Every returned tile passes the final published-FIR and Kaiser-bank ceiling
checks, including project-edge reconstruction anchors. Failure rejects the tile;
there is no clipper, normalizer, alternative solver or unchecked output fallback.
An independent Blackman-Harris radius-96, 32-phase reconstruction path is a
qualification probe, separate from the runtime verifier. Complete-sinc stress
results remain separate diagnostics. Gain motion, guard-threshold audibility,
subnormal behavior and full listening acceptance are not established by finite
peak checks alone.

`LimitedAudio` owns one immutable `StageAudio`, four canonical 8192-frame cache
tiles and at most two tile visits per public read. Cold tiles request 37440
past and 21056 future samples, clipped only at the real project boundaries.
That includes four passes of exact gain influence, cap/detector support, FFT
tile dependence and the final direct verifier. Intermediate context endpoints
never become verification boundaries. Each context is bounded to 131072 stereo
frames; a full interior tile currently reads 66688 bus frames.

Twelve additional cache entries retain exact bus ranges of at most 8192 frames
each. A context is split from its required start, with no outward rounding.
Adjacent interior tiles can reuse seven complete bus blocks and prepare only
9344 new frames. Every reuse still re-admits all dependencies. Keeping exact
range keys matters: expanding the final range could encounter unavailable media
or unsupported processing beyond the limiter's required support. Bus residency
is bounded to 98304 stereo frames, separately from the 32768 final cached frames,
their gains/metadata, active context and existing source/stage caches.

One request shares its deadline across cache validation, all bus reads, kernel
passes and publication. Source, stage and plan-work counters span cache validation
and bus preparation; the kernel's fixed pass count and context/output bounds
separately bound its numerical work. A request visits at most two kernel tiles.
Every cached tile retains complete transitive source/layout fingerprints,
including hidden Preserve inputs, and re-admits them through the exact plan's
provider. Input changes within a request fail atomically. Device callbacks do
none of this work. Monitor gain follows the limited canonical samples and never
changes authored or exported gain.

The headless result identifies its limited edge-faded stage, implemented order,
actual per-sample gain and the full verified tile ranges. `maximum_reduction_db`
is informational. Existing raw, time-mapped and edge-faded inspection APIs retain
their distinct meanings. The reader is shared infrastructure for eventual export;
no encoded file path or completed voice/mix graph is implied.
