# Edited slice capture and insertion

`CapturedEditSlice` holds an immutable, editable copy of a selected half-open
range from Your edit. `Command::SpliceSlice` inserts it at an ordinary Sequence
seam, `SpliceSliceAt` inserts inside a named direct child, and `ReplaceSlice`
replaces a nonempty range in that Sequence. Each is one reversible transaction.
These core and headless boundaries support
the native placement workflow specified in Section 9.7; they do not establish
that its native controls, move or occurrence editing are complete.

## Ownership and capture

Capture takes a validated document, an ordinary Sequence parent, a global Edit
range and a scratch `AudioTimingId`. It does not change the document, create a
revision or write history. The captured revision records provenance. A later
edit or deletion of the original beats cannot change the captured value.

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

The command's `timing.allocation` equals its new revision. Seam insertion retains
its reserved destination ordinal. Strict interior insertion uses a pre-Split
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

## Remaining product work

Native edited-content registers, endpoint pictures and local refinement must
use this capture on the service worker and reject late copy completions by
session and request identity. Extend the existing Original placement proposal
without losing its visible unsaved state, comparison windows, audition joins,
stale-target rejection or saved-edit recovery.

Atomic move remains required. It must preserve destination clocks before endpoint
splits and before removing any selected children, and reject placement inside
its own removed interval.
Named register persistence, role-only placement, cut-to-register behavior,
motion/text-object operators and nested occurrence interiors remain required.
These workflows remain open beyond the core/headless capture and placement
commands described above.
