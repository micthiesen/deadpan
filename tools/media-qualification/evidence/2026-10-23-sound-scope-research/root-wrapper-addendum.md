# Introduced group at the journal's live scope

The earlier fallback is incomplete. Narrowing BOTH the old references and the appended capture-time reference to owner discards a context that genuinely exists at capture time. Keep one live journal scope equal to the copied group, and allow only the old references to describe that scope as an explicitly introduced, unpaired root wrapper.

Bounding initial GroupSelection to one complete Source/Hold child makes this precise and removes multi-child/zero-duration alignment ambiguity. Multiple BeatSounds on that one owner remain fine.

## Required representation after capture of the new group alone

Suppose the old journal scoped Top, a fresh group G now wraps the complete owner, and the selection captures G without Top.

- Live journal scope: G.
- Pre-group references: historical scope becomes each already-proven historical owner alias. The introduced-Sequence relation includes G as an UNPAIRED LIVE SCOPE ROOT. Owner, timing, chronology and prior placement remain unchanged. Top is outside the selected forest and must no longer grant a context.
- The appended capture-time reference: historical scope G (the actual G in the newly captured layout), no introduced-root relation. This layout legitimately contains G and can supply its context.
- After paste: live G and every historical G/owner alias rename through their respective exact maps as usual. The single live journal scope remains copied G. Different references need not have the same historical scope kind.

This directly expresses the ownership history that the shared journal scope otherwise cannot: G did not exist for the early references, but did exist for the capture-time reference.

## Explicit bounded root-wrapper proof

Do not generally allow a scope to appear in a generic skip set. Make root-wrapper handling an explicit relation mode, or an explicitly validated special case of the introduced Sequence relation:

1. The listed root is exactly the live scope and is an ordinary Sequence, never the project root, historical scope, Repeat/Retime/Partition or event owner.
2. It has exactly one complete child, the independently proven Source/Hold owner, or a bounded chain of already-recorded one-child introduced wrappers ending at that owner. For the first implementation, rejecting chains is a defensible additional bound.
3. Wrapper and child have identical effective duration/extent, child offset exactly zero, no derived clock and no temporal transform. The historical scope equals the exact recorded historical Source/Hold owner alias and retains its full duration/clock.
4. The proof's pending pair is historical owner versus live owner AFTER traversing the root wrapper. Do not pair historical owner with G; G must remain absent from live_to_historical and historical_seen.
5. Record actual live edges owner -> G -> enclosing copied container in live_path. Build normal historical owner-to-root placement edges separately. Occurrence membership must still traverse the real tree.
6. Keep current ancestry/Repeat eligibility and the one-to-one owner proof. Charge root-wrapper traversal and relation entries. An unrecorded or incomplete/multi-child scope wrapper refuses.

For the capture-time reference the ordinary strict proof pairs historical G with live G. Therefore try_remap_context(G) returns None for pre-group references and Some(exact historical G) for the new reference. That is the desired distinction, not an inconsistency.

## Audio implication that must be tested

The per-reference context results are intentionally mixed. sound_clocks::placements already records historical_context/expands_history for each placement. The early references must retain their actual delivered clipping; references captured after G exists may expose G's complete input behind a LATER exterior crop. The all-placements expands_history shortcut will be false. Do not change it to true merely because the last reference maps G, or it could revive samples clipped before G existed.

The ordinary chronological route may already implement the required mixed case through each placement's allocation and anchor; this has not been run. Prove it with the explicit input and filter oracle. If it cannot expose complete post-G/pre-crop input while preserving pre-G clipping, introduce a typed ownership/birth boundary for G in preparation. Do not revert to journal.scope=owner, invent a historical G in every old table, or waive EQ cases.

## Decisive tests

- Capture G alone: assert journal.scope == G; early reference historical scope == owner alias and G unaliased; appended reference scope == historical G and G aliased. Full timing/reference identities and counts match the intended chronology.
- Copy/crop G after an odd NTSC move, then apply G EQ: its input must equal the complete DELIVERED bus at G creation transported through post-G placements, including samples hidden only by the later crop. Independently filter that signed input with Equalizer::process_tile. Samples clipped before G creation stay absent.
- Compare the two ownership orders explicitly: group before the first clipping move versus group after it. They need not expose the same hidden pre-G input; compare each to its own chronological oracle instead of asserting blind commutation.
- Copy/crop Top: preserve the existing Top alias across all references and its complete input. This remains a separate control from G.
- Recopy G, move it, reopen and Undo/Redo: early introduced-root relation follows live G's fresh IDs, new captures still pair their genuine historical G. No lost context, stale IDs or duplicate placement references.
- Reject a listed project root, a different scope, multi-child/partial wrapper, nonzero child offset, derived/temporal child, owner retarget, missing relation and malformed historical scope.

Integration is in the relation-aware correspondence root dispatch, edit_slice::sound_clocks::capture's unpaired selected-root branch, and exact wrapper renaming. Plan's existing mixed context selection remains the first implementation to test. No new media admission or receipts are needed.

Read-only design; no repository edits, builds or runtime checks were performed.
