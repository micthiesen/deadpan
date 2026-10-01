# Independent combined Trim core review

Reviewed frozen patch SHA-256
`9c6eee4968eee6efa1ee3f87f7737792c0abcb8ea0cfedd28123176d9c7b9b01`, its
README/manifests, accepted combined-authoring design, prepared-Split review and
actual staged reducer/preflight/tests. Read-only ripwire/source inspection; no
Cargo, native/media/browser execution, repository edit, commit or worktree.

## Findings

### P1: overwrite preflight accesses a nonexistent final-owner field

`work/crates/deadpan-core/src/source_trim_edit.rs:280` uses
`occupied.push(b.output_after)`, but `b` is `SourceTrimFinalOwner`, which has
`output` rather than `output_after`. This prevents the core crate from compiling.
Use `b.output`: filler subtraction must consume B's final overlaid tail, not
`b.geometry.output_after` (the potentially longer hidden candidate).

The existing overwrite table and padding-only-B tests already refer to the
correct public `.output` shape and should exercise this path after correction.

### P2: an unused B wrapper can reject an otherwise zero-allocation overwrite

`source_trim_edit.rs:170` calls the existing geometry resolver before it knows
whether the overlay retires B. That resolver immediately charges both geometric
candidate wrappers at `source_trim_geometry.rs:412-414`. The final authoring
preflight correctly omits a wrapper for retired B, but it is never reached when
the earlier provisional count exceeds capacity.

Concrete static witness:

- Direct Sources A `[0,10)` and B `[10,20)`, both with sufficient full Original
  handles and explicit selected windows.
- Capture their bindings in the small initial tree, then fill the remaining
  document node capacity with empty ordinary Sequences after B. This avoids
  requiring a new full-tree audio capture for the edit.
- Submit Overwrite `I=0, O=9, S=0, R=1` with B captured.
- A's final allocation/output is `[0,20)`, matching its grown physical duration.
  It needs no wrapper. B's final tail starts at `max(0,11,20)=20`, so B retires.
  `F=[0,20)` requires no filler or endpoint Split. The true temporary node count
  never exceeds the entry count and the final tree loses B.
- Geometry nevertheless counts B's intermediate `[1,10)` crop as one required
  wrapper and rejects `MAX_DOCUMENT_NODES + 1`.

Separate geometric candidate counts from the complete author's resource
admission, or make the early overwrite budget account for final B retirement.
Do not remove the author's real peak-node check, which must still charge both
complete exterior Split copies before removal. Add this capacity witness to the
resource tests; the existing private `peak_nodes` test cannot expose this earlier
rejection.

## Confirmed by inspection

- The overwrite footprint, final B-tail allocation and complement/filler
  arithmetic match the accepted normal form after the field correction. A is
  the only overwrite author; hidden B extension cannot consume a preceding
  sibling. Padding-only surviving B remains a retained crop.
- Original placements are captured once before Source changes or Split.
  Ripple groups distinguish A, separately rolled B and the remaining ordinary
  ancestor suffix. Positive Roll uses B's old retained overlap. SourceEndpoint
  steps retain `window=None`; each participating owner gets one new step.
  Physical prefix rebasing occurs after append and includes previous chronology.
- The prepared Split loop uses the outer allowance object and does not recapture.
  Its duration-preserving result becomes the baseline before installation and
  removal. Copied marks/lineage/bindings therefore enter exactly one final loss
  and reconciliation pass. The two Source prefixes are applied in that pass.
- Phase-only timing is installed with its referencing steps; this capture branch
  explicitly excludes neighbor Splits. Overwrite with already bound contexts
  needs neither another layout nor a new timing resource.
- Outer command dispatch bypasses generic double reduction, captures root sounds
  before structural mutation, restores them and allowances once, and computes
  the inverse against the original entry. Ripple contributes one root Trim map;
  fixed-root overwrite restores authored sound objects/routes. Fresh fillers
  receive no sound allowance and can suppress independent root contributions.
- Pool counts, global ID freshness/collisions, timing allocation, binding phase
  terms, Split copies and final document checks are enforced on private candidate
  state. Bounded wire visitors reject excess filler/Split IDs and unknown fields.
  Existing legacy adapters remain closed to the new command.

## Evidence still required

The 17 new integration tests use the intended public command/resource shapes;
all successful fixtures exercise exact inverse and forward replay. They have not
run. They cover synthetic core structure and clocks, not decoded PCM or indexed
picture behavior. In particular, execute mixed-intent PCM with historical
resumes/reanchors and a partial opaque Preserve/Repeat survivor, and indexed
pictures for the two-prefix and overwrite-tail cases. Keep full filter support
separate from selected support when writing the oracle.

Store receipt/full-span admission for every used qualified Source, CLI dry-run
parity, coordinated format versions, SQL rollback and fresh-revision Undo/Redo
remain explicit integration gates before exposure. They are correctly identified
as absent in this core-only stage, not supplied by its serialized resolution.
