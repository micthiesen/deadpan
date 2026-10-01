# MoveRange media verification handoff

## Final root gate

After this handoff, root's final locked workspace run passed all 2,792 tests,
with none failed or ignored, in 1,399.00 seconds. Formatting and strict all-target
workspace Clippy with the UI harness passed on the same final source manifest
`6609d6d091868ae4b71a87a039419bf61209ed4de868f28faf5754e27975e484`.
All 1,338 inputs were rechecked. The earlier scoped execution records below
remain intact, including the corrected fixture failure.

Status: focused verification passed under the parent's serialized runner: plan 3/3 and corrected audio 9/9, with zero ignored tests. Audio rerun took 13.325 seconds total (2.62 seconds test execution), source manifest `d79a017aac115f6396f568ace13e46f4c0e52d1570ba51c204ab5c90d58357d9`, evidence `audio-tests-2.log/json`. Plan took 36.87 seconds total (0.09 seconds test execution), source manifest `ba0354f66d0a425e26e27d71e8c9d02c6cd8b8f055a88adfc70b158d96d19727`, evidence `plan-tests-1.log/json`. Both ran Rust 1.97.1 with locked dependencies at base commit `a079a46f77306f9c07376f765e1e637835436741`. Source is frozen for the parent's full gate. No Cargo, decoder, DSP worker, native process, or UI execution by this agent. No production files changed by this agent.

The initial audio run passed 6/9 and failed the three sound tests before their assertions with `SourceRangeInvalid: an explicit audio duration requires selected audio` at `composite_insert.rs:189`. My blank fixture removed source audio but retained its natural-rate explicit mapping. A Source with neither picture nor audio would also be invalid. Corrected the fixture to a picture-only `SourceVideo::Still` using the existing synthetic still-capable asset, with `audio: None` and `audio_mapping: FitBeat`. This keeps soundless timeline time without introducing an unintended silent Hold gate. Reviewing the next validation boundary also caught my sound fixture's unsupported rate change: replaced its fitted Placement with natural-rate SelectedPlacement (full source duration 2000000/147147 frames; explicit [1,3) or [0,12) selection), and changed the independent opening resample step to 147/160. No picture decoding is claimed. Owned Rust 1.97.1 rustfmt and scoped diff whitespace checks pass after these fixture corrections. The parent rerun passed all nine tests. Retained initial failure evidence: `audio-tests-1.log/json` in this directory.

## Files

- `crates/deadpan-plan/tests/edited_slice/move_range.rs`: three new tests.
- `crates/deadpan-audio/tests/composite_insert/edited_slice/move_range.rs`: nine new tests.
- Both existing `edited_slice.rs` parents declare their new module.
- Plan `edited_slice/placement.rs` exposes existing `fixture`, `is_partition`, and `assert_live_scope` to the sibling tests.
- Audio `edited_slice/placement.rs` exposes existing `bounded_source` and `prior_edits` to the sibling tests.

## Picture assertions

- Source9 moves `[2,4)` to old6 and `[4,6)` to old2 both yield original frame order `0,1,4,5,2,3,6,7,8`, requiring exactly seven Split IDs. Every frame checks independent exact source point `1001*(original+1/2)`, retained frame ordinal, owner local position, nine-frame duration and independently rounded Q32 linear framing.
- A 59-frame nested fixture moves Source/whole owned Repeat+Preserve/Freeze fragments across ordinary parents in both directions. Complete old interval orders are `[0,6),[33,52),[6,33),[52,59)` and `[0,5),[24,33),[5,24),[33,59)`. Every output frame retains its provider point, leaf owner clock, captured geometry, gap issuer, Repeat occurrence IDs and creative owner framing/clip order. Added neutral Partitions carry no pose. Every instance validates.
- Original Repeat nodes, play order/allocation, sparse play/gap overrides and whole Preserve nodes retain their exact identities. Source/destination Sequence framing stays live with changed durations and local origins; the other former parent is absent from the moved contribution.
- Stable mark IDs cover all four source-cut biases, an absolute Sequence mark, a whole-child-local mark and a departed donor-local mark. The latter keeps its original boundary and becomes `OutsideHost`.

These are canonical plan/coordinate witnesses, not decoded picture or GPU pixel evidence.

## PCM assertions and independent oracles

The existing real mono 44.1 kHz WAV fixture is decoded through `PreparedSource`. Expected sample coordinates are authored independently from the command/plan/binding implementation:

- NTSC 30000/1001 has exact frame extent `8008/5` mix samples. Each manually listed island enters at `B(old_start)`, subtracts its original exact owner origin, and fills `B(new_end)-B(new_start)`. The source step is `147/160`. The oracle separately enforces retained discrete owner support and natural-rate geometric support. Shared code is only the qualified reconstruction kernel and existing fixture helpers.
- Left/right three-cut Source9 and left/right cross-parent Source7/Source4 cases compare complete output PCM against those independent islands, then cold-tail, reverse variable-size and repeated reads against the successful full vector.
- A moved `[2,4)` interval receives 3,204 samples at `[4,6)` rather than its former 3,203. Its final retained source point6406 is asserted nonzero. A whole Source2 moved from `[1,3)` to `[4,6)` gets the same extra allocation but its last point has exhausted original owner support: explicit zero PCM plus the exact terminal suppression interval is asserted.
- Prior InsertTime/DeleteRange bindings retain the independent phase `32032/5` through another move, and the unchanged suffix remains exact.
- Full Repeat movement retains reordered births, retired historical support, sparse overrides and gap recipes. Full PCM equality uses a five-frame shift, exactly8,008 samples, plus independent source and room-tone entry checks. Displaced prefix owners are compared at their individual entries because their internal allocation widths change.
- Full nonunity Preserve4-to12 moves with complete canonical stretch input history, compared with the existing independently prepared complete Preserve reference. Original stage identity and unchanged suffix remain exact.
- Source7 owner gain is independently evaluated with documented Q32 progress; moved samples use destination parent gain(-6dB), displaced survivors keep source parent gain(+6dB), with no leaked/duplicated ancestor gain. Windows are interior to avoid newly authored edge fades.

## Root sound correction

- The decisive root `[A10,B10,C10]` case places a sound at `[1,3)` then moves A to old terminal30. Full authored PCM and recipes/routes are identical; sound remains at `[1,3)`, now over B. Opening sound phase/step is independently resampled from its complete original span.
- A prior DeleteRange creates a routed root sound. Both left/right moves retain the exact recipes/routes and full authored PCM, including an active linear root gain envelope and cold out-of-order windows.
- Silent Hold movement is separate: ungranted live gating moves to the Hold's new output position; comparison away from old/new gate boundaries is exact. A pre-existing stable Hold allowance remains and restores unchanged full root-sound PCM.

## Focused commands run by parent

```sh
rustup run 1.97.1 cargo test --locked -p deadpan-plan --test edited_slice move_range
rustup run 1.97.1 cargo test --locked -p deadpan-audio --test composite_insert edited_slice::move_range
```

The parent's full workspace gate is in progress; its result is not claimed here. These focused tests do not qualify acoustic quality, audio device delivery, decoded/GPU pictures, generated media admission, native UI, or export. Core owner supplies structural no-op/empty-node/rejection and broader property evidence; store/CLI tests belong to parent.
