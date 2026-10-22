Next increment, read-only exploration; no implementation or execution yet:
All plays Hold duration inside a fixed unclocked Edit Retime under an outer Repeat.
Use the existing ordinary SetHoldDuration fixed-window reducer; do not merely remove one-play guards.

Witness: root Sequence[Lead3, Outer Repeat3(Stage), Tail3].
Stage fixed Edit Retime(output6,input[0,8),FollowSpeed), child Sequence[Before2,Held2,Speech6].
All-Default duration 2->4, representative second outer stable play.
Expected total24, fixed Stage extent/input, no clone/override, primary visible start10.
Hold exact root[10.5,12)->[10.5,13.5), sampled [10,12)->[10,13).
Speech live start12->13.5 but retained audio origin12 and scale3/4.
Lead/Tail and outer starts remain fixed; exact Undo.

Existing guards: scoped_edit/hold_duration.rs any Retime; shared_hold_duration.rs assumes integral start and end=start+intrinsic duration; ordinary semantic hold_duration.rs forbids Repeat ancestry; app project/scoped.rs and preview/scoped/model.rs likewise split these authorities.
Reusable: insert_time/hold_duration.rs fixed_window_owners and composite::prepare_hold_duration_owners, existing fixed_retime tests include Repeat inside/outside fixed windows and compact large repetitions.
One-play reducer calls occurrence_owners_after which refuses Retime, requires integral endpoint and captures root suffix, then plain recipe reducer bypasses ordinary duration apply. This is a separate larger follow-up.
