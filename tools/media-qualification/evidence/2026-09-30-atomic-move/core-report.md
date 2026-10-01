# Atomic MoveRange core handoff

## Final root gate

Root's final locked workspace run passed all 2,792 tests with none failed or
ignored in 1,399.00 seconds. Final formatting and strict all-target workspace
Clippy with the UI harness passed. The shared final source manifest is
`6609d6d091868ae4b71a87a039419bf61209ed4de868f28faf5754e27975e484`.
The collector rechecked all 1,338 inputs. Earlier command-specific results below
retain their original scope and source identities.

## Implemented

One linked command relocates a nonempty range between ordinary Sequence parents.
All addresses refer to the same pre-edit revision. Same-parent and cross-parent
placements support explicit seams and Source/Hold/unity-Partition interiors.
Complete composites retain their identities. Joint endpoint planning handles up
to three cuts of one physical context; a plain Source/Hold requires seven IDs.
Empty boundary children remain at source, interior empties move, and destination
slots preserve co-located empty ordering. Exact topology no-ops allocate no nodes
or clocks and retain the normal revision-only transaction convention.

Physical owners in the moved and displaced intervals receive one reanchor each
from one unchanged-time layout. Existing phases and retained provider support
remain complete. Source and destination ancestor effects stay live. Marks retain
logical identity, Split boundary bias, absolute Sequence pins, unresolved intent
and the existing cross-parent OutsideHost policy. Source revision conflicts now
include the current revision in their typed error.

Root sounds and routes remain byte-equivalent because the root owner clock and
duration are unchanged. They are detached only around soundless structural work.
Allowances follow Split's issuer remapping and final live Hold gates. The initial
design's root cut/gap map was incorrect; the corrected plan follows
`docs/SOUND_EVENTS.md:220,233-237` and the independent design review. There is no
new root-sound operation or route grammar.

## Frozen public API

```rust
Command::MoveRange {
    source_revision: RevisionId,
    source_parent: NodeId,
    range: FrameRange,
    destination: MoveRangeDestination,
    identities: SplitIdentities,
    timing: AudioTimingId,
}

pub enum MoveRangeDestination {
    Seam { parent: NodeId, index: usize },
    Interior { parent: NodeId, target: NodeId, at: FrameDuration },
}

pub struct SequenceRangeMove {
    pub range: FrameRange,
    pub destination_before: ProjectFrame,
    pub inserted: FrameRange,
    pub removal_join: ProjectFrame,
    pub required_ids: usize,
    pub timing_slots: u32,
    pub is_noop: bool,
}

impl ProjectDocument {
    pub fn range_move(
        &self,
        source_parent: &NodeId,
        range: FrameRange,
        destination: &MoveRangeDestination,
    ) -> Result<SequenceRangeMove, EditError>;
}
```

For source `[a,b)` and original destination `d`, insertion starts at `d` if
`d<=a`, otherwise `d-(b-a)` when `d>=b`. The removal join is `b` for `d<a`,
otherwise `a`. Reject `a<d<b` and destinations inside selected subtrees.
Timing uses the request's new revision and zero to two consecutive ordinals:
optional original unbound capture before Split, then optional placement capture.
Validate the entire supplied node pool, including unused IDs and historical
aliases. No paste/import identity pool or historical media admission is involved.

## Owned files

- New `crates/deadpan-core/src/move_range.rs`: API, preflight, joint cuts, pools,
  clocks, direct final relocation and mark/lineage reconciliation.
- `src/command.rs`, `src/lib.rs`: command dispatch, description and exports.
- `src/insert_time.rs`: expose existing Split count/pool helpers within core.
- `src/audio_binding_lifecycle.rs`: read-only unbound recipe inventory.
- `src/sound_routing.rs`, `src/sound_events.rs`: exact root-bus detach/restore
  and structural command admission.
- `src/legacy_v29.rs` through `src/legacy_v32.rs`: reject new command from
  closed historical grammars; old Move remains unchanged.
- New `tests/move_range.rs`: eight focused behavior/failure tests.
- `tests/sound_routing.rs`: unchanged unrouted/routed root recipes, routes,
  whole-Hold identity and allowance ownership.

## Verification status

Worker ran no Cargo and no UI. Root owns serialized verification.

- Root `rustup run 1.97.1 cargo check -p deadpan-core --locked`: PASS,
  20.48 seconds. Logs `core-check-1.{json,log}` bind the pre-error-detail source
  manifest. This preceded the `current_revision` error field fix and new tests.
- Root reports four persistence tests PASS, 67.21 seconds total. See
  `persistence-tests-1.{json,log}` for exact commands/source manifest.
- Independent implementation review found no actionable defect; see `review.md`.
- Root first focused test compile failed with E0624 because the new external
  sound test called crate-private `parent_of`. Corrected the test to use its
  explicit fixture parent `group`; no production API was widened. Failure log:
  `core-tests-1.{json,log}` (55.84 seconds).
- Root rerun PASS, exit 0 in 16.47 seconds:
  `rustup run 1.97.1 cargo test -p deadpan-core --test move_range --test edited_slice --test split --test marks --test sound_routing --test sound_allowances --test sound_events --locked --no-fail-fast`.
  See `core-tests-2.{json,log}` and source manifest
  `ba0354f66d0a425e26e27d71e8c9d02c6cd8b8f055a88adfc70b158d96d19727`.
  This includes all eight new MoveRange tests and the new root-bus regression.
- Owned Rust 1.97.1 formatting and `git diff --check -- crates/deadpan-core` PASS
  after the stale-error fix and focused test additions.
- Picture, decoded PCM and broader gates are pending root execution at this
  report point. No runtime success inferred from review or compilation.

Focused targets ready for root: `--test move_range`, `--test sound_routing`.
Relevant established regression targets: `edited_slice`, `marks`, `split`.
Tests cover identity retention in both directions, exactly-once reanchors, joint
three-cut counts, boundary no-op at maximum ordinal, ancestor relationships,
empty ordering/parents, mark boundary bias and OutsideHost, already-bound clock
budget, stale source, missing/colliding IDs and ordinal overflow, and unchanged
root sounds/routes/grants with inverse and serialization checks.

## Remaining scope and uncertainty

Native Move controls/proposals and both-join audition are not implemented here.
Temporal composite/occurrence interiors, role-only editing, named registers and
cut-to-register remain product follow-ups. No migration is added. Retained
physical support and live-ancestor output still require the independent audio
and picture tests already written by the separate media worker. Standard
document, binding, mark, depth and serialized-command bounds remain authoritative;
no play expansion or source rendering occurs in core.
