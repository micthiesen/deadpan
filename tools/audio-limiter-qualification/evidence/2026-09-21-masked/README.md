# Masked limiter research evidence, 2026-09-21

This directory retains one rejected mastering prototype and its bounded research
evidence. It does not qualify or implement Deadpan's production limiter.

| Retained directory | Original scratch location | Contents |
| --- | --- | --- |
| `conditioner/` | `/tmp/deadpan-conditioner-20260921` | Immutable conditioner design scripts, coefficients, response results and unmasked combined-kernel measurements. |
| `proof/` | `/tmp/deadpan-joint-proof-20260921` | Mask-dependent support derivation, conservative moment calculation and slope candidates. |
| `joint-audit/` | `/tmp/deadpan-joint-audit-20260921` | Forty-two verified WAV outputs, BS.1770 meter reports, complete finite-sinc audit and high-precision witness. |
| `mask-audit/` | `/tmp/deadpan-mask-audit-20260921` | Three verified alternating-mask WAV outputs and complete finite-sinc audit. |
| `prototype/` | `/tmp/deadpan-joint-limiter-20260921` | Original prototype scripts and parent result reports. Duplicate raw PCM and WAV trees are omitted because the audit directories retain the inspected WAV bytes and meter reports. |

All 42 original bounded cases report at or below -1 dBTP under the retained
BS.1770 measurements, and the independent finite-sinc audit found no violation
among those exact outputs. That result is fixed-fixture evidence only. The later
three-case mask audit found complete finite-sinc peaks of +2.705247806 dBTP and
+3.126769438 dBTP in the `0.8` and `16` cases. Those counterexamples reject the
prototype even though its finite internal constraints admitted the outputs.
BS.1770 measurement and complete ideal-sinc reconstruction are distinct tests.

The original README files are preserved unchanged. Their commands and several
scripts retain absolute scratch paths. Reproduce in a disposable copy, restoring
the recorded `/tmp` layout where required. The two audit scripts can run from a
copied directory with `--reuse-inputs`; running them in this evidence directory
would rewrite timing and environment reports. `prototype.py` additionally expects
the recorded conditioner path and a prebuilt Deadpan `measure_pcm` executable.

`SHA256SUMS` covers every retained file except itself. The 42-case parent report
binds `prototype.py` as SHA-256
`fe288ff9ba9b44a4f772433209b788b5413a031cbcebd21a5abbcbec2b272e5e`.
The mask-stress parent report does not contain a script hash; this archive binds
`mask_stress.py` (`3038dc088a24fd63507fd4397b16372e4d2cbfb01775b576b62a84ca802f2dd5`)
and that report through the common manifest without asserting missing execution
provenance.

No listening test, arbitrary-input proof, long-programme test, preview/export
integration, encoded-output verification, performance qualification or physical
reconstruction-device qualification was performed.
