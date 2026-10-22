# Nested fixed-Retime duration baseline witness

`baseline-test.rs` is a scratch-only draft for the pre-implementation old-code
control. It has not been installed, compiled, or executed. The required old-code
behavior is: public Rename isolation and the mapped raw `SetHoldDuration`
control pass, including structure, exact boundaries, sibling plays, inverse,
and JSON; only the final `hold_duration_available()` assertion is expected to
fail.

The fixture is:

```text
Sequence[Lead3, Outer x3, Tail3]
Outer -> Inner Repeat x2
Inner -> fixed Stage Retime(output 6, input [0,8), Edit, FollowSpeed)
Stage -> Body[Before2, Held2, Speech6]
```

Its total is 42 frames. The selected second/second Stage begins at 21. The
selected Hold begins at 45/2 and ends at 24 before the edit; after 2f -> 4f it
ends at 51/2. Sampled visibility changes from `[22,24)` to `[22,25)`, and the
cursor remains 22. The five other stable outer/inner Play combinations keep
their exact boundaries and 2f Hold definitions. This proves silent structural
duration behavior only; there is no audible-phase claim.

The Rename must prove exact isolation counts: 11 cloned nodes, zero marks, and
two real isolation steps. The Hold, Stage, Body, Speech, Repeat path and every
non-selected counterpart are mapped through `prepared.map_instance` or
`prepared.map_target`, not reconstructed from expected clone names.

## Compile-risk checklist

- The actual test module's `super::*` must expose its private shared fixture
  helpers (`fixture`, `repeat`, `retime`, `sequence`, `hold`, `edited`, `node`,
  `frames`, `revision`, `branch`, `state`, and `presentation`). The install
  location in the file header is chosen to preserve that relationship.
- Confirm `State::enter_in` can be called twice from the fixed Stage ancestry
  after selecting Outer second then Inner second. The cursor 22 lies within the
  selected nested Stage and Held sample envelope.
- Confirm `prepared.map_target` maps Stage and Body through both proof steps;
  `prepared.map_instance` should preserve the two stable iteration IDs while
  mapping the cloned Inner Repeat and leaf IDs.
- The exact pool assumes current isolation clones the whole Outer-selected
  Inner subtree (6 nodes), then the Inner-selected Stage subtree (5 more).
  This count should be validated by the live proof, not relaxed if it differs.
- `AnchorIndex::resolve_target` must accept an outside-in `InstancePath` with
  `[Outer, Inner]`, including mapped cloned IDs in the after document.
- The post-edit original definitions should remain byte-for-byte structurally
  equal; only newly isolated nested nodes should carry the selected Hold's 4f
  recipe. The mapped fixed Stage should retain output 6 and `[0,8)` mapping.
- Ensure no assertion follows the last `hold_duration_available()` positive
  assertion in the test. On the old code the public controls must all pass
  before that single intentional failure.
- Run a later implementation's normal focused test and formatting before
  treating this source as compile-valid. This assignment intentionally did not
  run Cargo, rustfmt, or Git and makes no verification claim.
