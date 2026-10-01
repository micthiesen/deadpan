# Atomic linked moves, 2026-09-30

This checkpoint adds the verified core and headless command. Native Move controls
and the two visible join previews remain required.

`MoveRange` relocates a linked Edit interval within or between ordinary Sequence
parents in one reversible transaction. Both source and destination name the same
pre-edit revision. Whole moved structures retain their identities. Endpoint cuts
retain complete contexts and audio clocks, including three distinct cuts inside
one Source. See the [contract](../ATOMIC_MOVES.md) for exact scope and coordinates.

## Ownership and independent review

Design review found that applying ripple deletion and insertion to root sounds
would violate their ownership. An internal reorder leaves the root clock and its
duration unchanged. The implementation retains root sound recipes and routes
exactly, while live silent-Hold gates and explicit grants follow their issuers.
For `[A10,B10,C10]`, a root sound at `[1,3)` remains there after moving A to the
end, now playing over B. It does not disappear or travel with A.

Independent implementation review inspected the complete command, joint cut
planning, capacity/identity checks, audio reanchors, sound ownership, legacy
admission and all new test targets. It found no further actionable defect.
Root review added the current revision to stale-source conflict details.

## Structural and picture witnesses

Core tests check both move directions, sibling and ancestor parent changes,
three-cut identity budgets, exact no-ops, co-located empty children, fresh timing
allocation, existing bindings, marks and rejection without partial mutation.
Exact no-ops allocate no nodes or clocks, including at the final timing ordinal.
Generic command history may still record a revision; native integration must
explain such a no-op without committing it.

Picture-plan tests compare every output frame with hand-authored permutations.
They retain exact Source points, ordinals, owner clocks and Q32 framing, whole
Repeat play identities and overrides, Preserve stages and Freeze geometry.
Source and destination ancestors remain live and apply once. Mark witnesses
cover both cut biases, child-local ownership, absolute pins and OutsideHost.
These are plan and coordinate checks, not decoded picture or GPU evidence.

## Decoded audio witnesses

The audio fixtures decode a real 44.1 kHz WAV into canonical PCM. Independent
sample-entry oracles use exact `30000/1001` frame boundaries and each owner's
original source phase. Both moved and displaced content retain their entries.
One relocated interval gains an allocated sample with real continuing support;
a separate whole-owner case exhausts its old support and reports explicit
terminal suppression. Neither case hides the fractional allocation difference.

Other cases cover prior pause/deletion history, complete Repeat and nonunity
Preserve contexts, cold and out-of-order reads, retained voice gain clocks and
live destination gain. Root sound cases check a fixed unrouted sound, an existing
route with a root gain envelope, and moving silent-Hold gates with and without
explicit grants. Device output and acoustic quality are outside these tests.

## Persistence and headless behavior

Read-only preview and JSON dry-run leave authored SQL rows unchanged. An injected
history-write failure rolls back the complete document and revision. Successful
commands add one revision/history entry and match their preview exactly.
Reopen and fresh-revision Undo/Redo restore complete authored state. Stale source
or destination revisions, invalid identity pools/slots and unknown JSON fields
fail without writes.

A synthetic accepted-generation fixture retains the complete Hold provider and
assets without reviving its stale request. It does not establish decoded
generated-video quality. A separate real qualified-video fixture retains source
receipts, assets, root sounds, routes and grants through move and Undo/Redo.

## Corrected test failures

The first focused core build failed because a new external test called private
`ProjectDocument::parent_of`. The test now uses its explicit fixture parent;
production visibility did not change. The next core run passes all 101 selected
tests. The three new picture-plan tests pass.

The first audio run passed six tests and rejected three fixture documents before
executing moves: silent Sources still declared an explicit audio duration. They
now use a valid picture-only Source. Source review also corrected those fixtures'
sound mapping to natural-rate placement with an explicit selected interval; the
independent opening-sample oracle uses the exact `147/160` step. All nine tests
then passed. Production audio code did not change for these corrections.

Four persistence tests and the CLI test pass. The first workspace formatting
check found two new test module declarations out of order. Their ordering was
corrected before lint and the full workspace test run. Formatting passes in
1.43 seconds and strict all-target workspace Clippy with the UI harness passes
in 629.35 seconds, using Rust 1.97.1.

The debug app link reports the existing oversized `__eh_frame` warning. No
suppression was added. This backend checkpoint does not claim a new release
build, painted replay or native-window check.

## Integrated verification

All 2,792 locked workspace tests pass, with none failed or ignored, in 1,399.00
seconds. This includes the new tests and all existing workspace targets and
documentation tests. Formatting and strict all-target workspace Clippy with the
UI harness also pass. All three final checks use source manifest
`6609d6d091868ae4b71a87a039419bf61209ed4de868f28faf5754e27975e484`;
the collector rechecked all 1,338 inputs before delivery. Commands, failed runs,
logs, review reports and hashes are in the
[retained evidence](../../tools/media-qualification/evidence/2026-09-30-atomic-move/README.md).

Core 34/database 43 are unchanged. The user's separate Deadpan window and Space
were untouched.

## Remaining scope

Native operation selection, source-removal and destination-insertion previews,
audition, stale draft handling and final range selection remain unimplemented.
Temporal occurrence interiors, role-only operations, cut-to-register and
persistent/named registers remain open. No DP requirement or gate is complete.
