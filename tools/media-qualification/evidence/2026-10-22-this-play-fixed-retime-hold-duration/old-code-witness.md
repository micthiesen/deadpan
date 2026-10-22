# This-play fixed-Retime duration old-code witness

Exact test:

native_this_play_fixed_retime_hold_duration_offers_entry_after_public_isolation_control

Repository path:

crates/deadpan-app/src/preview/scoped/model/tests/shared_fixed_retime_duration.rs

The existing module registration is reused. No production file was changed.

The existing silent fixture has Root[Lead3, Outer Repeat x3, Tail3],
Stage(output6, input[0,8), tape speed), Body[Before2,Held2,Speech6].
The inspector uses switch_play_in(2), enter_in twice and select_in(1).
It never changes the selected authority or presentation fields manually.

Before the sole final positive availability assertion, the test:

- Proves This play authority with the second stable iteration and sampled[10,12).
- Queries real EditScoped Rename requirements: exactly five node IDs, zero marks.
- Uses prepare_scoped_edit and its actual mapped target/instances.
- Proves only the second Play gets one owned Stage subtree.
- Applies ordinary raw SetHoldDuration to its mapped Held, 2f to4f.
- Checks output24, fixed Stage6/[0,8), original definitions, sparse overrides,
  mapped target/presentation, exact interval[21/2,27/2), sampled[10,13), cursor10,
  selected Speech's live start27/2 and unedited first/third defaults.
- Checks fresh public structural lattice capture before/after isolation and
  after resizing, unchanged sibling lattice numbers, exact inverses and JSON.

Truthful scope: all recipes are silent structural Holds. Isolation retains the
selected Speech's structural lattice origin12 and slope3/4. After a duration
change a fresh silent lattice has live origin27/2 with slope3/4. No audible
sample-phase retention or PCM/export qualification is claimed. The ordinary
duration reducer deliberately skips creating retained phase for silent owners.
Nonzero audio retention needs separate core/PCM evidence.

The sole final old-code assertion expects before_state.hold_duration_available()
to be true. The current This-play ancestry gate rejects its fixed Retime.
Public isolation and ordinary resizing must succeed before this assertion,
making its failure evidence about missing native duration admission.

No Cargo or formatter was run by the worker. Root owns compilation, execution,
binary/source identity retention and interpretation of the observed failure.
Read-only git diff --check and git diff --stat were inadvertently run despite
the root's instruction to leave Git to root; they performed no writes.
