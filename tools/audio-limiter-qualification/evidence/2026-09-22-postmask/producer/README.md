# Post-mask limiter scratch experiment, 2026-09-21

This directory retains one bounded mastering experiment. It is not production
code, limiter qualification, or a universal ideal-sinc ceiling claim.

## Signal and control construction

Every case has exactly 8,192 stereo frames at 48 kHz. The harness zero-extends
the full project and applies, in order:

1. the retained symmetric 255-tap Kaiser beta-16 conditioner with a 21 kHz
   half-amplitude cutoff;
2. either an explicit boundary envelope or hard-boundary mode;
3. the exact authored silence mask;
4. one shared stereo gain trajectory; and
5. float32 conversion.

The gain proof includes direct output samples, the retained BS.1770-5 4x rows,
and a separate 8-phase Kaiser beta-10 radius-256 finite reconstruction. Its
control anchors cover `[-256, N+254]`, which includes every nonzero zero-extended
detector output. The common local caps use the Lipschitz bound
`|X| g + L W`, with `L=2^-12`; release uses `R=2^-13`. Gain caps are floored to
`2^-32`, so the slope steps are exact integer increments. The internal target is
-1.25 dBTP minus a 1e-6 linear guard.

The input contract for this experiment is finite float32 stereo with absolute
magnitude at most 16. The conditioner L1 norm is 2.4938660232001224. The retained
global finite cap therefore gives `Wmax=2543.544089301512` and positive minimum
margin 0.24498087968293836. These numbers cover arbitrary masks and envelopes
that cannot increase the conditioned sample magnitude. They do not cover an
ideal brick-wall reconstruction.

## Results

The original bounded corpus has 13 cases and no harness failures. Every output
passed both retained finite detectors at -1 dBTP. The nearest result was the
hard/shortened 0.8 alternating-mask case at -1.250010 dBTP in the Kaiser scan.
The compiled Deadpan BS.1770 meter reported -1.938200 dBTP for that output.

The hot controls also passed:

| Case | Deadpan BS.1770 4x | Kaiser 8x/R256 | Minimum gain |
|---|---:|---:|---:|
| alternating 16 | -1.279846 dBTP | -1.255585 dBTP | 0.114534363 |
| 23,990 Hz, amplitude 16 | -1.277501 dBTP | -1.251502 dBTP | 0.114534166 |
| stereo-opposed two-sample peaks | -1.444845 dBTP | -1.279858 dBTP | 0.060961320 |

The 0.1 quiet tone and DC controls retained unity gain. The ordinary 512-frame
silent gap is exactly zero in the float32 output. The permitted-tail case retains
nonzero output in all 512 examined tail frames and also retains unity gain.
Output duration remains exactly 8,192 frames in every case.

The first `default96` experiment uses a fixed 96-sample ramp on every active
run. It does not erase one-sample fragments, but multiplies them by 1/96, about
-39.65 dB. That behavior suppresses the rapid-mask witnesses and is not a
faithful implementation of the specification's shortened-fade language for tiny
fragments. These outputs remain retained as explicit negative design evidence.

`shortened_edge.py` adds a separate candidate without changing the fixed96
bytes. For a run of length `N`, it uses `f=min(96,N/2)` and sample-centered
weights `min(1,(i+.5)/f,(N-i-.5)/f)`. A one-sample run has weight 1 and a
two-sample run has weights 0.5, 0.5. The three one-sample-island outputs are
therefore byte-identical to hard mode. The 0.8 and 16 cases require limiting and
pass the retained finite detectors. This endpoint rule is a candidate to review,
not an adopted product policy.

The integer future-cone and release recurrences were also evaluated in irregular
chunks with carried state and matched their one-shot recurrences exactly. This
checks only scalar gain-state partitioning. It does not qualify conditioner
streaming, true project crops, random seek, checkpoint restoration, or replay
from globally correct mask context. Prepared-PCM slicing would not supply that
evidence.

## Artifacts and reproduction

`pre-run-manifest.json` binds the first harness, conditioner, combined kernel
table, and compiled Deadpan meter before case output. The shortened-edge run has
its own pre-run manifest. `outputs/` retains exact float32 stereo WAV and raw
f32le bytes. `meters/` retains compiled meter JSON. `results.json` and
`shortened-edge-results.json` retain all per-case metrics, proof-row margins,
artifact hashes, and failures. `SHA256SUMS` covers every retained file except
itself.

From the Deadpan repository used for this run:

```sh
uv run --with numpy==2.5.3 python /tmp/deadpan-postmask-limiter-20260921/prototype.py
uv run --with numpy==2.5.3 python /tmp/deadpan-postmask-limiter-20260921/shortened_edge.py
```

The harnesses overwrite same-named output artifacts, so reproduce in a copied
directory when retaining these exact bytes. The first run's executable SHA-256
is `97216bd091c1f54b769ccfa0c709590fd03ae7e095b94e7fd12a2dc3ba78eaae`.

## Qualification boundary

The post-output Kaiser scan is a separate evaluation path inside the same Python
harness and uses the same retained kernel family as the proof cap. The compiled
Deadpan BS.1770 meter is a separate executable. Neither is the pending full-sinc
audit. Known earlier rapid-mask full-sinc counterexamples remain valid evidence
and are not redefined away. No production choice should be promoted until these
new outputs receive the separate full-sinc audit and the complete streaming,
chunk, checkpoint, and bounded-seek construction is tested.

Earlier rejected evidence remains untouched in
`tools/audio-limiter-qualification/evidence/2026-09-21-masked/`. The scalar
replay experiment separately reported by the parent is retained at
`/tmp/deadpan-gain-replay-20260921/`; it validates recurrence forgetting only,
not PCM or context preparation.
