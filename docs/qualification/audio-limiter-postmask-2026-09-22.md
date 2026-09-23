# Post-mask limiter experiment, 2026-09-22

The post-mask gain prototype is not adopted. Moving the shared gain after the
silence mask preserves zero samples, but does not resolve the reconstruction
failures found in the [preceding experiment](audio-limiter-masks-2026-09-21.md).
Four named cases, representing two distinct outputs, exceed the -1 dB ceiling
under complete finite, zero-extended sinc reconstruction. Both retained finite
meters report those same files below the ceiling.

This is DP-09/Gate A research evidence. It does not implement a production
limiter, change the specification, or qualify preview, export or listening.
[The archive](../../tools/audio-limiter-qualification/evidence/2026-09-22-postmask/)
contains all 16 output WAVs, compiled meter reports, producer scripts, pre-run
hashes, reconstruction code, numerical results and 80-digit witnesses.

## Signal and control

Each output contains exactly 8192 stereo float32 frames at 48 kHz. The chain is
the retained 255-tap conditioner, an experimental edge envelope or hard edges,
the exact silence mask, shared stereo gain, then float32 conversion. No filter
follows the mask. The conditioner and kernels are unchanged from the preceding
experiment. These experimental master envelopes do not establish the required
per-voice effect order in specification Section 10.2.

The gain constraints include raw samples, the BS.1770 four-phase FIR and seven
fractional phases of a Kaiser beta-10 radius-256 reconstruction. The internal
target is -1.25 dB with a 1e-6 linear guard. Caps are rounded downward to units
of `2^-32`; attack and release increments are respectively `2^20` and `2^19`
integer units. This makes their implemented gain slopes exact. Irregular chunks
with carried state reproduce those scalar recurrences. That check does not
cover detector preparation, random seek, checkpoint restoration or a PCM crop.
The floating-point detector cap calculation still lacks a full numerical error
certificate.

The first envelope, named `default96` in the retained files, applies a fixed
96-sample ramp even to single-sample fragments, reducing them by about 39.65 dB.
It is negative design evidence: specification Section 10.4 requires shortening
fades for tiny fragments. Passing after this attenuation cannot justify the
default behavior.

A separately retained `shortened-edge` candidate uses the fragment length `N`,
`f = min(96, N/2)` and sample-centered weights
`min(1, (i+0.5)/f, (N-i-0.5)/f)`. A one-sample fragment has unity weight; two
samples have weights 0.5 and 0.5. Its three rapid-mask outputs are byte-identical
to their hard-edge counterparts. This remains a candidate endpoint policy, not
an authored or production fade implementation.

## Measurements

All 16 files pass the compiled BS.1770 meter and the separate post-output
Kaiser scan. The latter uses the same kernel family as the gain constraints,
so its agreement is not independent filter coverage.

| Rapid-mask input | Edge mode | Complete finite-sinc peak |
| --- | --- | ---: |
| 0.1 | Hard or shortened | -13.509501 dB |
| 0.8 | Hard or shortened | **+2.706895 dB** |
| 16 | Hard or shortened | **+3.166365 dB** |
| 0.1 | Fixed 96-sample ramp | -53.154926 dB |
| 0.8 | Fixed 96-sample ramp | -35.093126 dB |
| 16 | Fixed 96-sample ramp | -9.072526 dB |

For the failing 0.8 case, the compiled meter reports -1.938200 dBTP and the
Kaiser scan reports -1.250010 dBTP. The amplitude-16 case reports -1.401400 and
-1.313597 dBTP respectively. The broader reconstruction failure survives the
post-mask gain change; it is not just failure to include the mask in detection.

The seven other outputs cover unmasked hot alternating samples, a hot 23,990 Hz
tone, quiet tone/DC, an ordinary silent gap, a permitted tail, and opposed
stereo peaks. No complete-sinc ceiling violation was found in these files.
Their qualified numerical upper bounds are below the -1 dB linear ceiling.
This small generated corpus does not stand in for a full signal, format,
listening, or encoded-output corpus.

All rapid-mask cases retain 4096 exactly numeric-zero odd frames and 4096
nonzero even frames. The ordinary 512-frame silent gap remains exactly zero;
the permitted-tail fixture retains nonzero output in that interval. Quiet
controls retain unity limiter gain, although the conditioner still changes PCM.

## Audit and provenance

The parent ran the previously independently authored complete-sinc oracle after
the new review agents stopped at their usage limit. No fresh independent-agent
reconstruction audit completed. The audit snapshots and verifies the producer's
WAV hashes, checks all 16 outputs, and reuses results only for byte-identical
channel data.
Eighteen distinct channels were evaluated. No producer output was regenerated.

Every reconstruction point includes all 8192 samples. The audit uses a
128-phase FFT grid, direct compensated checks, local refinement, explicit outer
tail bounds and between-grid curvature allowances. Eighty-digit direct sums
confirm the reported witnesses. The global bounds use ordinary floating-point
arithmetic and an explicit allowance; they are not interval certificates.
Complete sinc is a broader diagnostic reconstruction, not the published finite
BS.1770 estimator or a claim about every physical converter.

The producer pre-run manifests bind its scripts, coefficients, kernels and the
compiled `measure_pcm` executable before output creation. Retention rechecked
all these hashes, plus raw-PCM/WAV payload equality and audit-input equality.
The meter executable hash is
`97216bd091c1f54b769ccfa0c709590fd03ae7e095b94e7fd12a2dc3ba78eaae`.
This binds the observed executable; it is not a reproducible-build claim.
The run records macOS 26.5.2 arm64, Python 3.12.13, NumPy 2.5.3 and mpmath 1.3.0.
The repository was at `445fc57e66a860f938cfcc686aca12d4d8d34d46` during the audit.

Archive `SHA256SUMS` covers the retained files. Raw f32 files, duplicate audit
WAV snapshots, caches and the compiled executable are omitted. The exact WAV
payloads and executable identity remain available in the manifest and reports.
Reproduction instructions and immutable-file handling are in the archive README.

## Decision and remaining work

Do not promote either finite-detector prototype based on these results. The
next mastering design must address the retained rapid-mask failures without
silencing tiny fragments or filling authored silent intervals. Merely changing
gain order, repeating same-family meter checks or increasing attenuation in the
fixed fade is not evidence that this issue is resolved.

Finite-meter guarantees and the broader reconstruction checks must remain
separate claims. The eventual limiter still needs bounded preparation and
replay, reduction reporting, quiet-signal preservation, listening, encoded-file
verification and shared preview/export use. Per-voice fades and the authored
hard-edge policy remain open independently of mastering. No production DSP,
native window, device or application lifecycle code changed in this experiment;
the prior Rust gate and CI results do not qualify the new prototype.

The current repository gate also passed after retaining this evidence: format
check, strict workspace Clippy, all 794 tests, locked workspace build and CLI
doctor. Artifact verification checked all 50 manifest entries, all 16
WAV/raw-PCM/meter/audit identity joins, 32 high-precision witnesses, silence and
tail intervals, Python/JSON parsing and new document links. No native GUI,
device or listening check was repeated for these research/documentation files.

A subsequent independent archive-review attempt failed to load workspace
requirements before returning a review. The parent checked the retained scripts,
report claims and artifact joins directly. This checkpoint records that review
limitation; it does not claim fresh independent review or authorize adoption.
