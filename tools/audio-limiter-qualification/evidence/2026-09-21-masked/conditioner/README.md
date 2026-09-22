# Fixed conditioner experiment, 2026-09-21

The usable shorter candidate is `kaiser255-beta16`: a symmetric 255-tap Type-I
48 kHz FIR from SciPy `firwin`, with a 21 kHz half-amplitude cutoff and Kaiser
beta 16. This qualifies only the measured fixed filter, not a limiter.

| Stored candidate | Passband peak-to-peak ripple, 0–20 kHz | Worst refined stopband, 22–24 kHz | Exact delay |
| --- | ---: | ---: | ---: |
| kaiser255-beta16 | 0.000000364626 dB | -148.514313 dB | 127 samples, 2.645833 ms |
| kaiser511-beta16 | 0.000000194384 dB | -158.527951 dB | 255 samples, 5.3125 ms |

Both meet the requested 0.005 dB ripple and -120 dB stopband limits. The shorter
candidate supplies ample measured margin with less delay and support.

All coefficients are stored exactly on a 2^-52 grid. Symmetric even-index and
odd-index coefficient sums are each exactly 1/2, checked with rational arithmetic.
Thus the actual stored binary64 coefficients have exact DC gain 1 and Nyquist
gain 0. Every reported measurement follows this quantization and parity repair.
The maximum coefficient repair for the 255-tap candidate was 6.7461e-9.

`kaiser255-beta16.json` contains decimal float64 coefficients and their exact
integer representation. `kaiser255-beta16.f64le` contains 255 little-endian f64s.
Its SHA-256 is `8b4418568c73525050f7541cd4b8c8f8f4be8318e0f314260b63575d4aa3ce12`.

`results.json` retains all eight Kaiser attempts, impulse/symmetry/delay checks,
a 1,048,577-point response grid, refined stationary points, and numerical error
allowances. These are ordinary floating-point measurements, not interval proofs.
The 255-tap beta-12 and beta-18 attempts failed the stopband target. All six
`remez` attempts failed to converge at iteration 3; parameters and errors are
retained in `remez-attempts.json`.

`combined-kernels.json` additionally records unmasked convolution moments. For
the 255-tap candidate, maximum integer-origin distance sums are 16.099252 for
the BS.1770 phases and 16.388704 for the eight-phase radius-256 Kaiser kernel.
These do not apply automatically to mask-dependent output kernels.

Reproduce the design and measurements:

```sh
uv run --with numpy==2.5.3 --with scipy==1.18.0 python /tmp/deadpan-conditioner-20260921/design.py
uv run --with numpy==2.5.3 python /tmp/deadpan-conditioner-20260921/combined_kernels.py
```

`environment.json` records Python 3.12.13, NumPy 2.5.3, SciPy 1.18.0, macOS
26.5.2 arm64, the build configuration, and script hash. Design uses SciPy's
[firwin](https://docs.scipy.org/doc/scipy-1.18.0/reference/generated/scipy.signal.firwin.html)
under its [BSD license](https://github.com/scipy/scipy/blob/v1.18.0/LICENSE.txt).
No GPL implementation was copied. No repository files were edited.

No gain envelope, limiter, output masking, playback, float32 output execution,
listening, or ideal-sinc reconstruction guarantee is qualified here. Causal use
retains the integer delay; centered use requires the corresponding context.
