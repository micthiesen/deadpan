# Moving a sampled root signal onto a preparation grid

`RootSignalTransfer` resamples admitted, already time-mapped root PCM onto an
explicit `SignalSample` grid. `StageAudio::read_transferred` supplies that PCM
from one immutable plan and its qualified source provider. This implements the
bounded conversion needed when retained audio enters a new Preserve input.
It does not persist continuity bindings, author a Hold insertion, or connect
that operation to application playback. Core 14/database 20 are unchanged.

## Keep the old discrete signal

Root output allocates boundaries with round-even; preparation uses point-ceil.
Those clocks can disagree by a sample. At 30000/1001 fps, an explicit silent
Hold inside Preserve at output frames `[2,3)` starts at root sample 3203, while
intrinsic prepared-output suppression starts at 3204. A later fractional lookup
must sample the root signal including its silence at 3203.

Masking only the destination does not preserve that signal. A lookup just before
the silence still uses neighboring sinc taps. The old silent samples must be
zero before filtering, even if the destination point itself is audible. The
adapter then reapplies explicit silence at the destination point, so filtering
does not refill a silent interval. It uses the existing qualified `Resampler`,
including its exact integer-unity path; it adds no filter or normalization.

The transfer maps each destination index `n` to an exact old root sample:

```text
q(n) = root_at_anchor + (n - signal_anchor) * root_samples_per_signal_sample
```

Each old half-open suppression interval `[a,b)` becomes the destination interval
whose exact `q(n)` lies inside it. Its endpoints use ceil after inversion of this
map. Old root allocation has already happened; the adapter does not recompute it
from project frames using a different rounding rule. Requested blocks cannot
change the anchor, rate, full input support, or interval endpoints.

## Input and output contracts

`RootSignalBlock` contains exactly the requested root samples and explicit
suppression ranges clipped to that block. The ranges may overlap or be unordered.
They represent silent Holds and exhausted retained envelopes. Numeric zero,
missing source audio and a placement gap do not imply forced suppression: a
Preserve stage can retain nonzero decay there.

The adapter validates all PCM, including samples declared silent, before masking
input taps. It rejects missing/extra samples, wrong starts, empty/out-of-block
intervals, nonfinite values and input magnitudes above the shared limit of 16.
Samples outside the admitted full support supply zero filter context, and output
points outside that support are zero with explicit suppression metadata. It
never clamps to or repeats an endpoint sample.

Output uses `SignalSample` and merged suppression intervals. A recipe alone is
coordinate metadata, not media admission. The caller owns the root signal's
identity. `StageAudio` binds reads to its project/revision, verifies that support
is the complete root, and includes that identity and the exact transfer recipe
in `TransferredRootBlock`. Two different destination grids cannot be identified
by their sample numbers alone.

## Preparation order and bounds

The integrated reader follows this order:

1. Read root PCM from complete Source, RoomTone and Preserve contexts. Preserve
   retains its canonical input/output history and verified cache dependencies.
2. Apply root explicit silence and retained-envelope endpoint exhaustion to the
   old discrete samples. Do not bake creative fade gains into this input.
3. Resample the bounded masked input halo and reapply its exact destination
   audibility. Return typed samples and suppression metadata.
4. The consuming stage must still apply its current input/output silence policy
   around its own DSP. Creative edge fades remain after all time mapping.

One call returns 1..256 destination samples. The qualified rate range is
1/64..64 old samples per destination sample. At most 32,706 input samples are
assembled, through at most 128 callbacks of 1..256 samples. A callback may supply
at most 256 suppression intervals; the adapter reduces them into a fixed-size
destination mask rather than retaining an accumulating interval list.

`StageAudio` shares one work counter, source-provenance observation set and
deadline across the entire halo. Splitting that halo into decoder-sized reads
must not reset preparation limits or allow a source/layout to change halfway
through. Cancellation propagates through source reads, DSP and filtering. A
failure returns no partial output; completed preparation cache entries may remain
for a bounded retry. Deadlines are cooperative and checked before returning,
not a preemptive guarantee on native work. Halo storage is separate from the
existing prepared-PCM residency budget.

## Evidence and remaining integration

Tests compare the streaming adapter with a completely materialized, premasked
carrier passed directly through the existing resampler. They exercise fractional
origins, both rate limits, suffix-first and irregular reads, input-tap masking,
support exhaustion, exact output suppression and malformed provider responses.
Canonical DSP tests transfer resumed NTSC audio into a nonzero point grid and
feed it through another Preserve operation. Native decoder integration covers
qualified source PCM, root silence, room-tone/Preserve context reuse, shared
work exhaustion and a source-layout change between halo reads.

The [qualification record](qualification/audio-transfer-2026-09-23.md) records
actual checks and limitations. The [splice design](STRUCTURAL_SPLICE_DESIGN.md)
still requires authored live-to-frozen domain bindings, compact Repeat lifecycle,
composed resume anchors, edited-policy replacement, new seam envelopes, strict
history migration and the atomic inserted-time command. This adapter provides
the PCM conversion boundary; it does not establish those semantics by itself.
