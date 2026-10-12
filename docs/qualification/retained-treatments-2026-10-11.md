# Retained fades and treatments, 2026-10-11

Retained owner input now carries creative output edges and authored owner
clocks on its consumer sample grid. This advances the EQ integration boundary:
history before a copied clip's current delivery must use its actual reference
phase, edge progress and gain/mute coordinates.

`AudioOwnerRetainedInput::fades` reuses the existing bound-fade evaluator while
retaining the original sample-map anchor outside current delivery. `owners`
uses the same exact map, resolves current recipes on the retained grid, reports
exhausted support explicitly and excludes exterior owners. It keeps
RootRoundEven and PointCeil phases distinct. Queries retain span/work limits.

`StageAudio::read_retained_owner_input` evaluates the existing raw time/pitch
operand and transported creative edges. `read_retained_owner_treatments` also
uses the shared gain/saturation/balance evaluator, including mute ranges and
explicit treatment order. These are Original-operand reads, not complete sound
or group mixes. The complete current treatment chain is not automatically the
right input to an EQ stage; the temporal graph must select a stage boundary.

## Verification

The full plan/audio suites pass 935 tests. The final seven owner-input audio
tests also include a direct retained silent-Hold comparison added during review.
Strict workspace Clippy with all targets and `ui-harness`, formatting and
whitespace checks pass.

Plan checks compare whole and single-sample owner queries across negative
labels, child seams and exhausted support. They verify exact reference clocks,
PointCeil phase, span limits and work exhaustion for both owner and fade queries.

Decoded PCM checks exercise the real stereo WAV fixture and native pipeline:

- Faded history before current delivery and root zero matches the uncopied
  reference exactly, including both physical endpoints and exhaustion.
- A source captured on a Preserve input's PointCeil grid retains its fractional
  source phase and creative fades after moving and resuming.
- A ramp, mute range, saturation-before-gain order and stereo balance match the
  original authored bus at the corresponding reference positions.
- A muted exterior group is excluded from the retained operand while remaining
  effective on the current whole bus.
- Retained Preserve output keeps explicit silent-Hold samples zero and reports
  their suppression, matching the current output path.
- Memory, foreign-plan, cancellation and invalid-range failures occur before
  provider access. No reader extends authored output or grants a tail permission.
- Nested reads inherit the parent's exhausted work, expired deadline and depth
  limit. Muted reads still admit their source and retain its dependency receipt.

The primary agent reviewed this change without subagents. Verification is local
decoder/DSP and plan evidence; there is no GUI, device or encoded-export claim.
The command receipts, logs, source/fixture hashes and toolchain identity are in
[`2026-10-11-retained-treatments`](../../tools/audio-qualification/evidence/2026-10-11-retained-treatments/).

## Remaining integration

Implement explicit treatment-stage inputs, then the ordered EQ graph and its
aggregate work/memory/cache rules. Preserve contribution identity through group
sums and apply silence/incoming-tail permission per voice. Add authored EQ
storage, captured native/semantic controls and real preview/export qualification.
DP-09/DP-16 and the unchanged full-product goal remain open.
