# Independent joint-limiter output audit, 2026-09-21

All 42 actual float32 stereo WAV outputs from
`/tmp/deadpan-joint-limiter-20260921/run-8192` were retained and verified against
their SHA-256 values in the parent's `results.json`. The recorded prototype hash
also matches the retained script. No output was regenerated or modified.

No fixture exceeded -1 dBTP under complete finite-sequence sinc reconstruction.
The closest observed fixture was `tone-23000-4`, at **-1.204712679 dBTP**
(amplitude **0.8704911618730317**). An independent 80-decimal-digit finite sum
confirmed that witness. The allowed amplitude is 0.8912509381337456.

Every fixture's numerical global upper bound, including the outside tails and
between-grid allowance, remained below the ceiling. The largest was
**0.8709127039726899**. This is ordinary floating-point numerical evidence, not
an interval-certified proof or qualification for arbitrary inputs.

The audit uses all 8192 stored samples in every reconstructed point, with zero
samples outside that finite sequence. It uses no Kaiser window, fixed sinc
radius, periodic extension, or limiter code. Eighty nonzero channels were
scanned at 128 fractional phases and their largest local extrema were refined
with complete direct sums. Four identically zero channels were checked exactly.
The maximum observed FFT versus compensated-direct discrepancy was
5.551115123125783e-16. The full audit took about 17 seconds on the recorded host.

Unscanned tails use the bound `sum(abs(samples))/(pi*distance)`. A conservative
sinc second-derivative bound supplies the between-grid allowance. The source
oracle's formulas and comments are retained in `finite_sinc.py`; no formal
floating-point rounding certificate is claimed.

All requested gap samples were checked independently in the WAV data: 512 frames
for each gap fixture and 20 frames for each tiny-gap fixture. These are exact
numeric zeros, including signed zero. Both fully silent fixtures are identically
zero across all 8192 frames, so their complete sinc reconstructions are also zero.
The partial-gap check applies to stored samples, not continuous silence between
those samples.

Artifacts:

- `inputs/`: verified WAVs, original report/prototype, and compiled meter reports.
- `results.json`: every fixture/channel, peak coordinates, bounds, and mask checks.
- `high-precision.json`: independent 80-digit evaluation of the closest witness.
- `summary.json`, `environment.json`: outcomes, versions, and script/input hashes.
- `audit.py`, `finite_sinc.py`: retained audit and complete reconstruction helper.

Reproduce using the retained inputs:

```sh
uv run --with numpy==2.5.3 --with mpmath==1.3.0 python /tmp/deadpan-joint-audit-20260921/audit.py --reuse-inputs
```

Use `audit.py`, not the helper's original standalone entrypoint. Omitting
`--reuse-inputs` intentionally recopies the parent's current files and rechecks
their hashes. The audit never invokes the parent's prototype.

No separate BH4 detector was run. This milestone does not qualify limiter
dynamics, arbitrary masks or inputs, long signals, listening quality, playback,
encoded output, or a physical reconstruction device. No repository files changed.
