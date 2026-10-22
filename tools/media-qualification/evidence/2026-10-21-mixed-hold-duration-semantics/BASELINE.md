# Native mixed Hold baseline candidate

Scratch only. No repository changes or execution performed.

Install mixed_duration.rs as:
    crates/deadpan-app/src/preview/scoped/model/tests/mixed_duration.rs
Apply the one-line registration.patch to model/tests.rs.

Root-owned baseline command:
    cargo test --locked -p deadpan-app --lib preview::scoped::model::tests::mixed_duration -- --nocapture

Expected old behavior: both tests reach the final positive
State::hold_parameter(workspace) == Some("hold-duration 2f") assertion and
fail with actual None. Any earlier failure is fixture/setup failure.

Qualified offset-bframes Original, real Freeze and prepared RoomTone audio;
root45 fixture; no Retime or placed sounds. Outer Play2 / inner Default
is ordinal1/Default, with nine isolated nodes and raw duration +1. Outer
Default / inner Play1 is Default/ordinal0, with four isolated nodes and raw
duration +2. Both have a real representative at19 before mutation.

Raw positive control uses existing public scoped Rename preparation solely to
execute branch isolation, then restores the old label and applies raw
SetHoldDuration to the mapped Hold in immutable copies. Checks mixed identity,
actual mapping, unchanged original/owned recipes, exact output46/47, new
representative19/20 and one/two resized uses. This is not proof of a new atomic
temporal command or sound/mark journal; those remain subsequent tests.
