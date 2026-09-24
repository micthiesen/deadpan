# Mastering qualification boundary

Deadpan does not yet have a production master limiter. Sequence audition uses
the [pre-master playback path](PLAYBACK.md). The [gain-search experiments](qualification/audio-limiter-gain-search-2026-09-24.md)
resolve some earlier finite-fixture failures and expose others; neither tested
design is adopted. This document records the implementation boundary and the
qualification decision, not a completed audio feature or a smaller specification.

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

The next production candidate needs a declared finite detector and gain law,
final-f32 ceiling tests, quiet and ordinary-level controls, tiny fragments,
opposed stereo, exact silence/tails, hot near-Nyquist and longer coherent masks,
plus gain-modulation and listening review. Measure cold seek, throughput, peak
memory, cancellation and cache lifecycle. Keep the current failed candidates
out of playback; no production types, dependencies or controls are introduced
by this design record.
