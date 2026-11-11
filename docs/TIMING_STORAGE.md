# Compact retained timing and revision storage

Original-audio placement stores nothing: it is derived from the current tree
([sampling clocks](AUDIO_SAMPLING.md)), and cut edges likewise
([audio edges](AUDIO_EDGES.md)). The per-owner sampling bindings, reanchor
steps, sliced timing tables and provisional tables this page once described were
removed on 2026-11-11 (core 59/database 89). What remains stored is
`AudioBindingState` (the document field `audio_bindings`): the independent
beat-sound clock journals and the frozen `FrozenAudioLayout` timing tables they
name by `AudioTimingId`. `DocumentPatch::audio_bindings` is an
`AudioBindingPatch` of guarded changes to individual timing tables and the
sound-clock relation. Every before-value is checked and the complete result is
validated with the document. Deserialization charges every layout's collections
before materializing it. Tables named by sound clocks stay complete, because
sound clocks compile and compare whole processing subtrees. This page covers
how revisions, validation and command work are stored and reused so that cost
follows what an edit changes rather than the project size.

## Revision storage (database schema 64)

Revision documents are stored compactly (`ProjectDocument::to_compact_json`)
and as keyframes plus patches:

- A commit writes only its patch: an edit's history entry, or an undo/redo
  revision's own row in `revision_patches`. Its document is the marker `null`
  unless it is the initial revision or a keyframe. Nothing is rewritten when a
  child is committed.
- Each revision row records `depth`, the number of patches since its nearest
  stored ancestor, and `json_bound`, an upper bound on its compact document
  length. A keyframe is due every 64 patches and is normally written right
  after the edit, in its own transaction ([deferred
  keyframes](#deferred-keyframes)). A commit stores the keyframe itself when
  its depth would reach 128 or its bound would exceed the 64 MiB document
  limit, so any revision, the head included, is rebuilt from at most 127
  patches (63 when every due keyframe is written), and a document that could
  exceed the limit is serialized and checked exactly.
- The bound is the parent's bound plus the stored patch's length plus 4 KiB.
  A patch carries every after-value it installs, serialized by the same
  `Serialize` implementations as the document and wrapped in more syntax
  (keys, `before`, `after`, nulls), so a patch grows the compact document by
  less than its own length; the slack covers a map field appearing for the
  first time. A Sequence stored as a [child splice](#compact-history-rows)
  carries its complete old node and every inserted child, and its document
  form grows by at most those children and their commas, so the argument is
  unchanged. A history row that also holds the inverse only loosens the
  bound. The randomized store test checks the bound against the actual
  serialization after every commit.
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

### Deferred keyframes

Serializing, storing and hashing a complete document is the most expensive
part of a commit on a large project (16.3 MB of compact JSON and 37–55 ms on
10,000 bound Holds), and it used to land on
whichever edit happened to be the 64th. Keyframe placement is a storage choice, not authored state, so it
no longer happens inside the edit:

- A commit whose depth would reach 64 stays elided (a patch, as before) and
  leaves the keyframe due. `ProjectStore::keyframe_due` reports it.
- `ProjectStore::write_due_keyframe` later converts the head's row in a
  separate write transaction: the document becomes the head's compact JSON,
  `depth` 0 and `json_bound` its length, exactly what a keyframe commit
  writes. It serializes the cached validated head before taking the write
  lock. A host calls it once its request queue is idle, after the edit is
  visible; a writable open calls it for a keyframe the previous writer left
  due. Nothing else changes: no revision content, history entry, navigation
  patch, cursor or redo row.
- Only the head is converted. Every new revision's parent is the head, so
  the head has no children and no row's metadata depends on its own; the
  `UPDATE` also requires that it is still elided and childless. Later
  commits extend the new keyframe's chain (`depth` 1, its length as the
  bound), so `metadata_holds` checks every row exactly as before.
- Until the keyframe lands (edits arrive without idle time, or the process
  ends first), depth keeps growing. At depth 128 (`MAX_PATCH_CHAIN`) the
  commit stores the keyframe itself, so the bound on reconstruction is 127
  patches instead of 63, and the open-time checks (`check_stored_sizes`,
  `metadata_holds`, `reconstruct`) use that bound.
- The history receipt hashes every row, including the keyframe document and
  its metadata. The commit that extends the receipt keeps the chain value
  before its revision in memory (`ReceiptLink`; a writable open takes it from
  the complete hash it just computed). When the receipt ends at the head,
  the conversion first checks that this value reproduces the stored chain
  over the old row, then writes the new row and the receipt's recomputed last
  link in the same transaction. Without such a proof the conversion is
  refused and the keyframe stays due. A receipt that ends earlier, or comes
  from another validator build, does not cover the head's row and is left
  unchanged; opening then replays as before.

Crash safety follows from the single transaction: before its commit the head
is the elided row of a due keyframe, with the old receipt; afterwards it is a
keyframe with the new receipt. Both are valid histories, and opening proves
either through the receipt without replay. `tests/failpoints.rs` aborts a
writer at both sides of that commit (`keyframe-before-commit`,
`keyframe-after-commit`) and checks receipt-proved read-only reopening, the
writable reopen's conversion, `validate_full` and a later commit.
`tests/revision_storage.rs` checks that writing each due keyframe reproduces
the previous placement exactly, that a host that never writes them stays
within depth 127 with every revision rebuilt and receipt-proved, that a
read-only open never writes and a writable one converts, and that a converted
keyframe is covered by the receipt (changing it fails opening). A unit test
refuses conversion with a missing or wrong receipt link without changing any
row. Physical power loss is not covered by these process kills.

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
durations its validation produced. Inside
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
| Sound clocks: record, entry, node, run and work limits; every table referenced; live owners, kinds, Repeat ancestry | Whole binding state and document | Recomputed |
| Marks | Marks, structure, durations | Recomputed |

Validating the whole tree structurally on each commit is deliberate. Local
rechecks of the tree would need maintained parent, depth and reachability
indexes; a wrapped or moved subtree changes the depth of everything below it,
and marks and sound-clock ancestry depend on durations and Repeat ancestors
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
| Final result validation | Repeated the structural pass | Takes that shared value when the shared post-processing (prune, basis lock) ran without sound, beat-sound or allowance restoration | Those steps change sound clocks and basis state only |
| Split command | Split validated its result, then the transaction validated it again | The transaction adopts Split's complete validation when pruning removed nothing, no new timing table exists and the basis did not lock | The result is then the Split result with a new revision identity, which validation does not read |
| Split's anchor index | Every parent, offset and Repeat layout | Built only when a bound occurrence mark lies in the target; otherwise the target's parent is found directly | The index is read only to relocate those marks; the input validated, so building it cannot fail |
| Layout admission | Built the JSON text to measure it | Counts bytes without retaining them, once per layout | Same length, same limit |
| Patch diff, lineage comparison | Collected the key union and looked every key up in both maps | One merged pass over both sorted maps; lineage ancestors use hash maps | Same comparisons in the same key order |
| Store inverse check | Copied the result, applied the inverse, compared whole documents | `DocumentPatch::restores`: the same guards and errors, then a field-by-field comparison of the would-be result | Equal to `apply_stored(from) == to`, checked on every random step |

Ordinary one-leaf semantic replay also retains the successful leaf
`EditTransaction` in its private `OneLeafReplay`. Only an actual eligible reuse
hit can lend it to final aggregation: exactly one ordinary `InsertTime`, no
inputs or isolation, local command work and no derived clocks. Every prefix admission, revision/identity check,
byte/work charge, visitor and final document admission keeps its original
order. After that hit, only the document's revision header
changes. The aggregate therefore copies the validated leaf patches, changes
`forward.to_revision`, `inverse.from_revision` and the description, and keeps
authored timing allocation IDs on the leaf revision.

Release Core skips the repeated final aggregate validation, diff, inverse and
changed-ID traversal. Debug builds recompute the original whole edit and
assert equality. Store still independently validates the actual result,
serializes the actual outer request and rebased edit, enforces size limits,
checks decode/readback equality and verifies inverse restoration. A miss uses
the original factory. The retained forward/inverse state adds live memory;
whole-app samples do not isolate its cost. Tests cover actual hit/fallback
selection, two outer revision lengths, preserved leaf allocations, visitor
refusal, scoped-witness unwind and real 64 MiB/512 MiB refusals.

One ordinary local/compact `InsertTime` replay without inputs or isolation
also reuses two exact local wire counts. The actual entry is fully counted;
its untouched literal clone can reuse that successful count. The actual
post-leaf value is fully counted too. After the visitor, only its top-level
revision header changes, so checked subtraction/addition of the actual
serialized revision tokens supplies its final successful count. Counts
never come from the optional replay capability, document equality or revision
identity. This works with either a real capability or `None`.

Token errors, arithmetic uncertainty and over-limit results use the original
full serializer and error mapping at the original final site. Every cumulative
charge retains its exact amount and position. Debug builds compare complete
full-serializer Results against each reused count. Other transaction shapes
and reference modes retain full counting. This adds only scalar counts and a
bounded revision-header clone, with no new document/JSON buffer or persistent
cache. Tests compare both Some/None paths against independently forced full
counting, including visitor/prefix failures,
short/equal/128-byte outer headers and the actual 64 MiB/512 MiB limits.

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
The oracle is compiled only for tests (`test-support` feature).
Debug builds also assert at each reuse that the shared or proved durations and
any adopted validation equal a complete recomputation.

The structural walk and each validation family still run once per commit over
the whole document.

### Pause work scoped to its changes

A pause on a 10,000-sibling root still repeated whole-document work that its
result does not need. Each replacement below is exact; debug builds assert
it against the complete computation at every use, and the random suites in
`command_work.rs`, `structural_properties.rs` and the store's commit and
rebuild suites run through them.

| Step | Before | Now | Why it is the same |
| --- | --- | --- | --- |
| Result structure | A complete structural pass for lineage, marks and the final validation | `structural_durations_after_root_leaf` adds the new Hold to the Split result's (or the head's) durations and repeats only the checks the new leaf and the root's sum could fail | Only the leaf and the root differ; every other node, asset, override and the basis were proved for the base |
| Identity lookups | Ordered-map lookups by identity in the structural pass, sound-clock validation, frozen projections, the lineage closure and plan compilation | Transient hashed indexes (FNV-1a from a per-process random offset, `id_hash`; keys are the validated document's own bounded identities, so a crafted collision set can only slow a pass), merge-joins of sorted owners against their nodes and proofs, and the root (always a Sequence) without a lookup | The same entries are found; the hashed maps are never iterated for output |
| Plan compile | Copied the head's durations and looked every child up twice | Shares the retained durations and one hashed index | Same plan |

`FrozenIndex` parents are hashed, and every node of an admitted layout but
the root has one, so `has_node` equals `nodes.contains_key` (asserted in
debug builds).

### Compact history rows

`command::patch_wire` stores transactions in an exact compact form:

- A node change whose before and after are Sequences that differ only in
  their children, with at least 32 children, stores `before` and
  `after_children: {at, remove, insert}` instead of `after`.
- A transaction whose inverse is exactly `forward.inverse()` omits it.

A splice has one encoding (no `after` key at all; an explicit null is
refused), and the writer decides the omission by comparing the inverse with
the forward patch field by field rather than building its reverse.
Decoding reconstructs the identical typed transaction, the store still
requires its stored text to decode to the computed transaction before it
commits, and rows in the complete form remain readable. A splice must lie
inside its old list and produce at most 100,000 children. On a
10,000-sibling root a pause's history row keeps one copy of the child list
instead of four.

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

Edits and Undo/Redo on an open writer check the receipt's currency instead
of hashing the history again (`generation_preparations::verify_for_writer`).
When the receipt ends at the head, covers every revision, and its validator,
profile, generation-scope (including preparation constraints) and
qualification dependencies still hold, the stored rows are the ones the
writer proved at opening or wrote itself after their replay checks, under
the writer lock; the transaction then checks only the preparation table
against the head. Otherwise it performs the complete receipt validation as
before. Hashing every row on every edit had grown to 10 to 15 ms per edit on
the 10,000-beat fixture once its 16 MB keyframe existed, growing with the
history. As with every other read of an open store, a history row changed
underneath an open writer by another program is detected by the next open or
`validate`; a changed receipt dependency is still refused by the next edit.

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
the SHA-256 of their canonical JSON. An adversarial run showed that the slot
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

`crates/deadpan-store/tests/revision_storage.rs` covers keyframe placement and
size bounds, exact reconstruction of every revision through edits, undo, redo
and reopen, randomized commits compared with complete validation and an
independent rebuild from storage, receipt extension, invalidation and tamper
detection, and bounded pause history entries. The derived placement is compared
with the old stored clocks in `crates/deadpan-audio/tests/derived_timing.rs`
(identical at 48 kHz, at most one sample at 30000/1001).

## Commit-path costs

Frozen layouts share one immutable allocation, so cloning or comparing a
document copies no retained table. Parsing a document no longer re-serializes
its admitted sound-clock state or layouts to recheck the input's byte bound. A
commit parses no document and serializes only its patch (keyframes excepted),
and validation does not rebuild admitted layouts' indexes.

### Shared collections and identities

Every edit derives several documents from the validated head (the Split
result, the insertion result), and the store cache, the
workspace and the plan keep further copies. On 10,000 beats copying and
freeing those maps cost about 7 ms of a pause.
Two representation changes remove most of it without changing any value:

- `nodes` and `audio_lineage` are `SharedMap`s (core `shared_map`): a `BTreeMap` behind an
  `Arc`. Cloning or dropping one adjusts a reference count; the first mutable
  access through a shared handle copies the map once (`Arc::make_mut`), so an
  edit still copies exactly the maps it changes. `Deref` exposes the inner
  map, and equality, iteration order and serialization are the inner map's
  (equality has no identity shortcut, so irreflexive values compare as
  before). The workspace's document copy and the plan share the head's maps.
- Identifiers (`NodeId`, `RevisionId` and the other `identifier!` types) hold
  `Arc<str>`. Cloning one is a reference count rather than an allocation;
  equality, hashing and the wire form are those of the text.
- Identifier ordering compares the bytes inline, eight at a time
  (`compare_identity`), instead of calling the platform `memcmp` for every
  comparison. The order is exactly the text's byte order (a unit test
  compares both over shared prefixes of every length), so every map keeps
  its iteration and serialization order. Ordered-map lookups by identity
  had spent about a quarter of a 10,000-beat pause in `memcmp`.

## Remaining costs

- Each commit still validates its result once over the whole document; see
  [command work reuse](#command-work-reuse).
- An edit still copies each large map it changes once (`nodes` for every
  structural edit), and a commit's
  durable WAL sync (`F_FULLFSYNC`) takes 3 to 5 ms.
- An edit's history row still carries the changed parent Sequence's complete
  old children list once ([compact history rows](#compact-history-rows)).
- Every 64th revision still needs a keyframe: the complete document is
  serialized, written and hashed after the edit, in its own transaction
  ([deferred keyframes](#deferred-keyframes)). It no longer delays that edit,
  but it holds the writer for about 47 ms, so a request that arrives
  meanwhile waits for it. The native project service writes it when its
  request queue is idle.
