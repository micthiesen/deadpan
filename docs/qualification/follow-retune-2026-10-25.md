# Existing Follow parameter editing, 2026-10-25

Existing animated Follow framing now permits independent arrival-scale and
curve edits through `:creep` and Camera. DP-08.3 receives full local credit;
DP-08 and every delivery gate remain open. The fixed 162-criterion ledger
increases by 0.2604 percentage points, from 83.7426% to 84.0030%, still rounded
to about 84%. This is acceptance coverage, not remaining engineering time.

## Behavior and ownership

`framing_zoom::retune_follow` retains the exact target, fallback center,
normalization clock and transition window. `to=` is the absolute live arrival
scale; fallback scale changes proportionally from its own base. Omitting
`from=` preserves the bases and existing ramp start. An explicit start rebases
a whole transition, or changes only the ramp start of a ranged transition while
preserving its prefix. Both live and fallback bounds are validated.

The shared builder requires an admitted whole owner. A selected Visual range,
including an explicit full-owner range, refuses instead of relocating the
existing animation. Source, Hold, Sequence, Repeat and Retime use the same
transform. Explicit Play isolation retains its existing authority checks.
Named macros and dot retain semantic Keep intent and resolve the current
target, scale and clock on invocation. Raw stored Step curves remain valid;
the public creep command continues to require Linear or Smoothstep.

Camera adds Arrival scale (%) and Curve to its temporary preview. Uniform
Scale still changes the whole animation. Applying arrival/curve edits rebases
the scale handle at the exact retained owner position, preserving subsequent
proportional +/- changes and exact non-Q32 centers. Native text editing retains
partial decimal input, selection and IME state; invalid/composing fields cannot
Apply. Enter saves one revision and Escape restores the complete entry state.
Target-owned centers use a compact read-only summary so Apply and Cancel fit
at 960×640. Curve retains focus after pointer activation and vertical arrows,
while Tab, Enter and Space keep their native behavior.

## Executed checks and preserved failures

Runs use Rust 1.97.1, locked dependencies and the qualified FFmpeg/ORT prefixes
recorded in the receipts on this Apple M5 Max, macOS 26.5.2. Debug checks use
opt-level 2 and a separate Cargo target directory; actual media uses release.
The [evidence collection](../../tools/media-qualification/evidence/2026-10-25-follow-retune)
retains commands, outputs, before/after source maps, failed runs and final
reports. No whole-application, release-performance or new bundle qualification
is claimed.

- The initial six helper tests pass. Their receipt records concurrent edits to
  other inputs, so this is not a frozen final source proof.
- The first integrated check stops at two strict lint findings, fixed before
  tests. The second passes lint and 1,742 of 1,746 selected tests. Four failures
  are retained: generated command-reference refresh, undrained initial egui
  texture output, missing test field refocus, and duplicate fixture revisions.
  Corrected reference/setup tests pass in the four-test continuation, again
  with strict normal and UI-harness lint. This is not relabeled as a new
  all-green 1,746-test run.
- Later focused native lanes pass 79 tests, then the final compact-layout lane
  passes 80 tests with strict checks. The new actual pointer/arrow/Space test
  includes an Apply button below Curve and checks retained focus.
- Early painted replays expose a disabled-control lookup mistake, then actual
  pointer focus loss and egui directional navigation moving focus to Apply.
  Those production defects are fixed. A subsequent label check needed one
  ordinary paint after deferred field adjustment. The fourth replay then
  exposes Apply/Cancel below the minimum-size inspector; compacting the locked
  centers and hint fixes the actual layout. All original reports remain failed.
- Final `replay-fifth` at 60 Hz and `replay-final120` at 120 Hz each pass
  **415 zoom checks plus the shortcut audit**. They check actual painted clips,
  accessible/viewport bounds, keyboard and pointer curve effects, invalid/IME
  Apply refusal, no-write preview/cancel, save/Undo and semantic replay for both
  whole and ranged animations at 1280×820 and 960×640. The final PNGs were
  inspected. Injected input/offscreen Metal do not establish physical input,
  VoiceOver speech, display latency or performance percentiles.
  Reports retain screenshot-allowance and layout-retry warnings: the 60 Hz
  run has 33 second-retry frames and 20 sequences with distinct retry causes.
  Named image checkpoints remain captured; ignored-retry and repeated-same-cause
  guards pass. No warning is relabeled as a failed behavior assertion.
- Both final replays execute the same snapshot, SHA-256
  `649e00cec618407ff269ee39219df95bee112c161fb7d21bb690352226feb27d`.
  The 120 Hz receipt pins that binary before and after and records unchanged
  source. Its first driver attempt misordered logger arguments before any
  child launch; its second correctly refuses performance mode for a debug
  binary. The third is the passing visual behavioral run.
- The final live Kestrel digest matches the pinned fixture:
  `368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
  The production audit checks all 62 reservations over 21,885,504 routing cases
  without conflict. No global shortcut assignment changed.

Independent general, native text/focus and core/scope reviews report no remaining
findings. A final native review covers the directional-focus cancellation and
compact summary after the replay-discovered fixes.

## Actual media and complete history

The two release `preview_export retuned_` tests pass with physical-Mac steps
required and no ignored tests. Their 1,702 scoped input hashes remain unchanged;
an independent audit rehashes all inputs without a missing or changed file.
Each case retunes an existing Linear Follow to Smoothstep while retaining the
target, fallback and existing range. The moving/Lost/corrected target data is
authored over the 120-picture counter Original; this is not detector or tracker
accuracy evidence.

| Check | Whole animation | Ranged animation |
| --- | --- | --- |
| Exported pictures against independent exact pose oracle | 120/120 pass | 120/120 pass |
| Canonical authored, limited PCM and limiter gain before/after | All 192,192 stereo sample frames bit-identical | All 192,192 stereo sample frames bit-identical |
| Encoded A/V verification | Pass, zero audio offset | Pass, zero audio offset |
| Live/fallback bases retained | 3/2 and 5/4 | 3/2 and 5/4 |
| New terminal live/fallback scales | 3 and 5/2 | 3 and 5/2 |
| Transition window and start factor | Whole owner | [1/6, 2/3], start factor 6/5 unchanged |
| Complete Undo and portable history | 8 revisions, 8 verified receipts | 8 revisions, 8 verified receipts |

The cases use OwnerOutput. Core/native/CLI tests separately cover nonzero
RetainedOutput clocks, including offset 15/2 and duration 180 in the public CLI
history workflow; no emitted-media result is claimed for that clock here.
The ranged verifier reports 218 observable and 20 unobservable neighboring-index
comparisons, with minimum neighbor margin zero. All requested pictures pass;
the fixture does not distinguish every image from every adjacent image.
AAC timing metadata declares media time 2,048; verification uses actual
presented-file timing without event alignment or relaxed AAC-block tolerance.
No new listening or independent DSP qualification is claimed.

Executed release images, recorded after the run, are CLI
`e70a3baad7f57861ca13a70a3054e5722da91ebdf4841ac484c02e99f66fd47c`,
media worker
`70114641de62f0c9c839734f3dddab1221b01888661a43ed5583e40ca9ed90fe`,
and test executable
`e57b36b42651d6463ce480a2c729d168203889bd30ebb2ec29de3de29135959a`.
The media logger's binary maps are empty; these are post-run hashes, not
before/after binary pinning. No concurrent release build ran.

## Remaining scope

The independently byte-checked collection contains 146 payloads and four
metadata files, about 16.2 MB. It excludes build caches, binaries and full source
archives. An omission ledger records four nonempty SQLite SHM files and empty
lock/WAL sidecars; no nonempty WAL was omitted. The collector verifies copied
and decompressed bytes against source hashes and records executable identities.

Arbitrary re-windowing, stepped path return, escalation transformation,
broader effects/mastering and native workflows retain their existing tracker
obligations. This change edits parameters of an admitted existing animation;
it does not flatten paths, authorize sampled-target macros or ranges crossing
occurrence boundaries, close DP-08, or close a delivery gate.
