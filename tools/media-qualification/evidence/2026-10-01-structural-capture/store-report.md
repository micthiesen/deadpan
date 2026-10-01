# Whole-child capture and zero seam paste: store verification

## State and ownership

Store tests are at a coherent first-compile point. The root agent owns Cargo,
native media work and all execution. This agent has run only scoped Rust 1.97.1
rustfmt and whitespace checks. New live-tree tests have not yet been compiled or
run. No production changes or UI operations were made by this agent.

Owned source files:

- `crates/deadpan-store/tests/edited_slice.rs`: one child declaration
- `crates/deadpan-store/tests/edited_slice/child_capture.rs`
- `crates/deadpan-store/tests/edited_slice/child_capture/range_compat.rs`
- `crates/deadpan-store/tests/edited_slice/child_capture/retained_asset.rs`
- `crates/deadpan-store/tests/fixtures/edited_slice/range-v1.json`
- `crates/deadpan-store/tests/fixtures/edited_slice/README.md`

## Literal pre-change evidence

The scratch generator was appended only to
`baseline/crates/deadpan-store/tests/edited_slice.rs` under this task directory.
The original file prefix remained byte-identical to the retained baseline:
SHA-256 `20247de412eb31a6856996e53fc4e85a04a330e91555c2630065a1f45bd9fc23`.
All production source came from root's isolated commit
`2ea675646abc24e8410616af74b4193ed4af51da` snapshot.

Root ran the old generator successfully: one test, approximately 2m07 compile
and 0.43 seconds test execution. `old-range/range-v1.json` and closed, validated
`seam.deadpan`, `interior.deadpan` and `replacement.deadpan` packages were emitted
by that old code. The generator uses only existing fixture helpers, ordinary
core requests and real store create/commit/validate/reopen.

The 602,324-byte literal JSON was copied byte-for-byte into the checked-in
fixture. SHA-256:
`ea862010de94871087d282da55f982c0b967abb9f06d688af64f1dd7a5de6aca`.
It retains original document, range capture, source removal request/transaction,
and complete before/request/EditTransaction/after strings for all three slice
placement envelopes. Every case also retains the old writer's literal revisions,
history, state and redo rows. There are two durable old history entries per case.
No expectation was regenerated using changed production.

`range_compat` requires the new Range reader to normalize the missing selector
only to Range, and the writer to retain byte-identical capture/request JSON.
Recapture, complete typed transactions, transaction JSON, output JSON and inverse
all match the literals. It restores old rows into a fresh package shell without
applying commands to create expected history, opens read-only/read-write, performs
full validation and both Undo/Redo steps across reopen. Old history rows and old
revision document strings must remain exactly unchanged. This covers current
schema34/database43 history, not a new migration or relaxed legacy grammar.

## New structural cases

1. **Positive whole child.** Capture one complete Group between same-time empty
   siblings. Exactly one part retains Group and descendants; empty neighbors are
   absent. Source-only and proposed views preserve the expected structure, generic
   read-only preview works, and writer-required sealed factories reject read-only
   access. All authored rows and visible snapshot remain unchanged by preparation.

2. **Historical zero child and exact seam.** Run both an entirely empty project
   and one with existing physical Hold/Repeat clocks. A nested empty group retains
   static framing, gain, two biased local marks, an inner mark and an unresolved
   mark. Capture consumes zero timing slots. Delete the original, close/reopen,
   deserialize its historical capture, and insert it between two specific empty
   siblings. `u32::MAX` timing ordinal succeeds because no ordinal is consumed.
   Exactly one insertion history row changes structure with zero duration delta.
   The neutral wrapper retains the nested group and metadata. Existing marks,
   clock bindings, lineage and root bus stay unchanged. Copying the pasted wrapper
   succeeds. Reopen/full validation/Undo/Redo preserve authored state and history.
   Closing the store revokes its temporary zero source view.

3. **Historical selector forgeries.** Same-label/same-time sibling, wrong parent,
   shifted zero bounds and altered payload label remain structurally valid but
   fail historical recapture. The sibling forgery consistently renames its node
   address throughout nodes, parts, marks and lineage, so this is not a dangling
   reference test. Standalone and CommandRequest deserialization plus pure core
   application precede rejection by generic preview, sealed placement, standalone
   source view and commit. All authored cells and current snapshot stay unchanged.
   Missing selector on zero content remains an invalid old Range at ingress.

4. **Retained mark asset and real root bus.** Register the existing qualified
   `cfr-bframes.mp4` through managed Original storage and verified video/audio
   sessions. An unresolved Source mark belongs to the empty group; its asset is
   admitted only because the named historical revision registered it. Build a
   root SoundEvent, a live Hold allowance and an existing deletion route, then
   prove zero paste preserves their complete records, bindings, lineage and old
   marks. Undo through registration, reopen with the asset absent, and admit the
   zero source view against the retained historical receipt. A forged receipt
   fails every store path without writes. Valid zero paste restores the exact
   asset and mark without importing the independent root sound. Full validation
   and read-only reopen resolve the historical registration normally.

## Root execution command

```sh
rustup run 1.97.1 cargo test -p deadpan-store --test edited_slice --locked child_capture -- --nocapture
```

The five focused tests include the qualified-media fixture, which uses the
existing bounded native session path. Root may run the full edited_slice target
after the focused run to cover adjacent historical preview and placement tests.

## Limits and pending checks

Root's first focused execution compiled all five tests: four passed, while
`historical_empty_child_paste_is_one_structural_revision_with_exact_slot_and_undo_redo`
failed at `child_capture.rs:104` in fixture setup. The zero-time fixture unwrapped
`audio_lineage`, which canonical serialization omits when empty. The first
correction avoided the unwrap but still used mutable indexing, which inserts a
missing field as null. Root's integrated-first run again passed the other four
cases and failed this fixture with `InvalidJson` (null instead of the identity-key
object, line 1 column 33). The second correction uses
`get_mut("audio_lineage").and_then(Value::as_object_mut)`, preserving omission.
The remaining mutable object accesses in these owned fixtures target the
required `nodes` map or the known top-level object; optional maps are written
intentionally as complete values. Scoped Rust 1.97.1 rustfmt and diff checks
passed. The twice-corrected case awaits root's rerun. The passing tests include literal old
Range transactions/history, historical qualified media/root bus, positive child
capture, and forged selector rejection. Tests verify durable structure, closed
wire compatibility, historical media admission and absence of preview writes;
they do not decode zero-time pictures or qualify PCM, GPU pixels, native UI,
device output, export, cut-to-register or persistence of a clipboard. Core owns
selector grammar/size/depth limits and unsupported zero interior/replace/move
behavior. No production store gap has been demonstrated yet.
