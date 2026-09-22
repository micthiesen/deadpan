# Independent sinc audit of the Kaiser-256 limiter prototype

Date: 2026-09-21. Scratch research only. No repository, limiter, or specification
changes were made.

The existing candidate fails the requested -1 dBTP ceiling under complete
finite-sequence, zero-extended sinc reconstruction. Five of the 35 retained
float32 stereo output fixtures provide direct counterexamples. These are lower
bounds on the continuous peak, independently evaluated at the listed coordinates.

| Fixture | Candidate's 32-phase Kaiser-256 result (dBTP) | Full-sinc witness (dBTP) | Coordinate, channel 0 |
| --- | ---: | ---: | ---: |
| alternating | -1.231361 | +5.352418 | 8191.460636670931 |
| tone-23990-16 | -1.316145 | +3.792635 | 6084.573545641963 |
| tone-23990-4 | -1.250010 | +3.596454 | 8191.453839107329 |
| tone-23990-1 | -1.250010 | +1.308375 | 8191.457426854697 |
| tone-23990-0.86 | -1.308570 | +0.193216 | 8191.458505766785 |

The amplitude-16 tone failure is inside the stored signal, so these failures
cannot be attributed solely to reconstruction after its final sample. The other
listed witnesses are just after the last integer sample, at coordinate 8191.
Zero extension at integer positions does not eliminate between-sample tails.

The first two witness amplitudes are 1.8519143723753556 and 1.5475039710975338.
The allowed amplitude is 0.8912509381337455. An independent 80-decimal-digit
evaluation of every finite sinc term confirmed all five violations. The largest
difference between those values and the float64 compensated sums was
2.220446049250313e-16. This is far smaller than any observed ceiling violation.

## Inputs and provenance

`inputs/` retains the original `prototype.py`, `prototype-kaiser256.json`, and
all 35 WAV outputs read from `/tmp/deadpan-master-20260921` before analysis.
The report identifies radius 256, 8 constraining phases, attack 2048, release
4800, Kaiser beta 10, and the internal target -1.25 dBTP. Its reported comparison
uses 32 phases of the same finite kernel. The WAV generator writes little-endian
float32, format tag 3, two channels at 48 kHz, with 8192 frames per file.

Every WAV header and reported sample peak was checked before analysis. Exact
WAV SHA-256 values are recorded in `results.json`, and copied prototype/report
hashes are in `environment.json`. This audit measures the retained output bytes;
it does not claim a separate bit-for-bit reproduction of the limiter itself.

## Independent reconstruction

For each channel with N stored samples, the oracle evaluates

`f(t) = sum(n = 0 .. N-1, x[n] sinc(t-n))`, where `sinc(u) = sin(pi*u)/(pi*u)`.

Every stored sample participates at every queried coordinate. There is no
window, renormalization, finite radius, periodic signal extension, or reuse of
the candidate's kernel. The samples outside the retained sequence are exactly
zero. At integer coordinates the sample is used directly. At fractional
coordinates, the identity

`sinc(k + p - n) = (-1)^(k-n) sin(pi*p) / (pi*(k+p-n))`

avoids inaccurate trigonometric evaluation at large arguments.

`oracle.py` uses a 128-phase grid. Each polyphase convolution retains every lag
needed for the entire scan, with enough FFT zero padding for linear convolution.
It refines the 16 largest sampled local maxima per channel using direct complete
finite sums. Reported witness sums use `math.fsum`; `check_oracle.py` separately
reevaluates the witnesses with mpmath at 80 decimal digits.

Across all 70 channel scans, the maximum observed discrepancy between the FFT
and compensated direct sums was 6.661338147750939e-16. This checks each grid peak
and 64 deterministic random positions per channel. Analytic impulse, two-pulse,
cancellation and integer-interpolation checks also pass, along with a separate
17-sample arbitrary-signal comparison using NumPy's direct `sinc` definition.

## Outside tails and between-grid bounds

The scan extends from `-D` through at least `N-1+D`, with D chosen independently
for each stereo fixture. Outside that interval,

`abs(f(t)) <= sum(abs(x[n])) / (pi*D)`.

D is rounded up to a power of two large enough that this bound is below the
ceiling. For the alternating fixture D is 4096 and the tail bound is 0.491008.
For tone-23990-16 D is 2048 and the bound is 0.872593. No infinite tail has been
silently discarded.

The Fourier integral for sinc gives `abs(sinc''(u)) <= pi^2/3`. For `abs(u)>=q>=1`,
the differentiated closed form also gives
`abs(sinc''(u)) <= pi/q + 2/q^2 + 2/(pi*q^3)`.
At most two sample positions occupy each distance bucket `[q,q+1)`. The script
uses these decreasing bounds, the sample magnitude, and the finite sample count
to obtain a conservative bound M on `abs(f''(t))` at every coordinate. Linear
interpolation between grid points can then underestimate `abs(f)` by at most
`M/(8*128^2)`. For the first two witnesses those allowances are approximately
0.0004622 and 0.0004206 in amplitude.

This supplies an explicit continuous scan bound, not merely a claim that a
dense grid probably found every peak. It also bounds the possible difference
between a refined witness and an unexamined maximum. For the other 30 retained
fixtures, the combined grid, between-grid and outside-tail upper bounds remain
below the ceiling with the declared conservative numerical allowance.

The bound calculations and FFT still use ordinary float64, not directed interval
arithmetic. The numerical allowance is intentionally conservative and empirically
checked, but it is not a formally certified rounding bound. The counterexamples
do not depend on proving a global maximum: each confirmed witness already
exceeds the ceiling by a large margin.

## Why the finite detector misses these cases

`support.json` records partial unwindowed sums at each witness. At the amplitude-16
tone witness, terms within distance 256 contribute only 0.6497015026, while
more distant terms add 0.8978024685. At the alternating witness, the omitted
distance-256 contribution has magnitude 0.8341108485. Kaiser weighting changes
the nearer terms too. A denser phase grid using the same short kernel cannot
recover contributions that its support/window excludes.

## Reproduce

The retained environment is macOS 26.5.2 arm64, Python 3.12.13, NumPy 2.5.3,
and mpmath 1.3.0. Full NumPy build configuration and source hashes are retained.
The full 35-fixture scan took approximately 14 seconds on this host; this is a
research script timing, not a production limiter or playback measurement.

```sh
uv run --with numpy==2.5.3 python /tmp/deadpan-limiter-ideal-20260921/oracle.py --reuse-inputs
uv run --with numpy==2.5.3 --with mpmath==1.3.0 python /tmp/deadpan-limiter-ideal-20260921/check_oracle.py
uv run --with numpy==2.5.3 python /tmp/deadpan-limiter-ideal-20260921/support.py
```

`--reuse-inputs` prevents the full scan from recopying potentially changed parent
scratch files. The scripts overwrite their own result JSON files. Omitting this
flag intentionally snapshots the parent's current outputs again.

This is an ideal bandlimited reconstruction audit of these finite fixtures. It
does not qualify an actual DAC, encoded output, listening quality, arbitrary
inputs, another cutoff interpretation, or a complete standards-conformance
suite. It supplies counterexamples for the next limiter design and does not
promote this candidate or redefine the product ceiling.
