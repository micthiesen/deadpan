# Joint conditioner and finite-meter proof notes

For the prototype convention

`y[n] = m[n] * sum_(j=-127..127) c[j] x[n+j] g[n+j]`,

write a reconstruction sample anchored at integer `t` as

`z[t,p] = sum_q r[p,q] y[t+q] = sum_k a[t,p,k] x[t+k] g[t+k]`,

where `a[t,p,k] = sum_q r[p,q] m[t+q] c[k-q]`. Project input and
output are zero outside `[0,N)`. The mask must satisfy `|m[n]| <= 1`.

## Exact nominal supports and complete anchor domains

* Raw output: `q={0}`, combined input offset `k=[-127,127]`, anchors
  `t=0..N-1`. Skip when `m[t]=0`.
* BS.1770 with the parent's stated row offset `q=k_row-6=[-6,5]`:
  combined input offset `[-133,132]`, complete anchors `t=-5..N+5`.
  This differs by one from `combined-kernels.json`, which deliberately used
  `q=[-5,6]`. Do not reuse that file's per-phase offset labels unchanged.
* Eight-phase radius-256 Kaiser with `q=[-255,256]`: combined input offset
  `[-382,383]`, complete anchors `t=-256..N+254`. Phase zero is an exact
  impulse and only needs `t=0..N-1`; the seven fractional phases use the full
  domain.

An anchor outside those inclusive domains is identically zero. Within them,
skip a phase only when every nonzero `r[p,q]` lands on an outside or zero-mask
output. Conditioner overlap with nonzero input cannot revive an output removed
by the post-conditioner mask.

Centered execution emits exactly `N` samples. It does not emit conditioner
tails. The output is itself zero-extended for the reconstruction meters, so the
negative and EOS anchors above are required. A preview chunk/crop is not a new
zero-extended file boundary: derive it from the full-project coordinates and
discard context.

## Arbitrary-mask moment bound and slope

For any binary mask, or any real mask with magnitude at most one,

`sum_k |a[k]| |k| <= sum_q |r[q]| sum_j |c[j]| |q+j|`.

This is a triangle-inequality bound and does not rely on cancellation in the
unmasked convolution. With the stored 255-tap beta-16 coefficients:

* raw output maximum distance bound: `15.791573144141793`
* BS.1770 maximum with row offsets `[-6,5]`: `34.85459840933363`
* Kaiser maximum: `197.20315173080198`, at phase 1/2
* with `|x| <= 16`, global `W_max <= 3155.2504276928316`

At `C = 10^(-1/20) = 0.8912509381337456`:

* `L=1/4096` gives `C-L*W_max = 0.12092612668530034 > 0`
* `L=1/8192` gives margin `0.506088532409523`

Use `L=1/4096` for the prototype. It proves a strict positive numerator even
for arbitrary cuts, needs a 4096-sample (85.333 ms) cone, and still permits a
100 ms release `R=1/4800 <= L`. The total live source lookahead for one gain
sample is `4096+383=4479` samples. Recompute the margin if the internal target,
input bound, stored coefficients, mask range, or reconstruction kernels change.

Compute `X` and `W` with outward numerical allowances: an upper bound for
`|X|`, an upper bound for `W`, and a lower proof ceiling. If `X_hi=0`, use
`b=1`; the global positive-margin proof then gives `L*W_hi<C`. Set all-zero
constraints to `b=1`. Never repair a nonpositive numerator by clipping it.

## Extended gain-anchor domain

Define virtual gain anchors across `[-256,N+254]`, setting `b[t]=1` when no
constraint remains. This is needed because endpoint reconstruction constraints
are anchored outside the actual source indices. Compute

`F[t] = min_(u>=t) (b[u] + L*(u-t))`

with values capped at one, then

`g[t] = min(F[t], g[t-1]+R)`.

The future search can stop after `ceil(1/L)=4096`: a farther nonnegative cap
contributes at least one. Initializing the first virtual anchor directly from
`F` is sufficient; no constraint refers to an earlier gain. Emit only
`g[0..N)` through the centered conditioner. This produces both slope bounds,
`-L <= g[t+1]-g[t] <= R`, and `g[t]<=b[t]`.

For an exact output crop `[A,B)`, the conditioner needs gain samples
`[A-127,B+127)`. In real arithmetic with `R=1/4800`, release state older than
4800 samples cannot affect a gain bounded by one. This gives the mathematical
source interval `A-5309` through the inclusive index `B+4605`, intersected with
the project; equivalently the upper half-open bound is `B+4606`. Repeated f64
addition of a non-dyadic `1/4800` is not automatically the same operation as
the finite min-plus formula. Do not claim bit-identical suffix seeks from this
real-arithmetic bound alone. Use a matching gain checkpoint and identical
recurrence replay, define a canonical origin-based ramp, choose and prove a
dyadic release, or retain prepared PCM. Always use the full project mask and
coordinates; a fresh crop origin is incorrect.

## Minimum prototype checks

1. Compare direct conditioner, mask, and meter execution against the combined
   coefficient equation for random stereo input and masks, including one-frame
   projects and first/last impulses.
2. Assert the last nonzero BS anchors are exactly `-5` and `N+5`, and Kaiser
   anchors exactly `-256` and `N+254`; one anchor beyond each side must be zero.
3. Exhaust all 4096 BS mask patterns. For Kaiser, test all prefix, suffix, and
   single-interval masks plus random masks against the analytic arbitrary-mask
   bound.
4. For every generated gain sequence, assert `g<=b`, downward delta at most
   `1/4096`, upward delta at most `1/4800`, and direct reconstructed peaks at or
   below the proof ceiling plus the stated arithmetic allowance.
5. Require one-shot, varied chunks, EOS flush, and finite replay crops to be
   bit-identical in emitted `N` samples and gain values.

`masked-moments.json` contains the per-phase calculations. Reproduce with
`uv run --with numpy==2.5.3 python masked_moments.py`.
