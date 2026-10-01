# Native Move replay coverage

## Current state

The splice replay now invokes a bounded Move suite after the existing Copy and
Replace witnesses. Source changes are limited to
`crates/deadpan-app/src/preview/harness/splice.rs` and its `splice/` subtree.
Existing Copy/Replace control labels and Apply remain unchanged.

The root agent owns Cargo, execution of the real Metal replay and screenshots.
This worker has run no Cargo, GUI, device output or native replay. Scoped rustfmt
and `git diff --check` passed. The root reported that the optional-feature compile
and all 411 unit plus 3 headless tests passed. Native replay execution remains
pending.

## Added witnesses

- Current-revision production `v`/count/`y`, Copy/Move toggling, one Source with
  three cuts, exact forward and backward provider ordinals, unchanged duration,
  exact preview commit and full finished range selection.
- Independent removal and insertion site inspection without proposal identity
  changes. Local audition uses hand-written expected absolute sample boundaries,
  exact suffix offsets, the other site as a context cap, and playing/paused
  comparison with stale audio delivery rejection. Audio delivery is injected.
- One-frame-separated sites in both directions, with overlapping default
  context. Terminal cursor and displayed last picture are separate. Completed
  audition at `B(N)` maps to the exact counterpart boundary and back without
  subtracting one sample.
- Own-interior rejection and both exact no-op boundaries retain composed source
  endpoints, disable Apply and do not save on Enter. Undo makes a capture stale
  for Move while historical Copy remains usable; a new yank restores Move.
- Replace always uses Copy; its fixed selected range remains independent from
  the insertion destination across Replace/Move/Copy toggles.
- Native input batches retain duplicate count digits. Repeated operation/site/
  Apply/count keys are ignored while held frame motion remains repeatable.
  Native button focus owns letter keys and Enter. IME preedit and completion
  batches cannot toggle Move, change site or apply. Modified m is ignored.
- Receipt before workspace retains a nonempty old finished selection. Matching
  workspace delivery installs the full Move range once. A duplicate receipt
  after manual range navigation does not reselect.
- A real Close/Open clears the register. Old-session proposal and receipt
  delivery cannot replace a new draft. A genuine old generic range receipt
  paired with the new session's same-revision workspace cannot install selection.
- Ordinary group entry via Enter/Backspace, boundary reparenting with identical
  global Before/Proposed frames, retained physical child identity and empty donor
  group. Its terminal sample comparison remains identical on both sides.
- Cross-parent workspace delivery before generic selection completion, followed
  by root navigation. The delayed completion restores destination scope and
  range even though the revision was already visible; a later duplicate leaves
  manual root navigation intact.
- Minimum 960x640 and 1280x820 layout assertions for Move controls, both local
  timeline labels, endpoint textures and main picture, with replay captures.

## Review notes and remaining verification

Readback corrected the forward Move root assertion after the first compile.
Split legitimately creates contextual Retime partitions; the final assertion
checks that Move did not introduce an artificial Sequence wrapper. This was a
test-only correction. Source is now frozen for the root's visual replay.

The existing replay qualifies actual endpoint decoding and Metal presentation,
but its ordinary fixture does not establish NTSC PCM equivalence, real device
delivery, acoustic quality, OS IME behavior or independent physical keyboard
layout behavior. Those claims require the root's separate service/media tests,
keyboard audit and native verification. No new screenshot has been inspected by
this worker. The new Move suite does not separately inject a pending picture
during a site switch or navigate outside a site's comparison window; those
production behaviors remain covered only by other picture/logic tests until
the root adds a dedicated replay witness.
