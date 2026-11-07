# General sound-bearing Split delivery, 2026-11-07

## Result

Split preserves complete authored, edge-faded and limited PCM for every owner
kind (Source, Hold, Sequence, Repeat, Retime) under Sequence, Repeat and Retime
parents, including refinement, one selected play and nested selected plays. The
comparisons are exact: real WAV fixtures, whole recordings and a second read
partition. Two cases refuse instead of changing output; see Refusals.

## Why earlier attempts failed

Earlier work treated `Limit("plan work per read")` as the problem and spent 85
commits on accounting around it. Lifting that limit showed it was hiding wrong
audio. Measured with the limit lifted, a Split under a Repeat differed by up to
1.4 across about 10,000 samples per case. The first mismatch at each failure was
one `f32` rounding step, which looked harmless until every sample was counted.

`compile_forest` built its Repeat map from the journal's steps, which stop at the
sound's own scope, so the proof failed with `sound clock Repeat map does not
describe its complete ancestry` for any Split below a Repeat. `Err(_) => continue`
swallowed that, and the pieces were processed separately. Root EQ and wet sends
then lost look-ahead before the cut and tails after it. Isolating each treatment
at each level showed only these two time-spreading stages under a Repeat failed.

## Changes

- `sound_clock_context_repeats` derives complete ancestry. A Repeat pairs through
  a recorded step, the same surviving identity or a journal-recorded isolation.
- A delivery proof stops at the processor both contexts share and requires it to
  be the same node. Other proofs keep the strict enclosing-processor refusal.
- Below a processor a Partition crop cannot divide a complete copy, so each
  historical occurrence sounds once (`delivered_occurrence`). A per-piece rule
  loses plays when a Repeat is split between pieces. Outer plays stay distinct.
- Pieces edited after a Split no longer share a forest, so each keeps its own
  sound. These last two came from independent review, each with a regression test.

## Performance and limits

Preparation is about 2.1 times faster with bit-identical output: per-phase
resampler weights, exact integer position stepping, slice taps and one clock
read per 256 checks. Audio tests fell from about 3,228 to 1,515 test-seconds.

Measured peaks across the matrix and shipped limits:

| Limit | Peak | Previous | Shipped |
| --- | ---: | ---: | ---: |
| Plan work per read | 223,786,438 | 16,777,216 | 1,073,741,824 |
| Source provenance checks | 387,812 | 1,048,576 | 16,777,216 |
| Resident frames | 20,700,028 | 16,777,216 | 33,554,432 |
| Prepared stages per read | about 1,200 | 64 | 4,096 |
| Prepared frames per read | 57,644,916 | 134,217,728 | unchanged |

Resident frames are the memory bound, 256 MiB of stereo f32. The others bound work,
and the shared deadline bounds time. Unit tests keep the original budgets through `cfg(test)`, because frozen
evidence pins exact accounting at them; integration tests use shipped values.

## Refusals

- A beat with both its own treatment and an attached sound cannot be Split inside
  a speed change. Its treatment needs the sound in its nonlinear sum, while the
  processor above needs one continuous voice, and no read path does both yet. This
  remains open implementation work, not an accepted limitation.
- A pause that divides a speed change with attached sound refuses. Measured
  output changed on both sides of the pause, by up to 0.26 before it even without
  treatments, so the previously accepted edit was wrong. This also remains open.
- Split of the project root with a root-owned sound keeps its refusal. Editing
  never cuts the root itself.

## Not established

Native Split, receipts and macros over these documents, emitted media from a
Split project and listening.

Reads are correct but too slow. A limited 4,096-sample read of the nested fixture
can exceed the 60 second limit when competing for cores, and the slowest matrix
test takes about 17 minutes alone on the 18-core development Mac. The matrix
reserves six test threads so reads keep that unchanged limit. This is open
performance work, not an accepted cost.
