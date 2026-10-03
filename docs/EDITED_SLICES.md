# Edited slice capture and insertion

`CapturedEditSlice` holds an immutable, editable copy of a nonempty half-open
range or one exact direct child from Your edit. `Command::SpliceSlice` inserts
it at an ordinary Sequence seam, `SpliceSliceAt` inserts inside a named direct child, and `ReplaceSlice`
replaces a nonempty range in that Sequence. Each is one reversible transaction.
The native [placement workflow](SLICE_PLACEMENT.md) uses these boundaries for
linked Copy/Replace and the separate [MoveRange command](ATOMIC_MOVES.md) for
current-source removal. Section 9.7 remains partial.

## Ownership and capture

Capture takes a validated document, an ordinary Sequence parent, an exact child
or global Edit range, and a scratch `AudioTimingId`. It does not change the document, create a
revision or write history. The captured revision records provenance. A later
edit or deletion of the original beats cannot change the captured value.

`capture_selection` accepts `SliceCaptureSelection::Range { range }`,
`Child { node }`, or `Children { first, last }`. Child identifies exactly one
direct child, including an empty Sequence, and retains its complete subtree.
Adjacent empty siblings at the same
frame are excluded. Historical admission recaptures the full selector and
payload against the named revision. `capture` remains the nonempty Range entry.
The derived global range alone cannot identify an empty child. Children selects
the inclusive exact sibling span and keeps empty children at both endpoints.
See [structural selections](STRUCTURAL_SELECTIONS.md) for its capture,
mutation and historical admission contract.

Range serialization keeps its previous shape. A missing selector reads only as
Range using the stored interval; it never infers Child from structure or labels.
Explicit null, unknown, duplicate or inconsistent selector fields fail. Child
uses an explicit tagged selector. Core 34/database 43 remain unchanged, with
literal pre-change commands, complete transactions and stored history as the
compatibility witnesses.

The selection contains complete owned beat contexts and separate output windows:

- Whole selected units retain their structure, internal mappings, framing,
  captured Hold geometry, gain, edge choices, sparse overrides and owned marks.
- Partial Source/ordinary Hold endpoints retain the complete owner behind a neutral,
  unity `RetimePurpose::Partition`. Their curves and processing contexts keep
  their original origins and durations.
- Group contents exclude the unselected group's own framing and treatments.
  Selecting the whole group retains those choices. The project root and other
  unselected ancestors are not implicitly copied.
- Independent root sound events, sound routes and Hold allowances remain
  outside this structural ownership selection. Their source document remains
  unchanged. Root sounds at the destination undergo one insertion transform.

The initial boundary uses the existing ordinary Sequence endpoint admission:
Source/ordinary Hold fragments and complete intervening composites. Capture can
traverse nested unity Partition windows without changing historical Split
admission. It does not flatten
Repeat plays or retimed output into new source clips. Partial Repeat/Retime
occurrences and generated Hold interiors remain outside this boundary.

## Destination placement

The destination parent and its ancestors must be ordinary Sequences. Interior
insertion names a direct child and a strict local boundary inside it. Replacement
names a nonempty global Edit interval within the parent. Source and ordinary
Hold endpoints may sit behind bounded chains of unity Partition windows, so a
pasted fragment remains eligible for later interior insertion or replacement.
Whole intervening composites remain structural. These commands do not resolve
Repeat or nonunity Retime occurrences implicitly.

The read-only `slice_splice_interior` and `slice_replacement` queries return the
number of Split node identities needed. Those identities are separate from
`slice.identity_requirements()`. Validate both pools together, including unused
supplied identities and historical aliases, before constructing a candidate.
The temporary peak includes destination, split and imported nodes.

Capture original sampling lattices before splitting endpoints. Prepare the
destination suffix on the split, undeleted tree at the original insertion
boundary or replacement Out. Replace the selected child interval directly with
the imported root; never construct a shorter deletion-only clock. Existing
marks and lineage reconcile against the final structure, imported marks finish
separately, and the outer command transforms root sounds and allowances once.
Copied Holds receive no new permission to play independent root sounds.

An empty Child inserts only through `SpliceSlice` at an explicit sibling index.
It keeps its labels, framing, treatments and owned marks under a fresh neutral
wrapper without adding frames or samples. Existing bindings, timing records,
lineage, marks, sound events, routes and allowances remain exactly unchanged.
No suffix clock, root sound transform or unused timing ordinal is allocated.
The insertion is still one authored revision with an exact inverse. Interior
insertion, replacement and interval Move reject an empty source.

## Exact picture and audio clocks

The complete owner below a cropped window keeps its picture mapping and framing
clock. A freeze retains its captured geometry as well as its independent live
framing. A copied group remains a group; the new enclosing Sequence adds no
creative treatment.

Audio capture retains both the sampling lattice and the selected sample entry.
It preserves existing resume terms and reanchor chronology, complete referenced
timing layouts and opaque Preserve processing contexts. Selected moving owners
receive a crop entry while traversal stops at non-unity Preserve stages.
Destination suffix preparation preserves the old suffix entry independently.

The inserted sample allocation remains
`B(destination + duration) - B(destination)`, where `B` is the origin-based
project sample boundary. It may differ by one sample from the captured range's
allocation at fractional frame rates. Retained phase and provider support
determine that extra sample; exhausted retained support is silent. Do not alter
the destination interval, trim a reference vector or pad PCM merely to make the
old and new lengths agree.

## Independent paste identities

`identity_requirements()` returns bounded counts for authored nodes, logical
marks, historical aliases and imported timing records. The node count includes
the neutral enclosing Sequence and any endpoint windows. The caller supplies
fresh pools through `SlicePasteIdentities`; the core does not generate IDs.

Every paste renames current nodes and marks, all referenced historical layouts,
and audio lineage allocation/origin pairs. Repeat identities are renamed within
families connected by explicit live/historical binding relationships. Unrelated
Repeats can share old allocation/ordinal values without becoming one family.
Compact play order and complete birth Run support, including retired plays,
remain intact. No operation expands every Repeat play to perform this rename.

The command's `timing.allocation` equals its new revision. Positive seam insertion
retains its reserved destination ordinal; empty structural insertion uses none.
Strict interior insertion uses a pre-Split
lattice ordinal, then a suffix ordinal. Replacement uses a pre-Split ordinal
only when endpoints need splitting and a suffix ordinal only when material
follows its Out. Imported records follow the used destination ordinals. Check
only the consumed interval, so whole-range replacement needs no unused suffix
clock. Combined identity, node, mark, clock, treatment, serialization and work
limits apply before a candidate is admitted. Frozen layout indexes are rebuilt
after renaming. Ordinary subtree insertion's positional Repeat normalization is
not a substitute for this path.

## Marks

A mark is a boundary with one or more physical bindings. Partial windows retain
only selected bindings, using exact coordinates and boundary bias: Right at an
internal In, Left at an internal Out. Outside bindings are not clamped into the
copy. A whole selected unit retains its own hidden intent.

Each retained logical mark gets one new identity. Its fragment ownership,
loss policy, label and unresolved state remain meaningful after renaming.
Already unresolved intent does not become bound just because an address happens
to exist at the destination. Absolute Sequence pins remain absolute and follow
their explicit loss policy when they cannot survive. Ambiguous Source occurrence
resolution fails capture explicitly.

## Persistence and media admission

The serialized capture is bounded and structurally validated. Paste requires
the same project and presentation basis. Immutable media is shared, while beat,
mark, play and timing identities remain independent.

The store reads the named immutable capture revision before admitting a
slice placement and verifies the complete payload against a deterministic capture
of that parent and range, using its retained scratch timing identity. This also
rejects valid historical media from outside the declared selection. An exact
qualified asset record or accepted generated artifact in that verified capture
may be retained even after its last beat was deleted or a
legacy source registration was undone. Current source-profile rules still
apply. Caller-supplied clipboard metadata cannot establish source qualification
or accept a new generated artifact. Reusing accepted media does not revive its
old generation request or require the model to remain installed.

Preview is read-only. Commit records one command, patch, revision and history
cursor change atomically. Undo and Redo restore authored state under fresh
revision identities; they cannot make an old destination request current again.

## Native register and previews

[Project registers](NAMED_REGISTERS.md) extend the default copy with a–z
slots. They use the same immutable captures, historical admission and saved-cut
receipt path described below. A successful named write also updates the default
copy; paste and placement retain the selected slot's exact content at entry.
The bank is saved independently of timeline revisions and restored after reopen.

### Frame cuts at the cursor

`x` cuts one linked picture and sound frame at the retained Edit cursor;
`12x` cuts up to twelve. The interval stops at the displayed ordinary Sequence's
end and never falls back to the selected beat. At that end, outside the group,
or with an active or finished Visual selection, including an empty one, it
refuses without changing history. Use `d` for a Visual selection. Original,
Sources and Placed sounds cannot direct `x` at the retained Edit cursor.

`:delete-frames 12f` captures the cursor, group, session, revision and eligibility
when command entry opens. Bare `:delete-frames` means one frame. It accepts only
positive whole-frame amounts up to 4,294,967,295 with an `f` suffix; extra
arguments, zero and overflow fail. Its hint shows the captured half-open range,
actual frame count and any group-end clamp. A later reply cannot supply a missing
target or retarget a stale command.

The resolved interval uses the same `CutEditSlice` service as Visual deletion.
Capture, deletion and register persistence succeed together; one
Undo restores the removed content. The saved receipt reports the actual interval
and frame count. Unsupported partial composite endpoints reject the complete
interval rather than shortening it to a convenient child boundary. Held `x`
does not repeat edits, and native controls, text, IME and modified shortcuts
retain input. A dialog opened earlier in the same input batch blocks the cut.
Frame cuts need no separate authored command. The shared project bank uses
database schema 53.

### Whole beats and ranges

In Your edit, `v`, motion and `y` capture the selected range on the project
service. The durable capture retains its source revision; runtime copies receive
fresh session/request identities on reopen. Copying creates no history entry.
Later edits, Undo and reopening preserve the saved copy. A newer yank supersedes
an older UI confirmation even when its focused pane rejects copying. It cannot
cancel an accepted save. Versioned bank snapshots retain durable contents;
late or duplicate confirmations cannot finish a newer selection.

Without a Visual selection, `yy` captures the complete selected child. An empty
Visual selection remains an error and cannot fall back to a beat. `d` cuts a
nonempty Visual range; `dd` cuts the selected child. `:delete` retains its exact
command-entry target, including absence. The service privately captures that
target, commits one `DeleteRange` or `DeleteRipple`, and publishes the copy only
after save. Capture, admission and commit failures keep the previous register.
A saved cut retains both its copy and a dedicated receipt if refresh fails,
with explicit reopening guidance. Exact successful retries return that receipt.
New yank/cut intent supersedes an older pending confirmation without cancelling
an already queued authored cut. Undo restores removed content and retains the
saved copy. The deletion and register update share one SQLite transaction.

Normal Edit `y` or `d` followed by frame, beat or group-boundary motion copies
or cuts from the retained cursor to that motion's destination. Use one positive
distance count, such as `y5l` or `5dl`. A copy preserves the cursor and selected
child; a cut selects the join. Empty motion intervals refuse. These
[typed selectors](SEMANTIC_MACROS.md) use the same planner as macros and
headless requests, retaining the exact historical capture for media admission.

Fast `p/P` inserts beside the selected beat or replaces the selected Edit range.
`:splice` opens the same visible placement workflow used for Original slices.
In/Out refinement recaptures the historical source within its ordinary Sequence
parent, leaving the register unchanged. Source endpoint pictures remain available
when destination preflight rejects placement. Copied Edit and destination Edit
clocks have separate labels; the main viewer can inspect either endpoint.

Empty copied groups show a structural card with the historical label and path.
They have no source endpoint picture or audition job. Destination picture and
caption remain available. Exact Sequence slots distinguish equal-time siblings;
the prepared result retains its slot and new node, and commit selects that node
without inventing a Visual time range. Positive Child copies can use interval
Move because its endpoint rules exclude adjacent empty siblings. Empty groups
use cut/paste to move.

The store issues an opaque `AdmittedSliceView` containing the exact immutable
document, historical media receipts and session handles. A standalone source
view materializes the slice in an empty neutral Sequence. It excludes unselected
source ancestors and destination framing. A placement view captures the exact
command and committed base. Neither writes history. Qualified historical sources
and accepted Generated providers retain their original admission requirements.

`Snapshot::proposed_edit_slice` binds those store-issued values; the existing
strict Original proposal path remains separate. Catalog validation happens at
construction, with Arc identity checks on repeated reads. Temporary edited views
check session liveness around warm and cold reads and before publishing cached
audio batches. Ordinary committed playback keeps its existing warm private-PCM
behavior after the store closes. Workers compile and decode away from the UI.

Commit uses the exact prepared request in one transaction. A successful receipt
survives refresh failure. The stale visible workspace retains its selection and
cursor, and a coalesced copy completion cannot replace reopening guidance.

## Remaining product work

The [atomic MoveRange command](ATOMIC_MOVES.md) relocates current contents between
ordinary Sequence scopes, preserving whole-unit identities. Native `:splice`
provides explicit Move selection and local removal/insertion picture comparison
and audition. Historical copies remain copyable but cannot authorize removal
from a newer revision. See [native qualification](qualification/native-move-2026-09-30.md).
Role-only placement, text objects, analysis-dependent motions and nested
occurrence interiors remain required.
These workflows remain open beyond the capture and placement commands described
above. Native media, interaction and performance evidence is recorded separately
from the core timing proofs.
