# Informational audio measurement

`deadpan-audio` exposes independent integrated-loudness and true-peak meters for
the fixed 48 kHz stereo mix. Both consume immutable PCM. Neither changes gain,
normalizes passages, enforces a ceiling, or makes pre-effects source PCM a final
master. Specification Section 10.3's limiter, reduction reporting and final
preview/export integration remain open.

## Streaming contract

Construct each meter with an admitted frame budget from one frame through 24
hours (4,147,200,000 stereo frames). Feed contiguous ordered blocks of 1 through
256 frames. All samples must be finite and have magnitude at most 16. A rejected
or cancelled admission changes no counters, history or accumulated measurements.
Cancellation is checked at entry; an admitted block completes as one bounded
transaction. Run these meters on a preparation or analysis worker, never the
device callback.

The API cannot infer rate, channel layout, skipped PCM, or revision identity from
an untagged slice. The host must bind one immutable mix, resample/downmix first,
and maintain continuous order. A new analysis origin or seek requires a fresh
meter and replay from that origin. Chunk boundaries do not reset history.
`finish` consumes the meter. Empty analysis is permitted by finishing without a
push; it reports zero measured frames.

## Integrated loudness

`LoudnessMeter` implements the published 48 kHz K-weighting filters, unit stereo
channel weights, 400 ms windows at 100 ms hops, and two-pass BS.1770-5 gating.
Windows begin at analysis sample zero. Only complete windows are retained; the
last incomplete hop is discarded without extending the audio. Both gates use
strict comparisons: first -70 LUFS, then 10 LU below the absolute-gated mean,
with the absolute gate still in force.

The power ring holds 19,200 f64 values. The admitted duration reserves every
possible complete-window energy before processing. At the 24-hour limit this
means 863,997 energies and 7,065,576 bytes of numeric storage, plus small state and
allocator overhead. Push performs no further allocation. Each hop recomputes
the sum of the fixed ring so long silence cannot inherit subtraction drift.
Final gating scans the retained energies and allocates no report storage.

`LoudnessReport` contains admitted frame count, complete/gated window counts,
integrated LUFS and the relative gate. The LUFS fields are `None`/JSON `null`
when no complete window passes the absolute gate. The relative gate may itself
be below -70 LUFS. `LOUDNESS_ID` versions the algorithm. This is integrated-only
measurement, not a complete EBU Mode meter with momentary/short-term displays,
loudness range, controls and conformance certification.

## True peak

`TruePeakMeter` uses the published BS.1770-5 Annex 2 48-coefficient FIR in four
12-tap phases at 48 kHz, with f64 accumulation and fixed memory. It also retains
original sample maxima so an interpolated reading cannot under-read a sampled
peak. Both channels remain independent. Finishing evaluates the remaining 11
zero-context frames to include the complete finite FIR response, without
increasing the input count or producing audio.

`TruePeakReport` includes the algorithm ID, input count, per-channel sample and
true-peak amplitudes, and dBTP. Digital silence has a zero amplitude and `None`
for dBTP. This finite standardized reconstruction is not an exact continuous-time
oracle. Passing it alone does not qualify a limiter against other reconstruction
filters, difficult near-Nyquist signals, or the final encoded file.

## Qualification

The [measurement report](qualification/audio-metering-2026-09-21.md) records
generated EBU/ITU cases, a separate FFmpeg reference, throughput, failure paths,
review and repository checks. Rust tests cover chunk independence, final filter
context, stereo polarity, quiet passages, frequency response and transactional
rejection. These are mathematically generated fixtures, not the official EBU
audio archive or a claim of full conformance.

Build and run the developer harness using the qualified media prefix from
[development](DEVELOPMENT.md):

```sh
cargo build -p deadpan-audio --example measure_pcm --release --locked
cargo run -p deadpan-audio --example measure_pcm --release --locked -- INPUT.f32le NEW_REPORT.json
python3 tools/audio-metering-qualification/run.py --measure target/release/examples/measure_pcm --output /tmp/deadpan-metering-new-run
```

Input is trusted generated raw little-endian f32, interleaved stereo, 48 kHz.
The Rust harness rejects partial frames, invalid samples and over-budget files;
it writes and syncs a same-directory temporary report after successful analysis,
then publishes without overwriting an existing report. Ordinary write errors
remove the temporary file; interruption may leave an unpublished temporary file.
It hashes the bytes actually read. This development helper
does not replace the qualified source provider or secure immutable-asset
snapshot boundary. Python and the reference FFmpeg are developer tools only.

Primary references: [ITU-R BS.1770-5](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf)
and [EBU Tech 3341](https://tech.ebu.ch/files/live/sites/tech/files/shared/tech/tech3341.pdf).
