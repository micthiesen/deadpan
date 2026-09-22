# Alternating-output-mask audit, 2026-09-21

Two of the three retained outputs fail the -1 dBTP ceiling under complete finite,
zero-extended sinc reconstruction. All WAV SHA-256 hashes match the parent's
`mask-stress/results.json`. No outputs were regenerated or modified.

| Existing output | Refined full-sinc peak | Peak amplitude | Outcome |
| --- | ---: | ---: | --- |
| alternating-mask-dc-0.1 | -13.509501457 dBTP | 0.2111178367 | Below ceiling |
| alternating-mask-dc-0.8 | +2.705247806 dBTP | 1.3654078340 | Ceiling violation |
| alternating-mask-dc-16 | +3.126769438 dBTP | 1.4333045248 | Ceiling violation |

Every other stored output frame is exactly zero in each WAV: 4096 masked frames
per file, both channels. Independent 80-decimal-digit complete finite sums
confirmed each channel's witness. These results demonstrate that the fixed
conditioner does not prevent this later output mask from recreating a signal
whose long sinc contributions exceed the finite detector's admitted ceiling.

The audit uses all 8192 source samples at every queried point, a 128-phase grid,
direct compensated-sum refinement of 16 local extrema per channel, and explicit
outside-tail and between-grid numerical bounds. It uses no Kaiser window,
finite sinc radius, or periodic signal extension. Ordinary numerical bounds
are not interval certificates; each large violation is already a sufficient
counterexample without claiming its witness is the exact global maximum.

`inputs/` retains the three verified WAVs and parent report. `results.json`
contains both channels' extrema, positions, hashes and bounds;
`high-precision.json` contains all six independent high-precision checks.
`environment.json` retains tool versions and script/input hashes.

Reproduce against the retained files:

```sh
uv run --with numpy==2.5.3 --with mpmath==1.3.0 python /tmp/deadpan-mask-audit-20260921/audit.py --reuse-inputs
```

Run `audit.py`; `finite_sinc.py` is the retained reconstruction helper. The
scripts never run the parent's limiter. This is counterexample evidence for
these exact output bytes, not universal qualification or a replacement design.
No repository files changed.
