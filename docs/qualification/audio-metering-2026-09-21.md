# Audio measurement qualification, 2026-09-21

The informational 48 kHz stereo meters are implemented in `deadpan-audio`.
The compiled headless harness passed 18 generated fixture cases and seven file
failure checks. This is partial DP-09/Gate A evidence. No production limiter,
gain reduction report, app audio path or final export master is qualified here.

Evidence: [retained run](../../tools/audio-metering-qualification/evidence/2026-09-21/),
[final numeric results](../../tools/audio-metering-qualification/evidence/2026-09-21/probe-final/qualification.json),
and [contracts/reproduction](../AUDIO_METERING.md).

## Environment and scope

Apple M5 Max, Mac17,7, 128 GiB, macOS 26.5.2 build 25F84; Rust 1.97.1.
The base source revision is `19922f89e40e1f8b3e73b1671c24322f1c9efb4d` plus the
meter changes identified by the retained source manifest. Release compilation
uses the existing qualified LGPL FFmpeg 8.0.3 prefix. The meter algorithms are
safe Rust and introduce no runtime dependencies. The report-writing example
reuses the workspace's existing pinned `tempfile` as a development dependency;
no third-party versions changed.

The independent developer reference is Homebrew FFmpeg 9.0.1, a GPL build.
It is neither linked into the app nor approved for distribution. Its complete
version/configuration and executable hash are retained. The fixture generator
uses Python's standard library; no runtime is added to the end-user product.

Input is generated raw f32le interleaved stereo at 48 kHz. Reports record the
SHA-256 of actual input bytes, algorithm identities, frame/window counts,
measurements and elapsed/processing times. Fixture PCM stays in scratch; no
official EBU audio is copied or redistributed.

## Observations

All defined integrated readings differed from FFmpeg's independent `ebur128`
result by at most **0.04912 LU**. The reference prints one decimal place, so this
is agreement at that reporting precision, not a sub-0.05 LU absolute error
claim. Published expected-value checks use a 0.1 LU tolerance.

| Generated case | Deadpan integrated LUFS | FFmpeg LUFS |
| --- | ---: | ---: |
| EBU 1, stereo 1 kHz at -23 dBFS | -22.993297 | -23.0 |
| EBU 2, stereo 1 kHz at -33 dBFS | -32.993297 | -33.0 |
| EBU 3 and 4, low-level regions around a louder section | -23.013869 | -23.0 |
| EBU 5, -26/-20/-26 dBFS sections | -22.978657 | -23.0 |
| Opposite-polarity stereo, -23 dBFS | -22.993299 | -23.0 |
| Full-scale 997 Hz in one channel | -3.010280 | -3.0 |
| 20 Hz stereo at -23 dBFS | -36.966563 | -37.0 |
| 10 kHz stereo at -23 dBFS | -19.649118 | -19.6 |
| Silence, below-gate, short and empty input | Undefined | -70 sentinel |

The generated EBU true-peak cases 15 through 19 measured -6.020600, -5.975909,
-6.313275, -6.026489 and +3.029073 dBTP. All meet their published +0.2/-0.4 dB
tolerances. Rust integration tests additionally synthesize transient cases 20
through 23 at all four decimation offsets. They test the final FIR tail, sample
peak floor, independent channels, chunk boundaries and rejected input. The
compiled harness checks FFmpeg against the same published expectations for these
five faded waveforms. Both implementations pass those expectations.

The unfaded 10 kHz tone exposes a separate limitation: Deadpan reports
**-22.703947 dBTP**, while FFmpeg reports **-22.2 dBFS**, a 0.504 dB difference.
The initial attempt to require every arbitrary waveform to agree within 0.4 dB
failed and is retained in `probe-comparison-failed`. That is not a published
inter-filter tolerance for this abrupt transient. The final harness keeps every
comparison and explicitly reports `high-10000` as a diagnostic mismatch;
published reference-case assertions remain separate. No uniform sub-0.4 dB
accuracy claim or final-master safety claim follows from this implementation.
FFmpeg's empty-input 0 dBFS sentinel is also recorded as an inapplicable peak
comparison, not treated as a measured disagreement with silence.
An independent complete finite-sinc edge scan measures -22.557340 dB at sample
coordinate 1.2725024, between the two finite-filter readings. The official FIR's
steady-state response is about -22.983 dBTP. The numerical review therefore
identifies boundary reconstruction sensitivity, not a coefficient or streaming
implementation fault. The retained edge scan is diagnostic, not an
interval-certified global maximum.

The loudness tests also cover arbitrary chunk partitions, frequency response,
strict gates, long silence, late invalid samples, cancellation and exact final
window boundaries. Both APIs preserve state after rejected admissions.
Short clips can have a peak while integrated loudness remains undefined.

The raw-file harness rejected partial stereo frames, NaN, infinity, magnitude
above 16, and a sparse file one frame beyond the 24-hour budget. None created a
report. A second write to an existing report failed without changing its bytes.
An injected 32-byte child file-size limit forced a real report-write error;
neither the destination nor the unpublished temporary file remained. The
same-directory temporary report is synced before no-clobber publication.
All successfully measured fixture hashes matched the unchanged input files.

In the initial retained `probe` run, the 100-second EBU 4 fixture used
**0.358143 seconds** for both meters together,
of timed processing, about 279 times the fixture duration. Nonempty cases in
this run ranged from roughly 257 to 359 times duration. Timing excludes fixture
generation and FFmpeg; processing time excludes file reading/hash work and
report serialization. These small local observations are not callback deadlines,
worst-case latency, concurrent playback or product performance qualification.
At the admitted 24-hour maximum, loudness numeric storage is bounded to
7,065,576 bytes plus state/allocator overhead; the test checks capacity without
processing a full day. No measured process-RSS claim is made.

## Limiter research retained as failures

The [research records](../../tools/audio-metering-qualification/evidence/2026-09-21/limiter-research/)
contain three gain-envelope experiments over 35 float32 fixtures. The test
reference was a 32-phase, radius-256 Kaiser reconstruction. The JSON field
`oracle_dbtp` names that finite reconstruction, not an ideal continuous-time
oracle. No experiment is linked to production.

| Candidate detector | Failed fixtures | Worst measured output |
| --- | ---: | --- |
| 8-phase BH4, radius 64, 512-frame attack bound | 7/35 | +1.334007 dBTP |
| 8-phase BH4, radius 128, 1024-frame attack bound | 5/35 | +0.227951 dBTP |
| 8-phase Kaiser, radius 256, 2048-frame attack bound | 0/35 against the same filter family | -1.231361 dBTP |

A subsequent independent [full-sinc audit](../../tools/audio-metering-qualification/evidence/2026-09-21/limiter-research/ideal-sinc/README.md)
also rejects the Kaiser candidate. Complete finite, zero-extended sinc sums find
five violations, including +5.352418 dB for alternating input and +3.792635 dB
for the amplitude-16 23,990 Hz tone. The latter is an interior witness at sample
coordinate 6084.573545641963, not an end-of-file artifact. Independent 80-digit
evaluation confirms all five witness amplitudes. All 35 exact generated WAVs,
hashes, scripts and results are retained. This adversarial reconstruction is
separate from the standardized finite dBTP measurement and actual DAC behavior;
its failures are not silently waived.

The first two fail the required -1 dBTP ceiling on near-Nyquist input. The third
uses the same kernel family as its reference, so passing does not provide an
independent qualification. A gain-slope bound can prove the candidate's own
finite interpolation points while another reconstruction still exceeds the
ceiling. This is why the reporting meter is separate from limiter design.

The first experiment stopped on an assertion; the retained parameterized reruns
record every case instead of hiding later failures. A first harness build also
found unsupported SHA digest formatting and was corrected. Two focused test
invocations initially omitted the required media-prefix environment; their
properly configured reruns passed. These are development failures, not waived
product requirements.

## Review and remaining qualification

The independent numerical review checked coefficient transcription, phase
history, final context, weighting/gates, budgets, chunk behavior and test scope,
and found no correctness issues. General review led to atomic report publication
and explicit reference peak checks, with injected write-failure evidence. The
subsequent actual comparison failure remains a reported limitation as described
above, rather than increasing an arbitrary tolerance to make it pass.
The repository gate passed formatting, strict
workspace Clippy, all **781 tests** (zero failed or ignored), workspace build and
`doctor`. General review and the final source manifest are recorded in the
evidence directory.

GUI, native lifecycle and device tests were deliberately not repeated: this
change has no UI, device or startup path. Existing visual/keyboard evidence
remains separate. Official EBU programme-file conformance, full EBU Mode controls
and displays, a qualified limiter, limiting reduction, AAC post-encode peaks,
full voice processing, final preview/export integration and listening acceptance
remain open. The official EBU archive was unavailable to automated retrieval
(HTTP 403), and its restricted redistribution terms are not replaced by these
generated tests.

Primary references: [ITU-R BS.1770-5](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf),
[EBU Tech 3341](https://tech.ebu.ch/files/live/sites/tech/files/shared/tech/tech3341.pdf),
and [EBU test-sequence terms](https://tech.ebu.ch/files/live/sites/tech/files/shared/testmaterial/use%20of%20EBU%20AUDIO%20test%20sequences.pdf).
