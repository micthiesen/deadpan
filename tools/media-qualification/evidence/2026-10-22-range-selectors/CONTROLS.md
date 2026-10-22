# Range-set old-code controls

Drafted against the current public Core source during the whole-node-set phase.
These files have **not been installed, compiled or executed** by their author.
No repository files were changed. No Cargo, rustfmt or Git writes were run.

The integration controls use `#[path = "range_set_fixture.rs"] mod fixture;`.
Copy the fixture next to the two test files when installing the baseline tests.
`draft-identities.json` records exact SHA-256 and byte counts for the drafts and
the eight public source/test files consulted. These are observed file identities,
not proof that a commit contains them. Root must freeze and record its committed
baseline, actual executed binary hashes, case inventory and results separately.

## Counterfactual

`range_set_counterfactual.rs` constructs one 24-frame Source through public
`ProjectDocument::new`, `AddAsset` and `Insert`, with no fabricated media result.
All commands verify JSON round trips, forward/inverse application and document
round trips. The asset is explicitly synthetic structural metadata, without
audio or a decoder claim.

The direct public captures are `[2,6)`, `[10,14)`, `[18,22)`. Each retains exactly
the original complete Source node, the same asset and snapshot, one exact part
mapping, and consecutive capture-timing ordinals. Existing semantic Motion Yank
is an independent same-range control with an unrelated Visual selection. The
separate public SpliceSlice oracle proves this literal result:

| Output | Source |
| --- | --- |
| `[0,4)` | `[2,6)` |
| `[4,8)` | `[10,14)` |
| `[8,12)` | `[18,22)` |

It verifies actual root child order, each transparent Partition mapping and
duration, the complete Source recipe under each partition, and three distinct
physical Source owners. The selection sum is 12 frames; the `[2,22)` hull is 20.

A canonical candidate `CapturedEditSet` JSON object is assembled from these
already validated existing capsules, with one scope and an empty mark map.
On the old code it must reject specifically at `whole-child owned components`.
This proves that the first blocker is admission, while preserving the later
problem: every range component legitimately repeats the same authored Source
ID. A future implementation needs component-qualified identity spaces.

The sole positive assertion on the old code is the last strict SemanticProgram
wire admission using `{"type":"ranges","ranges":[...]}` in reverse input
order. This shape awaits the design owner's confirmation. The old baseline
should fail only there after every preceding control passes.

When exercising a new implementation, preserve the old frozen test and binary.
The candidate-set negative expectation must become positive validation and real
union execution in the new regression; it must not remain an expected refusal
after range support exists.

## Shared unresolved mark multiplicity

`range_set_shared_mark.rs` creates a temporary sibling host with `Insert`, uses
`SetMark` to put one `KeepUnresolved` mark owned by the Source but hosted by that
sibling, and removes the host with `DeleteRipple`. Public transforms create the
actual unresolved state; the test never edits document or mark JSON directly.

Two disjoint range capsules `[2,6)` and `[18,22)` retain the *same* original mark,
with one identical unresolved physical binding each. The separate SpliceSlice
oracle contains two distinct physical Source owners and one binding for each,
under two fresh MarkIds. It verifies owner existence, source recipes, exact
range mappings, label, bias, loss policy, coordinate and unresolved state.

Separate paste intentionally does not establish correct union semantics. The
future union must have **one logical MarkId with two physical bindings** after
each component gets its own destination owner. Deduplicating equal source-side
bindings before owner renaming loses one of these physical bindings. Copying
the hull instead gives 20 frames and only one physical owner, which is also
wrong. The new regression must exercise the actual union path and assert
logical-mark count 1 and binding count 2, not simply reuse this oracle as proof.
