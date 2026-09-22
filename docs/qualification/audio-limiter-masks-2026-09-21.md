# Conditioned limiter and suppression research, 2026-09-21

The joint gain/filter/suppression prototype remains unadopted. It passed a
42-fixture corpus, then failed two independently reconstructed rapid-mask cases.
This is retained research for DP-09/Gate A, not a production limiter or preview
and export acceptance result.

[Evidence](../../tools/audio-limiter-qualification/evidence/2026-09-21-masked/)
retains the coefficient candidates, scripts, proof calculations, exact audited
WAVs, hashes, meter readings and independent reconstruction witnesses. The
earlier [limiter failures](audio-metering-2026-09-21.md) remain separate.

## Candidate and bounded result

The prototype applies shared stereo gain before a symmetric 255-tap conditioner,
then applies the authored output suppression mask. It emits exactly the original
8192 frames at 48 kHz, with zero extension outside that interval. The conditioner
uses a 21 kHz cutoff and Kaiser beta 16; its exact integer delay is 127 samples.
The stored coefficient halves each sum exactly to 1/2 in rational arithmetic,
giving exact DC unity and a Nyquist zero. Its numerical 0–20 kHz ripple is about
0.000000365 dB, and its qualified numerical 22–24 kHz stopband upper bound is
-148.413 dB. These are coefficient measurements, not perceptual qualification.
Failed coefficient designs and all attempted Remez runs are retained too.

Gain constraints include raw output, the existing BS.1770 FIR phases and seven
fractional phases from an eight-phase radius-256 Kaiser reconstruction. Partial
suppression masks enter the combined coefficients explicitly. The tested gain
uses attack and release increments of 1/8192 with an internal -1.25 dB target
and an additional numerical guard. No automatic loudness matching is applied.

All 42 outputs passed the compiled informational meter. Independent complete
finite, zero-extended sinc reconstruction also found no ceiling violation in
those exact files. The largest refined peak was **-1.204713 dB** in
`tone-23000-4`; the largest qualified numerical global upper bound was
0.870912704, below the -1 dB linear ceiling of 0.891250938. The audit checks all
8192 samples, a 128-phase grid, refined extrema and tail/curvature allowances.
An 80-decimal-digit check confirms the worst refined witness. These ordinary
numerical bounds are not interval certificates or universal guarantees.

## Subsequent counterexamples

Three additional files apply a final every-other-sample silence mask to constant
stereo sources. All 4096 suppressed frames remain exactly numeric zero.

| Source amplitude | Full finite-sinc witness | Result |
| --- | ---: | --- |
| 0.1 | -13.509501 dB | Below ceiling |
| 0.8 | +2.705248 dB | Violates ceiling |
| 16 | +3.126769 dB | Violates ceiling |

All file hashes match the producer's report. Independent 80-decimal-digit sums
confirm the witnesses. A mask after the conditioner can recreate high-frequency
structure with long reconstruction contributions outside the finite detector.
Including that mask in the finite constraints does not establish the desired
independent reconstruction bound. These are deliberately severe stress inputs;
the result does not assert that ordinary speech cuts produce these magnitudes.

The finite-filter proof remains useful within its stated domain. Its
arbitrary-mask distance bound is 197.203151731 for the Kaiser phases; with input
magnitude at most 16, a 1/4096 downward slope leaves a positive numerator at the
-1 dB ceiling. That mathematical recommendation was not the slope used to
produce the retained 42 files. The proof cannot extend a finite detector's
guarantee to complete sinc reconstruction. Its corrected BS row support is
`[-6,5]`, with combined support `[-133,132]` and anchors `-5..N+5`; the earlier
conditioner note used a shifted phase convention. Retained proof notes identify
this distinction explicitly.

## Remaining work

Resolve suppression and reconstruction behavior before adopting this design.
It still needs explicit floating-point error bounds, meaningful chunk and
checkpoint/seek parity, bounded worker memory and cost, reduction reporting,
listening tests and shared preview/export integration. The prototype's full
array computations do not establish real-time behavior or long-project limits.
No production dependencies or end-user Python requirement are introduced.

Session-run metadata identifies the existing Apple M5 Max/macOS 26.5.2
environment and the compiled meter from source revision
`64ee95f1e03e3be780a88c4eb2e2b004518ff44f`. The archived run reports independently
record macOS/arm64 and developer NumPy 2.5.3, SciPy 1.18.0 and mpmath 1.3.0, but
do not bind the meter executable hash, its revision or the hardware model.
Retained manifests bind the exact scripts, coefficients and output identities.
