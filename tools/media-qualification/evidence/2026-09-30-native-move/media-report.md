# Native Move media verification

Status: source ready for parent-run focused checks. Owned Rust 1.97.1 rustfmt and scoped `git diff --check` pass. No Cargo, media worker execution, UI, or process-worker launch by this agent. No production files changed. These new tests have not yet run.

## Owned files

- `crates/deadpan-app/src/project/tests/splice_equivalence.rs`: new child declaration; current-source PCM adapter now distinguishes strict Original receipt identity from the exact store-issued edited-view receipt identity. Existing Original checks are retained and strengthened with `validate_original_proposal`.
- `crates/deadpan-app/src/project/tests/splice_equivalence/move_range.rs`: three new service tests.
- `crates/deadpan-app/src/worker/edited_slice_tests.rs`: one new Move admission/decoder regression. No parent module declaration changes required.

## Cases and independent evidence

1. Same-Source Move in both directions on the qualified 120-frame CFR A/V fixture:
   - `[20,30)` to original60 gives `[0,20),[30,60),[20,30),[60,120)`; result `[50,60)`, removal20.
   - `[70,80)` to original10 gives `[0,10),[70,80),[10,70),[80,120)`; result `[10,20)`, removal80.
   - Every proposed frame is checked against an independently authored original-ordinal map. Actual decoded metadata, original PTS (`ordinal*1001`) and RGBA bytes are checked at removal and both insertion edges against explicit Original ordinals. After commit those complete pictures, framing and captured context equal the preview.
   - Canonical limited PCM, limiter gains/suppression/context metadata at all three joins plus a non-silent displaced impulse window agree with the committed revision. Cold final-site and reversed warm reads agree.
   - Independently specified sample islands use the 48 kHz fixture, exact NTSC `B(f)=round_even(f*8008/5)`, original-sample step1 and complete source support `[0,192192)`. Raw PCM around all join windows must equal direct qualified source reconstruction, without querying a plan or audio binding to derive expected phases. These cuts are five-frame multiples; backend tests separately cover one-sample allocation differences.

2. Cross-parent forest containing two partial Source fragments and one complete owned Freeze:
   - Seed original `[0,40),[40,80),[80,120)` under live groups A/B, insert five-frame Freeze39 with captured framing between the first two sources, then move global `[30,55)` from A into B's source at old105.
   - Result `[80,105)` contains three roots; its first child is only10 frames. `validate_result` must accept the contiguous forest without manufacturing a wrapper.
   - Every resulting frame uses an explicit ordinal formula. Actual decoded pictures cover removal, both group boundaries, insertion edges and both Hold edges. Hold identity, its static framing and captured geometry remain unchanged; A/B framing uses final global origins and durations60/65, and old parent framing is absent from transferred content.
   - Interior authored PCM independently applies A's +6dB to the retained prefix and B's -6dB to transferred Original30 samples. A fixed root sound at frame30 remains in root time. Its actual bus samples are independently reconstructed as Original50 at A's gain plus the root event's Original0 samples at its own -6dB, without inheriting A's gain.
   - Sound recipes/routes remain exact. Limited PCM and decoded pictures across all joins then match the committed result.

3. Qualified historical register:
   - Capture genuine current media, change source framing/revision, then request Move against the new destination revision with the old captured source. Move must reject the older source revision while preserving decodable copied endpoints20/29. The same register still prepares historical Copy. No proposal changes persisted history.

4. Worker boundary:
   - A genuine sealed Move proposal goes through unchanged `Work::EditedProposed`, decoding explicit shuffled ordinals at both sites against Original metadata/bytes.
   - Strict `Work::Proposed` rejects the edited admission. Alternate seal, substituted document Arc, cancellation, new base identity and stale source revision reject.
   - Closing the store revokes warm and cold temporary Move views; reopening does not revive them. The existing ordinary committed warm private Original decoder remains usable after close.

## Shared API used

`Proposal.operation: Operation::Move`; `Prepared.parent`, `Prepared.range`, `Prepared.movement` (`source_before`, `destination_before`, `removal_after`), and `Prepared::validate_result`. The implementation stays on existing `PreparedMedia::Edited`, `Snapshot::proposed_edit_slice` and `Work::EditedProposed`. No new Move decoder or production boundary adjustment was needed from source inspection.

## Parent focused commands

```sh
rustup run 1.97.1 cargo test --locked -p deadpan-app project::tests::splice::equivalence::move_range
rustup run 1.97.1 cargo test --locked -p deadpan-app worker::project_tests::edited_slice_tests::admitted_move
```

Also rerun the existing parent `project::tests::splice::equivalence` tests because the PCM test adapter gained the sealed edited branch. Parent owns compilation, fixes found by execution, and full gate scheduling.

## Limits

Decoded RGBA equality and canonical plan framing/context are checked separately. This does not prove GPU-composed pixels, physical display, audition device delivery, listening, UI routing/layout, export, historical generated providers, or broader Move structural admission. Parent owns rendered/native replay; service owner covers unsupported/no-op/receipt behavior. No passing new-test result is claimed yet.
