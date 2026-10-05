# Compact retained timing and revision storage

Pauses, ripple deletes, moves, splices and trims retain each shifted owner's
pre-edit audio clock ([owned bindings](OWNED_AUDIO_BINDINGS.md),
[reanchors](AUDIO_REANCHORS.md)). The semantics are unchanged: every shifted
physical allocation keeps its exact pre-edit sample grid and entry. This page
describes how that state is stored so that storage and edit cost follow what
an edit changes rather than the project size times the number of pauses.

## What grew

Measured on the 10,000-Hold `large-10000` fixture before this change
([performance record](qualification/performance-2026-10-04.md),
[timing-storage record](qualification/timing-storage-2026-10-05.md)):

- Every timing capture stored a `FrozenAudioLayout` of the whole structure
  (about 284 compact bytes per node). Each pause created a new one for its
  newly unbound owners or phase terms, and the old ones stayed referenced, so
  four pauses held four 2.8 MB tables.
- Composite edits appended one reanchor step per shifted owner per edit,
  even when the step could not change any resume.
- Each history entry stored the complete `AudioBindingState` four times (both
  sides of the forward and inverse patch): 58 MB per entry after a few pauses.
- Each revision row stored the complete pretty-printed document.
- Validation charged every retained layout node, so the fifth pause failed
  with `audio binding work exhausted`.

## Sliced timing tables

A placement only projects its own alias through its ancestors: origin, scale,
Repeat play layout, Retime selections and meaningful support depend on the
ancestor path and on sibling durations, never on sibling contents. After each
command, `compact_new_timings` slices every timing table the transaction
introduced (`FrozenAudioLayout::sliced`):

- Keep every alias any placement names (physical owner, clock root, Repeat
  arguments, birth definition roots and captured Repeats) plus all ancestors,
  with their exact nodes, override keys and lineage.
- Replace every other subtree by a duration-preserving spacer under its first
  alias: a silent Hold, or an empty Sequence for zero duration. Runs of such
  Sequence siblings merge into one spacer; zero-length runs vanish.
- Existing tables are never rewritten. Tables named by sound clocks stay
  complete, because sound clocks compile and compare whole processing subtrees.

A later pause's table therefore holds its path and a few spacers (4 to 9 nodes
on the 10,000-beat fixture) instead of the whole project. The first capture
that binds every owner still needs every owner's path.

## Inert reanchor steps

A step adds `B_step(entry) - B_step(anchor)` to the resume phase and moves the
anchor to `entry`. When no placement in the binding or the step names a Repeat
argument, birth or gap, every occurrence and definition resolves the same
clocks. If the step's entry then equals the current anchor, or it has no entry,
its contribution is exactly zero for this and every later step, and local-origin
rebasing shifts both values equally. `AudioBindingState::reanchor_step_is_inert`
checks this by resolving the binding with and without the step; the shared
append path omits such steps. Owners under Repeats always keep their steps.
Unsplit suffix owners of a pause, delete or move no longer gain a step each.

## Granular binding patches

`DocumentPatch::audio_bindings` is an `AudioBindingPatch`: guarded changes to
individual timing tables (sorted `AudioTimingChange` records), owner and gap
bindings, and the sound-clock relation. Every before-value is checked; the
complete result is validated with the document. Deserialization charges every
layout's collections before materializing it. A later pause's history entry now
holds its new table, its new and changed bindings and the changed nodes.

## Revision storage (database schema 63)

Revision documents are stored compactly (`ProjectDocument::to_compact_json`)
and as keyframes plus patches:

- The newest revision always keeps its document. When a child is written, the
  parent's document becomes the marker `null`, unless the parent is the initial
  revision, every 16th revision in the linear chronology, or lacks a stored
  patch.
- An edit's patch is its history entry's forward patch. Undo and redo store
  their rebased forward patch in `revision_patches`.
- `snapshot_at` and register, render and source-registration readers rebuild an
  elided revision from its nearest stored ancestor with
  `DocumentPatch::apply_stored`, which keeps every before-value guard and
  validates only the final document. At most 15 patches are applied.
- History validation recomputes every command as before. It compares stored
  documents when present and stored navigation patches always; an elided
  revision must have its patch. A tampered patch fails validation.
- Schemas 59 to 62 upgrade in place by adding the empty table; their existing
  revisions keep complete documents. The writer upgrades only after the old
  history validates. Commands now record retained timing differently, so an
  older package whose history contains a timing-retaining edit (pause, ripple
  delete, move, splice, trim) cannot replay and is refused as
  `UnsupportedSchema` without any change. The current revision and the initial
  revision must keep their documents.

## Qualification

`crates/deadpan-audio/tests/timing_representation.rs` runs random edit
sequences (InsertTime at seams and interiors, Split, Repeat wraps with silent
and room-tone gaps, ripple and range deletes, MoveRange and undo) over Sources, room-tone and silent
Holds, Preserve retimes and optional root sounds at 30000/1001, 24 and 25 fps.
Each step runs once compact and once inside `with_reference_timing_representation`,
which disables slicing and step omission. It requires identical authored
structure, identical resolved root clocks, a compact document no larger than
the reference, and bit-identical authored-bus and limited-output PCM over the
whole timeline. Granular patches must round-trip their wire and restore the
previous revision. See the [qualification record](qualification/timing-storage-2026-10-05.md)
for the release run and measurements.

Tests that assert step counts or complete layouts run inside the reference
representation; the property test above proves the compact form equivalent.
`crates/deadpan-store/tests/revision_storage.rs` covers elision, exact
reconstruction of every revision through edits, undo, redo and reopen, a
tampered navigation patch, and bounded pause history entries.

## Commit-path costs

Frozen layouts share one immutable allocation, so cloning or comparing a
document copies no retained table. Parsing a document no longer re-serializes
its admitted binding state or layouts to recheck the input's byte bound, a
binding whose structural size bound fits skips exact size serialization, and
a transaction validates its entry revision structurally once rather than in
full.

## Remaining costs

- The first edit that binds owners still captures every physical owner's
  lattice, and the full-structure table they share: on 10,000 Holds about
  3 MB of bindings and 2.8 MB of layout, retained once.
- Commits still parse, validate and serialize the whole document, so edit cost
  is linear in project size. Opening a package still recomputes every command.
