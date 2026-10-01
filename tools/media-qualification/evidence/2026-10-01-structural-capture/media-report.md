# Structural capture media regressions

Implemented in the shared checkout:

- `crates/deadpan-audio/tests/composite_insert/structural_capture.rs`, with its parent module declaration.
- `crates/deadpan-plan/tests/edited_slice/structural_capture.rs`, with its parent module declaration.

The audio test uses the existing SHA-256-verified 44.1 kHz WAV provider at 30000/1001 fps. Its 16-frame destination includes Source, RoomTone, a cropped nonunity Preserve stage, a prior positive pause insertion with retained bindings, a routed root sound with a separate sample offset, an active silent-Hold allowance, gain, explicit lineage and bound/unresolved marks. It captures one nested empty group, inserts it at every root slot, and checks all 25,626 raw and authored PCM samples exactly against the pre-edit document. Separate source and RoomTone phase oracles check real nonzero decoded content. Cold first seeks into the Preserve crop, warm repeats and shuffled full-range reads compare exact output. Existing audio bindings, sounds, routes, allowances, marks, lineage and node metadata remain equal; the shared edit helper verifies exact inverse restoration. Capture and paste both use `u32::MAX` to expose unnecessary timing-ordinal consumption.

The plan test covers every slot around three adjacent empty siblings, preserving each explicit child position and a nested copied group. It checks every frame across Sources, a Freeze with captured geometry, a framed Sequence and a two-play Repeat with a Freeze gap. Handwritten exact PTS, source ordinals, provider coordinates and provider/Sequence/Repeat/root curve coordinates accompany complete before/after picture/framing equality. The inverse restores the original document. This is synthetic plan evidence only, not decoded/GPU evidence.

Verification performed: scoped Rust 1.97.1 rustfmt and `git diff --check` passed. Cargo, native media and UI execution remain owned by root and have not been run by this agent.

Focused commands for root:

```sh
rustup run 1.97.1 cargo test -p deadpan-plan --test edited_slice structural_capture --locked
rustup run 1.97.1 cargo test -p deadpan-audio --test composite_insert structural_capture --locked
```

No production, core/store, project service, documentation or existing test bodies were changed by this media subtask.
