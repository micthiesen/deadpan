# Master gain search, 2026-09-24

Neither candidate is adopted. A global gain optimization can satisfy both finite
BS.1770 measurement and the retained complete-sinc diagnostic on 16 short files,
without the previous prototype's conditioning filter. Independent review found
that its first successful version nevertheless muted 1,090 active frames. A
constrained revision fixes that corpus failure but remains too slow and globally
dependent for interactive mastering. A faster finite-context alternative passes
short tests and repeatable seeks, but fails the longer coherent stress signal.

This is DP-09/Gate A research evidence. Production mastering, full voice processing,
encoded export and listening remain open. The [mastering boundary](../AUDIO_MASTERING.md)
keeps the specification's tested −1 dBTP requirement distinct from the stronger
complete-sinc diagnostic and records the next practical acceptance contract.

## Global shared-gain experiment

All 16 inputs have 8192 stereo float32 frames at 48 kHz. They include rapid exact
zero masks, opposed alternating samples at 0.1/0.8/16, quiet tone/DC, a hot
23,990 Hz tone, a silent gap, a permitted tail, two opposed impulses, one- and
two-sample Hard fragments, and the exact two failing WAVs from the
[post-mask experiment](audio-limiter-postmask-2026-09-22.md).

Apply one stereo gain after the authored mask, with linear knots every 64 samples
and the final endpoint. Sample caps, signed reconstruction constraints and
attack/release slopes participate in the same linear program. Maximize total
gain with a −1.35 dB internal target. Reconstruct final f32 from the original
input on each pass; do not filter after silence or clamp the solved gains.
Add violating complete-sinc and published finite-FIR rows as cutting planes.

The producer's 32-phase complete-sinc scan uses every sample, zero extension and
linear convolution. Acceptance uses its numerical between-grid/tail allowance
against the product ceiling, followed by the retained 128-phase oracle. This is
whole-fixture optimization, not independently optimized playback blocks.

| Version | Actual result |
| --- | --- |
| v1/v2 | Hot rapid-mask input exceeded the 32-solve budget. Partial results and failure records are retained. v1 did not retain per-round failure history. |
| v3 | All 16 files pass numerical peak checks; rapid-mask 16 takes 92.95 s. Near-Nyquist output gains reach zero and erase 1,090 active frames. |
| v4 | Selecting cuts across gain cells reduces rapid-mask 16 to 1.86 s in the full rerun. Near-Nyquist muting remains; alternating 16 still takes about 53 s. |
| v5 | A lower gain bound participates in the optimization. All 16 files pass both numerical checks and retain every active frame. Alternating 16 still takes 39.92 s for 0.171 s of audio. |

v3 and later admit at most 96 solves per case and a 180-second cooperative
deadline, passing the remaining time into HiGHS. These are not preemptive
native-call or peak-memory guarantees. No process RSS measurement was made.
Timings include global diagnostic work for the LP runs and may overlap other
experiments; they are observations, not isolated native benchmarks.

v5 first constructs and rechecks a positive uniform reference gain `u`. It
constrains every knot to at least `u/2` inside the solve and verifies the constant
reference against each added LP matrix. This limits extra attenuation relative
to that particular conservative reference to about 6.0206 dB, subject to measured
floating residuals. It is an experimental fidelity constraint, not a product
setting or proof of perceptual quality. A positive gain can still underflow an
arbitrary tiny f32 input; actual active-frame preservation was checked here.

The hot near-Nyquist v5 gain minimum is 0.010331085048034618, compared with zero
in v3. All 8192 active frames survive. The eight unchanged controls are the quiet
tone/DC, quiet rapid/alternating inputs, both tiny fragments, ordinary silent-gap
file and permitted tail. Their entire WAVs remain byte-identical. All original
zero samples remain zero, and shared gain reproduces every output byte.

## Finite-context alternative

The independent candidate uses exactly four post-mask correction passes. Each
measures actual f32 output with samples, the published BS FIR and 15 fractional
truncated-sinc phases. It dilates detector caps over affected input samples and
uses integer attack/release recurrences before applying shared cumulative gain.
The fixed-pass algorithm does not assume that signed peaks decrease monotonically.

| Detector radius | Short files passing all scans | Observation |
| --- | ---: | --- |
| 256 | 16/23 | −2 dB guard also attenuates an ordinary 0.8 tone. |
| 4096 | 17/23 | About 0.107 s median producer work per 8192 frames; broader peaks still fail. |
| 8192 | 23/23 | About 0.167 s median; support covers the short files and reduced cases have constant whole-file gain. |
| 8192, 65536-frame inputs | 1/2 | Ordinary 0.8 tone stays byte-identical; the coherent rapid mask fails. |

The longer rapid-mask output passes its finite bank and BS meter at approximately
−1.35 dB, while its complete-sinc numerical upper is **+0.987005 dB**. The directly
refined amplitude is 1.1198203898373151, so this is not merely a loose upper bound.
Independent high-precision evaluation of the retained WAV confirms a
**+0.982967 dB** witness at coordinate 65534.43516864303.
The full source, output, gain and detector reports remain in the archive.

Fixed absolute 8192-frame FFT tiles and conservative numerical context produce
five bit-identical shuffled reads, including one-sample requests, on a separate
262144-frame signal. However, a cold 257-frame request loads 246017 context frames
and takes 1.763 s. Left/right context is 131072/114688 frames. Source decoding,
Preserve preparation and device scheduling are not included. These results
demonstrate one repeatable bounded computation, not responsive project seeking.

## Independent review and evidence

The [retained archive](../../tools/audio-limiter-qualification/evidence/2026-09-24-gain-search/README.md)
includes frozen source versions, failure histories, final WAVs/gains, per-case
reports, compiled-meter results, audit scripts, exact source hashes and reviews.
No failing output was regenerated during retention.

The independent v3/v5 audit reconstructs f32 output from the shared knots and
checks the published FIR with exact integer convolution, including all trailing
context and the sample-peak floor. Fresh 80-digit direct sinc sums check reported
witnesses. Pinned SciPy CSR replay verifies every v5 gain floor and output byte;
the scalar audit's one-ULP interpolation difference is documented separately.
The compiled Rust meter also passes all 16 v3 and all 16 v5 outputs.
Its retained executable SHA-256 is
`97216bd091c1f54b769ccfa0c709590fd03ae7e095b94e7fd12a2dc3ba78eaae`;
it was not rebuilt for these measurements. That establishes executable identity,
not a reproducible-build claim.

The numerical global sinc oracle is unchanged from the previous experiment.
Its FFT arithmetic allowance is empirical, not an interval certificate. Review
found a 5.91e-11 direct-evaluator discrepancy near an integer on the quiet
one-sample fixture; the fresh witness remains far below the ceiling. No old
oracle was changed or its previous result upgraded by assertion.

Runs used repository `034a35308e6652ab548c10a8999afd6f16f28d98`, Apple M5 Max,
macOS 26.5.2 arm64, Python 3.12.13, NumPy 2.5.3 and SciPy 1.18.0. Exact independent
audit runtimes and source identities are retained separately. Python and numerical
packages are developer tools; they are not app dependencies.

The repository gate passed: format check, strict locked workspace Clippy,
1,241 tests with zero failed or ignored, locked workspace build and CLI doctor.
Archive verification checks 778 retained files, 125 input/output/gain joins and
all 32 compiled-meter identities. Review strengthened the verifier to derive
active-frame and byte-identity results from WAVs, require unique complete meter
coverage and reject missing required retention inputs. A documentation review
also removed an unintended prerequisite to finish the voice graph before
implementing a limiter; the specification fixes signal order, not work order.

No native GUI, device, acoustic listening or encoded-file check was repeated for
this research increment. No Rust implementation, authored schema, application
control, ImageGen target or normative specification changed. The previously
verified playback path remains pre-master. Full implementation and acceptance of
DP-09 remain required.
