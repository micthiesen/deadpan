# Aligned grouping while preserving historical context aliases

Historical design draft. Its owner-only live-scope fallback for copying a new
group is superseded by `root-wrapper-addendum.md` and the current `decision.md`.
Retain the group as the live journal scope, using an explicit unpaired unary
scope root only in older references. This document is research, not implementation.

Recommend keeping each broad journal/reference scope and frozen timing table unchanged, and recording the newly introduced ordinary Sequence wrapper as an explicit per-reference live structural relation. Teach SoundClockCorrespondence to pass through only those recorded wrappers, without giving the new wrapper a historical context alias. This preserves old Top aliases and the distinction between old and newly introduced treatment owners.

This is more work than narrowing scopes, but it retains the existing complete-history contract through later exterior crops. The pending simple PCM probe, even if green, cannot establish that contract for future copy/crop operations.

## Why a transient or inferred solution is insufficient

The correspondence is rebuilt from saved documents by Plan, Store validation, capture, paste and later edits. A command-local proof cannot carry the neutral-group permission after reopen. Leaving only unchanged scopes with a more permissive child-count comparator loses provenance: equal duration, equal leaf shapes and matching sample ranges do not prove which old Sequence context corresponds to which new one. In particular, a fresh wrapper and a copied existing one-child Sequence can have identical ranges but different treatment-owner history.

Do not flatten every Sequence or use equal-looking nodes to discover aliases. Keep the existing one-to-one pairing for every old node, including existing Sequence context owners. AudioLineageId is explicitly not sufficient clock/admission authority.

## Smallest durable relation

Add a bounded, strict per-SoundClockReference set of introduced live Sequence IDs, analogous in purpose to the explicit Introduced relations in SoundClockRepeatMap. Empty means today's exact proof. A GroupSelection inside a retained scope adds its fresh group ID only to older references whose paired scope contains the grouping parent. No clock is appended and no old timing/scope/owner identity changes.

The relation says only: this ordinary Sequence wrapper was introduced after this historical reference and is transparent for independent raw clock correspondence. It does NOT grant its own historical context, media, new sound support, silence permission or processing expansion. Later current EQ/saturation on that wrapper still applies through the ordinary current graph.

Keep the initial admission bounded to the same aligned Source/Hold/pure-Sequence case. This relation should not quietly authorize new Repeat/Retime/Partition or duration edits.

## Correspondence rule and invariants

In audio_reference.rs::sound_clock_correspondence_inner / paired_sound_clock_children, compare the old ordered child list with the current ordered children after recursively passing through ONLY reference-listed introduced Sequences. Preserve actual parent edges in live_path for occurrence validation. Pair every pre-existing node as before; do not insert skipped wrappers into live_to_historical or historical_seen.

Validate each listed node: it exists exactly once, lies strictly below the paired live scope, is an ordinary Sequence, is not the scope/root/independent owner, and has no derived clock. Every listed wrapper must actually be visited by the proof. Preserve exact child order, zero-duration child identities, summed duration/effective clock and the complete old subtree; no leaf may be added, omitted, duplicated or reordered. Charge nodes, edges and relation entries before traversal, with one bounded aggregate budget and the existing scope-level proof cache.

The same old Top and other old Sequence nodes remain in live_to_historical, so try_remap_context_with_work still maps their contexts. The new wrapper is deliberately absent, so its context returns None for those old references and consumes delivered historical input. A later reference captured after grouping naturally contains that wrapper and can pair it normally; the relation must therefore be per reference, not journal-global.

## Critical future copy/crop scenario

After grouping inside Top, capture/copy/crop the complete Top and put EQ/shared processing on Top. Older Top aliases must remain proven inside every relevant reference. StageAudio::read_beat_sound_owner_input must still expose the same complete historical support, including any terminal label excluded by the new rounded delivery allocation. Full Top PCM and its filtered tail must match the independently constructed equivalent operation order.

Then repeat with treatment on the NEW wrapped group. Its pre-group references must not acquire an invented historical alias. It consumes delivered input at that new ownership boundary. This difference is observable after exterior crop, even if a simple uncropped grouping probe is numerically green.

Copying Top preserves the broad scope and renames each introduced-wrapper ID with the exact live-node map. Historical tables remain unchanged except their existing qualified copy renaming. A newly appended capture-time reference has an empty introduced-wrapper set because its frozen layout already contains the group.

Capturing the new group alone needs an explicit small capture adjustment: existing edit_slice::sound_clocks::capture tries historical_node(new_scope), which correctly has no alias for the new group in old references. When the selected root is an introduced unpaired wrapper, choose the already-proven complete independent owner scope inside the selected forest, rather than inventing a historical group. This excludes Top, which was not copied, and keeps the new group's consuming boundary external to old references. Filter wrapper entries to those still within the chosen live scope. For the initial Source/Hold-only case that narrower paired scope is unambiguous.

## Exact integration points

- sound_clock.rs: strict bounded relation on SoundClockReference; old constructors default empty; validation includes relation size and live membership.
- audio_reference.rs: wrapper-aware correspondence overload while existing public methods pass empty relation. Keep ordinary exact pairing and owner identity checks unchanged.
- sound_clock/grouping.rs and sound_events.rs: prepare the aligned command's precise relation updates once, stamp the command/document, and bypass only its retained-scope guard. group_selection.rs installs complete prepared journals before validate. No scope narrowing.
- deadpan-plan/src/plan.rs::beat_sound_clock_scopes: pass the saved relation into the existing proof; try_remap_context_with_work needs no semantic change.
- edit_slice/sound_clocks.rs::capture: preserve relations on retained references, implement the unpaired-new-wrapper capture rule above, append an empty relation on the new capture.
- edit_slice/rename.rs::Inventory/Renamer and occurrence_edit.rs: rename introduced live wrapper IDs through exact current-node maps; never map them as historical nodes.
- sound_clock/edit.rs::split/restore: carry wrapper IDs through the existing exact copied-node map and filter/narrow only when the already-supported lifecycle chooses a genuinely smaller live scope. Preserve old references and append an empty relation for a newly captured layout.
- Strict store/copy admission must recapture and validate the relation; no new media/provider/DSP implementation is required.

Ungroup removing a recorded wrapper should remain refused in the first slice. Supporting removal later requires an explicit relation retirement transform, not silently accepting missing wrapper IDs. Full-owner deletion can use existing journal retirement. Do not leave stale IDs or infer a replacement wrapper by range.

## Why not rewrite equivalent historical layouts

Purely adding a neutral Sequence to each frozen layout can preserve old contexts and coordinates, but must allocate new timing identities rather than mutate retained tables in place, rewrite every affected reference and maintain exact historical owner maps. More seriously, normal strict pairing would also grant the NEW wrapper a historical context alias, which it never owned. Avoiding that still requires an explicit unpaired-wrapper relation or equivalent typed metadata. Rebuilding layouts therefore adds storage/identity work without removing the essential authority distinction.

## Required proof

Retain old-code exact guard failure after independent positive controls. Require all existing Top/context aliases to remain mapped, the fresh group alias to remain absent in pre-group references, and all timing tables/scopes/owners/order to remain exact. Compare complete decoded-WAV PCM plus signed Top input through two NTSC shifts, existing and subsequently authored Top EQ/shared nonlinear stages, copy/crop Top, copy/crop the new group, recopy, Undo/Redo and reopen. Include the terminal-sample hidden-history regression patterns in sound_clocks/partial.rs and owner_input.rs, source revocation, source/shape impostors, zero-duration children, work bounds and atomic failed writes. No treatment refusal should substitute for preserving valid semantics.

Read-only design research. No repository changes, builds, runtime helpers or experiments were performed.


## Final bounded proof API contract

Keep existing public correspondence entry points exact by default. Add one explicit relation-aware owner-correspondence entry point accepting: the two immutable layouts, exact old/live scopes and owner pair, existing Repeat relation, the per-reference introduced Sequence relation, and a maximum work budget. Return the existing SoundClockCorrespondence plus charged work. Do not implicitly enable flattening from matching durations or general topology.

The relation belongs to the reference's CURRENT side. Each entry names one introduced live ordinary Sequence and grants only transparent child traversal for this reference. No entry can create a historical_node alias for that Sequence. A command preparation constructs entries only from the current GroupSelection's fresh group identity after exact selector/preflight validation; store/capture admission must preserve that authored relation instead of rediscovering it from shapes.

Proof construction must:
1. Validate the entire relation before installing any alias: sorted/distinct/bounded IDs, every node exists, ordinary Sequence kind, strictly within the named live scope, never root/scope/event owner, no derived clock. Every entry must be traversed exactly once.
2. Preserve the exact old scope pair and every pre-existing Sequence/Source/Hold alias. Flatten only relation-listed current wrappers when consuming the corresponding ordered child run. Keep every zero-duration old child in order; do not match solely by interval.
3. Compare all old processing nodes/durations/effective clocks and applicable Repeat descriptors as the existing strict proof does. Preserve one-to-one historical coverage and the explicit event-owner pair. No old subtree may disappear because a listed wrapper happens to have the same range.
4. Retain ACTUAL current parent edges through skipped wrappers in live_path. Retain historical parent edges unchanged. Occurrence remapping still proves real membership; skipping a wrapper in alias pairing must not detach its descendants from the real path.
5. Expose old Top and other paired contexts through historical_node/try_remap_context unchanged. For each pre-group reference, historical_node(new_group) remains None. References captured after grouping contain the group normally and carry an empty relation for it.
6. Charge relation entries, child edges, indexes and pair work against one checked aggregate budget; cache by exact old timing/scopes/Repeat relation/introduced-Sequence relation and owner-proof mode. Do not reuse a result with a different wrapper set.

If a bare wrapper-ID list leaves ambiguous pairing with an existing one-child Sequence, refuse ambiguity rather than choosing by equal range. Preserve the command-prepared exact parent/selected-child witness or strengthen each relation entry with its exact historical parent/ordered child-run aliases. Those aliases rename through the timing's historical map while the introduced Sequence renames through the live-node map. This keeps the relation explicit; it does not authorize arbitrary associative Sequence flattening.

For the aligned command, install prepared journal relation changes only after the structural group exists and before post-group validation-dependent queries. Preserve the pre-group clone with old valid journals for mark/lineage transforms. Unchanged scopes and timing tables remain valid after relation-aware proof is available. Already-accepted outside-scope/no-conflict grouping keeps byte-identical clocks.

## Missing tests that decide this design

- Alias identity, directly: old Top and every old internal Sequence still map to their exact historical aliases; new_group has no old alias; all actual live parent edges include new_group. Include nested one-child old Sequences and identical-duration siblings to expose positional ambiguity.
- Existing Top EQ/shared nonlinear stages: complete PCM and signed Top owner-input equality before/after grouping, including an NTSC terminal label outside a newly rounded allocation.
- After grouping, copy/crop Top, then apply/change Top EQ. Its old alias must preserve complete history. Use the independent full-input Equalizer::process_tile oracle in owner_input.rs; test warm and cold partitions.
- After grouping, copy/crop the NEW group, then apply EQ to that group. Its pre-group references must consume delivered input. Reuse partial.rs's new-wrapper-EQ control; do not substitute group-before-prefix as its oracle.
- Copy/recopy renaming with disjoint authored and historical IDs and an initially empty AudioLineage map. This catches an identity-only proof that passes locally but fails after paste/reopen.
- A new capture reference after grouping: old references retain the introduced relation; appended reference has no relation for the group and can pair it as present in its own layout.
- Nested introduced wrappers, zero-duration selected children, multiple events sharing one old scope, multiple historical references, and two wrapper sets sharing the same timing/scopes. Cache keys must not collapse them.
- Unsupported/malicious cases: missing/outside/duplicate/unused wrapper IDs, a listed Retime/Repeat/Partition, scope/root/event-owner listed as wrapper, changed/reordered/omitted old child, owner retargeting and ambiguous one-child Sequence pairing. Reject before mutation.
- Later move, plain Split, supported endpoint edits and full-owner deletion: propagate or retire relation IDs through exact provenance. Ungroup removing a recorded wrapper remains refused initially. Undo/Redo/reopen/portable copy and forced Store failure must preserve the exact relation bytes and existing histories.

Derived historical layouts are not selected: they would manufacture an old context alias for the fresh group. General inferred flattening is not selected: it loses explicit provenance and can confuse existing versus introduced contexts. Root owns the shared type/schema and integration; this report contains no code draft.
