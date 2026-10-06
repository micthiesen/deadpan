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

## Revision storage (database schema 64)

Revision documents are stored compactly (`ProjectDocument::to_compact_json`)
and as keyframes plus patches:

- A commit writes only its patch: an edit's history entry, or an undo/redo
  revision's own row in `revision_patches`. Its document is the marker `null`
  unless it is the initial revision or a keyframe. Nothing is rewritten when a
  child is committed.
- Each revision row records `depth`, the number of patches since its nearest
  stored ancestor, and `json_bound`, an upper bound on its compact document
  length. A revision is stored as a keyframe when its depth would reach 64 or
  its bound would exceed the 64 MiB document limit, so any revision, the head
  included, is rebuilt from at most 63 patches, and a document that could
  exceed the limit is serialized and checked exactly.
- The bound is the parent's bound plus the stored patch's length plus 4 KiB.
  A patch carries every after-value it installs, serialized by the same
  `Serialize` implementations as the document and wrapped in more syntax
  (keys, `before`, `after`, nulls), so a patch grows the compact document by
  less than its own length; the slack covers a map field appearing for the
  first time. History rows hold the forward and inverse patch together, which
  only loosens the bound. The randomized store test checks the bound against
  the actual serialization after every commit.
- `snapshot`, `snapshot_at` and register, render and source-registration
  readers rebuild an elided revision from its nearest keyframe with
  `DocumentPatch::apply_stored_in_place`, which keeps every before-value guard
  and validates only the final document.
- History validation checks the stored metadata of every row: a keyframe's
  bound covers its document, and an elided row extends its parent's chain by
  exactly one patch with a bound at least the parent's plus that patch and the
  slack. By induction every bound covers its true document.

Database 64 refuses every earlier schema unchanged; see [development
formats](DEVELOPMENT_FORMATS.md).

## In-memory head and validation reuse

The store keeps validated documents of committed revisions in memory, keyed by
revision identity (`DocumentCache`): the head and the revision it replaced.
Revision identities are never reused once committed and revisions are
immutable, so an entry stays correct. Entries are added only after a write
transaction commits or by a read outside any transaction; a read inside a
transaction only consults the cache, because it may observe that
transaction's own uncommitted revision. The head is resolved through the
stored head identity on every call, so a read-only store still observes
another writer's newer revision. The writer lock makes the open writer the only
process that changes history; a row changed underneath an open store by
another program is detected by the next open or `validate`, not by the open
store.

`ValidatedDocument` (core) pairs a shared document with the durations and
per-owner audio binding proof its validation produced. Inside
`ValidatedDocument::scope`, `durations`, `structural_durations` and
`validate` on that exact document return the retained result: a pure function
memoized on a value that cannot change while it is borrowed.

A commit is now:

1. Take the cached validated head (no parse).
2. `deadpan_core::apply_validated`: the command runs inside the head's scope,
   so the input is never revalidated, and the result is validated once. The
   result equals the forward patch applied to the head (the patch is the
   complete difference and identity fields cannot change), so the store
   adopts it without applying and validating the patch again.
3. Every check history replay would perform for the new revision: the stored
   command and edit decode to the computed values, the inverse restores the
   head exactly (equality with a validated document needs no validation),
   store admission and transition rules as before, and the per-revision
   receipt and single-original checks for any changed asset or presentation.
4. Write the revision row (marker plus metadata), history entry, cursor, and
   extend the history receipt, in one transaction; then cache the result.
5. The native workspace refresh takes the same validated document and compiles
   its plan inside its scope, so the plan does not validate it again.

Undo and redo apply the stored, rebased patch with its guards and validate the
restored document once.

### Validation invariants

Each commit validates its result completely, except for one reuse with an
exact equivalence argument. The families checked by
`ProjectDocument::durations`, and what each depends on:

| Invariant family | Reads | Commit |
| --- | --- | --- |
| Schema, node/asset/override counts, edge count, presentation range, root kind | Counts and scalar fields | Recomputed |
| Asset records | Each asset | Recomputed |
| Tree shape: one parent, acyclic, all reachable, depth ≤ 256 | Whole tree | Recomputed |
| Per-node labels, edge policies, kind rules, durations, Repeat layouts, Retime ranges | Node, its children's durations, its assets | Recomputed |
| Sound events, allowances, targets | Sounds, owners, durations, assets | Recomputed |
| Framing, picture context, gain and captured-context budgets | All nodes (aggregate limits) | Recomputed |
| Cutaways and captions on Source/Hold only | Each node | Recomputed |
| Tails outside speed stages | Ancestor Retimes | Recomputed (skipped when no speed stage exists, which cannot fail) |
| Basis state | Presentation, durations | Recomputed |
| Audio lineage | Lineage map, nodes | Recomputed |
| Audio bindings, per owner: wire size, reanchor anchors, placement templates against their timing tables | That binding and the immutable tables it names | Reused when the patch changed neither the binding nor any table it names |
| Audio bindings, aggregate: record, entry, node, run and work limits; every table referenced; sound clocks; live owners, kinds, resume bounds, Repeat ancestry | Whole binding state and document | Recomputed |
| Marks | Marks, structure, durations | Recomputed |

The per-owner binding checks are pure functions of exactly that binding and
those tables, and they spend a fixed amount of the shared work budget. A
reused owner spends the same amount at the same position in the same owner
order, and only when the remaining budget strictly exceeds that amount: with
at least that much left, every check inside it receives at least the budget
it used before and succeeds identically. Otherwise it runs again. Owners
iterate in the same order, so every aggregate and failure point is unchanged.
`changed_ids` uses the same binding patch for its changed owners and is
checked against the complete comparison in debug builds.

Validating the whole tree structurally on each commit is deliberate. Local
rechecks of the tree would need maintained parent, depth and reachability
indexes; a wrapped or moved subtree changes the depth of everything below it,
and marks and binding ancestry depend on durations and Repeat ancestors
anywhere above a change. The complete pass costs about 9 ms on 10,000 beats
with retained clocks, and it remains the single source of truth.

### Command work reuse

The remaining whole-document passes inside a command were repeated, not
required. Each replacement below computes the same value once or computes an
exact local equivalent; `deadpan_core::with_reference_command_work` restores
the previous computation on the current thread as an oracle.

| Step | Before | Now | Why it is the same |
| --- | --- | --- | --- |
| Lineage and mark transforms | Each rebuilt the result's structural durations | One shared value per structure (`command::shared_structure`) | Only `audio_lineage` changes in between, which the structural pass does not read |
| Final result validation | Repeated the structural pass | Takes that shared value when the shared post-processing (prune, compaction, basis lock) ran without sound, beat-sound or allowance restoration | Those steps change bindings and basis state only |
| Split command | Split validated its result, then the transaction validated it again | The transaction adopts Split's complete validation when pruning removed nothing, no new timing table exists and the basis did not lock | The result is then the Split result with a new revision identity, which validation does not read |
| Split's anchor index | Every parent, offset and Repeat layout | Built only when a bound occurrence mark lies in the target; otherwise the target's parent is found directly | The index is read only to relocate those marks; the input validated, so building it cannot fail |
| Pause Split input | The captured copy (head plus new bindings) was validated from scratch | Its durations are the head's (`with_proved_durations`) | A copy differing only in a binding state that `validate_for` accepted against the head validates to the head's durations; no other invariant reads bindings |
| Intermediate validations (Split result, capture, resume terms) | Every binding owner's placement checks | Owners unchanged from the validated head in scope reuse its proof, through the exact binding patch to that head | The same argument as committed results: reused checks are pure functions of the binding and its immutable tables |
| Capture byte limit | Validated again, then serialized the complete binding state | Layouts remember their exact compact length; bindings use the structural bound binding admission already relies on; exact counting only when the bound exceeds the limit | `to_json` after a successful validation fails exactly when the length exceeds the limit |
| Layout admission | Built the JSON text to measure it | Counts bytes without retaining them, once per layout | Same length, same limit |
| Patch diff, lineage comparison | Collected the key union and looked every key up in both maps | One merged pass over both sorted maps; lineage ancestors use hash maps | Same comparisons in the same key order |
| Store inverse check | Copied the result, applied the inverse, compared whole documents | `DocumentPatch::restores`: the same guards and errors, then a field-by-field comparison of the would-be result | Equal to `apply_stored(from) == to`, checked on every random step |

`crates/deadpan-core/tests/command_work.rs` runs 48 seeded random sequences of
40 edits over Hold-only, linked-Source and placed-sound documents (pauses at
seams and interiors, Splits anywhere and of occurrence-mark hosts, Repeat
wraps with and without gaps, ripple deletes, Hold durations, Local,
Occurrence and Sequence marks, play counts, Group, Ungroup, Retime wraps,
root and beat sounds and Hold allowances, including refusals of at least four
kinds) and requires
identical transactions, results, durations and errors with and without the
oracle, through both the validated and the unscoped entry points, and
identical `restores` results for matching, reversed and conflicting patches.
Separate tests compare the capture's byte check with `to_json` below and above
the structural bound (12,000 newly bound Holds) and with sound clocks (no
bound), and check that scoped binding reuse rechecks every owner of a
replaced timing table. The oracle and these probes are compiled only for
tests (`test-support` feature).
Debug builds also assert at each reuse that the shared or proved durations and
any adopted validation equal a complete recomputation.

The structural walk and each validation family still run once per commit over
the whole document; on a 10,000-sibling root a pause also composes one resume
term per later physical owner, which the authored semantics require.

## Verified history receipts

Every revision's stored rows (revision, keyframe metadata, history entry or
navigation patch, Compound step reservations) are linked into a SHA-256 hash
chain in chronological order. The single `history_receipt` row records:

- the validator build: a SHA-256, computed by the store's build script, of
  every workspace crate the store reaches in the `Cargo.lock` dependency graph
  (core, media, analysis, jobs, source and the rest, derived rather than
  listed; tests, benches and examples excluded), `Cargo.lock` itself, the
  workspace manifest and `rust-toolchain.toml`. An unreadable input fails the
  build. Any change to that code invalidates every receipt, including edits
  that do not change behavior, which only costs one replay;
- the number of revisions it covers, the last one and the chain value there;
- a digest of the single-original profile row, and a chained digest of the
  qualification receipt rows that existed, by rowid.

Opening a package hashes every stored row (no document or command is parsed),
checks every row's keyframe metadata and the history cursor structure, and
compares the chain at the receipt's last revision. When the validator,
profile and qualification digests and the chain match, replay starts after
that revision; otherwise from the initial revision. The per-revision receipt
and single-original checks follow the same prefix. A writer then certifies
the complete chronology. Each commit extends the receipt after performing
every replay check for its revision (step 3 above); a commit that changes the
single-original profile leaves it, and the next writer open replays, except
initialization, which validates the new profile against the two covered
revisions and records it. A read-only open never writes, so after a validator
change or a receipt-less package, read-only opens keep replaying until a
writer opens the package and recertifies it. Recovery checkpoints never trust
a receipt: before publishing one, the store bounds every stored value and
replays the complete history of the copy.

Any modified, removed, inserted or reordered row changes the chain from that
revision on, so it is recomputed and rejected if it no longer agrees.
`project validate` (`ProjectStore::validate_full`) ignores the receipt and
recomputes everything; `project validate --quick` performs what opening
performs.
`ProjectStore::open_validation` and `validate_report` say how many revisions a
receipt proved and how many were recomputed.

Trust model: the chain detects accidental corruption and changes made outside
Deadpan that do not also rewrite the receipt. It is stored in the same file
and is not keyed, so it is not authentication against someone with write
access to the package, who could equally have rewritten history before
receipts existed. Validation remains an integrity check, as before.

## Register bank digest (database schema 65)

Named registers are operational state outside the timeline history, so the
history chain does not cover them. Register contents were already addressed by
the SHA-256 of their canonical JSON. The 2026-10-05
[adversarial run](qualification/adversarial-2026-10-05.md) showed that the slot
table and bank version were not covered: a lost, renamed or retargeted slot row
or a changed version still validated. `register_state.bank_digest` now stores a
SHA-256 over a domain tag, the bank version and every `name:content-id` slot in
name order. Every bank write updates version, slots and digest in its
transaction, and every bank read recomputes the digest, so opening (which reads
the bank during receipt validation), `project validate --quick`,
`project validate` and checkpoint publication refuse a mismatch as
`Registers`. Version semantics are unchanged: it is the cache identity
`register_version` reports and advances on every durable bank write; the
digest binds it to the slots it describes. The trust model is the history
chain's: unkeyed corruption and partial-tamper detection, not authentication.

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
`crates/deadpan-store/tests/revision_storage.rs` covers keyframe placement and
size bounds, exact reconstruction of every revision through edits, undo, redo
and reopen, randomized commits compared with complete validation and an
independent rebuild from storage, receipt extension, invalidation and tamper
detection, and bounded pause history entries. Core `validated_tests` compare
reused binding proofs with complete validation, accepting and rejecting. See the
[incremental commit record](qualification/incremental-commit-2026-10-05.md).

## Commit-path costs

Frozen layouts share one immutable allocation, so cloning or comparing a
document copies no retained table. Parsing a document no longer re-serializes
its admitted binding state or layouts to recheck the input's byte bound, and a
binding whose structural size bound fits skips exact size serialization. A
commit parses no document and serializes only its patch (keyframes excepted),
and validation does not rebuild admitted layouts' indexes. See the
[incremental commit record](qualification/incremental-commit-2026-10-05.md).

## Remaining costs

- The first edit that binds owners still captures every physical owner's
  lattice, and the full-structure table they share: on 10,000 Holds about
  3 MB of bindings and 2.8 MB of layout, retained once.
- Each commit still validates its result once over the whole document, and
  a pause validates its Split intermediate; see
  [command work reuse](#command-work-reuse). A pause still captures a
  complete-structure layout and builds its index before slicing it, and on a
  wide root composes one resume term per later sibling.
- Every edit's patch carries the changed parent Sequence's complete children
  list before and after, in both directions; on a 10,000-sibling root that is
  serialized, checked and written (with `F_FULLFSYNC`) on every commit.
