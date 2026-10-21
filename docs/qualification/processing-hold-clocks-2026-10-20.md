# Hold sound clocks inside processing inputs, 2026-10-20

Ordinary `SetHoldDuration` retains moved physical audio clocks inside fixed
Preserve and Shift input windows. The processor's recipe, selected window,
output duration and outer placement remain fixed. Current input is processed
again, with its retained sample phase. This advances DP-07 without completing
the requirement.

Base `eddb16c9`, Apple M5 Max, 128 GB, macOS 26.5.2 (25F84), Rust 1.97.1 and
the qualified FFmpeg 8.0.3 prefix. No schema, primitive, keybinding or DSP change.

## Reproduced input-phase reset

The 30000/1001 fps witness uses a three-frame silent lead, two-frame silent
Hold and four-frame natural-rate Source with 44.1 kHz sound starting at source
sample 3100. Preserve selects input `[0,8)` into four output frames; Shift
selects the same window into eight output frames with a three-semitone shift.
Growing the Hold to three frames leaves the root duration fixed but moves the
Source's input entry from sample 8008 to 9610.

Before the fix, all 256 observed samples agree with the newly computed phase
`3100 + 147/400`, rather than the retained phase 3100. The first actual sample
is -0.10523279, while the independent retained-phase recipe gives -0.09149733.
The old baseline's literal first 256 samples pass before resizing. A nonzero
selected start also fails: input `[1,8)` should retain `3100 + 441/800`, but
resizing replaces it with phase 3100. The retained failed test binary has
SHA-256 `16064b4550d2258d1cec09c2aab164a48bf98dddfe52ccd818fa44faa63feafc`.

## Clock ownership

The duration-specific collector stops ripple at the first fixed Retime and
continues checking all ancestors for unused Repeat defaults. A narrow capture
selector records each affected recipe's current placement on its own clock.
Root placements use the old root window; processing-input placements retain
their selected PointCeil support without applying a root-coordinate window.
Moved opaque processor outputs belong to the enclosing input clock, while
their intrinsic descendants retain their own input clocks.

Existing processor output bindings and fractional resumes remain unchanged.
The live recipe and revised operand rebuild canonical DSP. Newly captured
unbound outputs gain a lattice without a movement step. Ordinary root-ripple
and dedicated derived-clock dispatch remain separate. Silent-owner filtering,
generated fallback, Tail bounds and dormant-default compactness stay intact.

## Independent PCM and durable history

All 18 PCM regressions pass. They cover unbound and prebound Preserve
and Shift inputs, selected starts zero and one, full current grow/shrink
allocations, finite native Source crops, historical cropped resumes, nested
affine input and nested opaque processing outputs. Every sample and suppression
flag is checked exactly, including alternate chunk schedules.

For the nested affine operand, old origin `15/4` gives PointCeil sample 6006;
new origin `9/2` gives 7208, rather than root RoundEven 7207. The literal Source
step is `49/40`. This distinguishes the clock domains instead of accidentally
passing at an integral boundary.

Two output controls independently construct complete old and current inputs,
run real `CanonicalStretch`, apply the literal unchanged one-third-sample
output resume and compare both raw transferred and consumed output. Preserve
prepares 6407 PointCeil points while the root allocation has 6406 samples;
Shift prepares and allocates 12813. Current silence policies are applied before
transfer and at consumption. Changed processed PCM is never compared against
old processed PCM as if its DSP context were unchanged. These controls reuse
the unchanged transfer primitive with literal coordinates to test the new
wiring; they do not independently qualify that kernel.

The Repeat matrix independently checks every sample and suppression flag for
default and owned Hold grow/shrink, both unbound and partially bound. Its fixed
input `[0,18)` has 28829 points. The owned Source begins at PointCeil 14415,
where root RoundEven would give 14414; the two default plays retain different
literal phases. RoomTone gaps keep their old fractional loop phases and exact
321-source-sample extent. A gap moved to frame 10 has one extra live slot
beyond its old physical allocation, which remains suppressed zero. Current
finite Source crops also cover newly exposed suffix audio after shrink.

Seven new core tests and the six existing fixed-affine controls pass. They
cover exact clock ownership, current history, unchanged processor outputs,
cropped and nested affine windows, default/owned Repeat interiors, million-play
compactness and 258 dormant-default resizes without binding growth. Together
with the 18 PCM tests, the final focused run passes all 31 checks in 1.955 seconds.

The durable-store regression passes four Preserve/Shift unbound/prebound
cases: preview equals commit, output duration stays fixed, reopen and Undo/Redo
retain exact bindings, invalid shrink refuses, and a proven late Compound
failure leaves document, registers and SQL revision/history/step counts intact.
This is metadata evidence, separate from decoded-media checks.

## Test corrections and sound boundary

The first core build fails a test-only integer conversion; the assertion now
supplies the required exact-ratio numerator type. The first executable core
round passes 11 tests and fails a journal test that attached sound to an unmoved
outer tail, then expected a new journal. That premise was wrong. The corrected
coexistence control moves an unchanged attached processor through the existing
root-ripple path and checks its journal against the complete shared timing
layout. It does not claim new fixed-window attachment support.

The first strict lint run rejects two constant-only arithmetic assertions in
the nested-affine PCM test. Those tautologies are removed; literal boundary
derivations and complete actual PCM/mask comparisons remain unchanged. This
test-only correction changes no production or release-fixture source.

Fresh and historically bound edits whose attached sound's processing subtree
changes retain the existing `sound clock processing subtree changed` refusal.
Those cases must remain atomic. Independent review found no production or PCM
oracle issue and identified the incorrect journal fixture before its run.

## Real media

All four release exports pass, checking every picture and its provenance:
100 pictures in total. Preserve selects input `[1,9)` into four output frames
and keeps the project at 23 frames; unity-rate Shift selects `[0,8)` into eight
and keeps it at 27. Both grow the Hold from three frames to five and shrink it
to one. Separate nonzero energy checks inspect only the current processed
Source interval, so the outside suffix cannot conceal a muted processor.
These checks use the qualified Original's actual audio; the deterministic
provider tests separately establish 44.1 kHz phase arithmetic.

Four complete emitted audio windows pass at measured offset zero, without
uncovered samples, low-SNR, level or silence flags. Ten signal blocks are
compared. The complete ranges are `[0,36837)` for Preserve and `[0,43243)` for
Shift. The approved timing metadata declares 2048 samples of encoder priming;
verification does not align events, drop packets or relax the offset bound.

| Export | Pictures | Minimum block SNR | Maximum level difference |
| --- | ---: | ---: | ---: |
| Preserve grow | 23 | 32.589 dB | 0.0346 dB |
| Preserve shrink | 23 | 32.589 dB | 0.0346 dB |
| Shift grow | 27 | 43.659 dB | 0.0128 dB |
| Shift shrink | 27 | 34.836 dB | 0.0518 dB |

Repeated freeze and Background images leave 10, 8, 18 and 14 index-neighbor
comparisons unobservable respectively; their pixels and expected provenance
are still checked. The release test completes in 11.84 seconds after its build.

## Broad checks and retained record

Formatting and strict Clippy pass. All 2,661 selected core, audio, store
and CLI macro/scope/completion tests pass in 112.533 seconds, with 724
skipped by the retained filter. No warnings or leak diagnostics are reported.
The separate focused and history runs, failed old-code witnesses, test-only
compile and journal failures, independent review, source and executable
identities, four MP4s and complete verification reports are retained in the
[evidence](../../tools/media-qualification/evidence/2026-10-20-processing-hold-clocks).
`inventory.json` hashes every retained file.

## Limits

General speed-stage updates, dedicated derived-clock changes, attached-sound
processing lifecycle and broader Section 8 work remain open. This work does
not establish physical input, acoustic judgment or a new latency bound.
