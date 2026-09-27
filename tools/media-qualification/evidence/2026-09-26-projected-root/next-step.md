# Continue the full Deadpan goal

This turn implemented and independently reviewed `AudioProjectedRoot` and
`StageAudio::read_projected_root`. Root allocation uses absolute RoundEven;
intrinsic preparation remains PointCeil. Exact policy is regridded before
rounding. Crop/resume retain full intrinsic context, old root policy labels,
current sample phase and explicit support exhaustion. Separate physical domains
keep independent anchors. Read the completed gate report and qualification
before relying on the final verification counts. This is a borrowed evaluation
API, not authored nested insertion or normal root-plan compiler integration.

All earlier single-Original, Documents library, visible-key UI, ImageGen, native
review and complete-spec obligations remain active. The full goal is not complete.
Git metadata is read-only; preserve the verified checkpoint, harness and assets.

## Next core representation proposal (read-only map, not accepted schema)

The implementation peer mapped the current core after the source freeze. A
candidate is sparse `EffectiveClockOverrides` keyed by stable NodeId, compiled
into a separate exact timing index. Ordinary nodes retain current integer
behavior. Source/Hold overrides map an exact retained authored-local range to an
exact effective output extent. Retime overrides map an exact range in the child's
effective clock to an exact effective output extent. Sequence prefixes sum exact
extents; Repeat prefixes remain compact over stable iteration runs, sparse play
overrides and exact gap extents. Root extent must remain integral and representable.
Authored Source/Hold/Retime counts and mappings stay integer. Do not dilate every
duration to manufacture an integer grid; that changes physical Preserve input.

Review the shape before implementation. It must establish a real atomic command,
not just another detached query. For Retime 3→2, a root cut and one-frame insertion
at 1 map to child cut/span 3/2. Leaf retained windows become [0,3/2) and [3/2,3),
the authored one-frame Hold has effective extent 3/2, and the live Retime maps
child [0,9/2) to output 3. With outer 3→2 and inner 4→3, inner effective output
grows to 9/2 and its child to 6. Preserve both actual owner ancestors over the Hold.

Concrete core consumers requiring integration:

- `document.rs` authored durations and validation; proposed `effective_timing.rs`
  exact compiler/types, without empty scaffolding.
- `anchor.rs` and `anchor/location.rs`: `BoundaryScope.duration`, Sequence
  offsets and descent currently carry FrameDuration/i64. All must use the exact
  timing index; merely storing a map is insufficient.
- `repeat_layout.rs`: compact exact child/gap/play prefixes, no play expansion.
- `command.rs`, `insert_time.rs`, `split.rs`, `occurrence_edit.rs`, `marks.rs`
  and audio-binding lifecycle: guarded per-node patch deltas, inverse/replay,
  changed IDs, copy/rekey/prune, exact anchors, isolation and mutation admission.
- `deadpan-plan/src/plan.rs`: exact Sequence, Repeat, leaf and Retime maps while
  retaining integral root duration. Framing already has `evaluate_exact`.
- `core/audio_reference.rs` FrozenAudioLayout still persists integer node
  extents, Retime FrameRange and RepeatLayout. Subsequent capture/resume cannot
  reconstruct a derived clock unless this frozen vocabulary also represents it.
  Explicit rejection would be an interim limit, not completion of nested editing.

Extend InsertTime using locate_boundary, outside-in Repeat occurrence isolation,
exact leaf retention, insertion under its actual Sequence/group owner, explicit
implicit-gap materialization, and one transaction for framing ownership, marks,
edges, lineage, bindings, routes and clocks. Capture original audio timing before
zero-time isolation/splitting. Retain lower picture context and live ancestors
once. The newly implemented borrowed projections still need persisted owned
input/output routes, source dependency identity and ordinary compiler integration.

Existing Hold duration, Repeat, move/group, split/delete and occurrence commands
must transform or explicitly reject stale overrides, never silently drop them.
Later Retime edits must retime the inserted Hold with its owner.

## Closed migration boundary

A persisted clock feature would require core 25→26 and database 31→32. Introduce
closed `legacy_v25`, replay DB31 as V25, and keep DB30 as V24. Freeze core25's
contextual InsertTime admission in the V25 adapter before widening current
admission. Legacy document/request/patch grammars reject new clock fields and
new fractional command vocabulary even when empty or null. Do not replay old
histories through widened current admission by assertion.

Meaningful regressions: the two fractional Retime examples; one selected play in
a very large compact Repeat; integral-root and overflow failure atomicity; exact
marks/anchors; patch and durable undo/reopen; later duration/Repeat edit on a
clocked subtree; authentic DB30 and DB31 replay; forged DB31 nested insertion
history that current code could execute but old admission must reject.

No representation, schema version, new command admission or migration from this
map has been implemented. Keep the proposal separate from measured capability.
