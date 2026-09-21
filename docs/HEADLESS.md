# Headless editing foundation

The standalone CLI and `deadpan-app --headless` use the same implementation.
Neither headless entrypoint initializes a window or media worker. This is the
engineering API, not the finished editor's user interface.

```sh
cargo run --locked -p deadpan-cli -- project create /tmp/example.deadpan --fps 30000/1001 --size 1920x1080
cargo run --locked -p deadpan-cli -- project validate /tmp/example.deadpan
cargo run --locked -p deadpan-app -- --headless project dump /tmp/example.deadpan --json
```

Creation requires a new `.deadpan` directory path and an explicit presentation
basis. Automatic first-source adoption still belongs to the unimplemented import
workflow. Sources, analysis, and renderers cannot be inferred from this blank
project. The source package's `project.sqlite` is authoritative; `manifest.json`
contains discovery identity only. No loose JSON document is read as current state.

## Commands and dry runs

Copy the actual `project_id`, `revision_id`, and root node ID from the dump into
a request such as this. `new_revision` is optional; the host generates a UUID
when omitted. Caller-supplied revision IDs must never have been committed before.

```json
{
  "protocol": 1,
  "project_id": "PROJECT_ID_FROM_DUMP",
  "expected_revision": "REVISION_ID_FROM_DUMP",
  "command": {
    "command": "insert",
    "parent": "ROOT_NODE_ID_FROM_DUMP",
    "index": 0,
    "subtree": {
      "root": "pause-1",
      "nodes": {
        "pause-1": {
          "label": "The pause",
          "kind": {
            "type": "hold",
            "recipe": {
              "duration": 45,
              "video": {"type": "background"},
              "audio": {"type": "silence"}
            }
          }
        }
      }
    }
  }
}
```

Save the request outside the project database, then run:

```sh
cargo run --locked -p deadpan-cli -- command /tmp/example.deadpan --json /tmp/request.json --dry-run
cargo run --locked -p deadpan-cli -- command /tmp/example.deadpan --json /tmp/request.json
```

A dry run returns the validated forward/inverse changes, affected node IDs, and
duration delta with `committed: false`. It does not allocate a persistent revision
or change undo history. It checks the same reducer, serialized size limits, and
never-reused revision rule as commit. A stale expected
revision fails with `RevisionConflict` and the current revision, without writing.

Supported commands are `insert`, `delete`, `move`, `group`,
`ungroup`, `wrap_repeat`, `set_repeat`, `insert_plays`, `move_plays`, `set_hold_duration`, `set_hold_provider`,
`rename`, `add_asset`, `set_mark`, `delete_mark`, `set_play_override`, `clear_play_override`, and `edit_occurrence`. Their exact typed parameters are defined in
[`Command`](../crates/deadpan-core/src/command.rs). `set_repeat` changes an existing
Repeat; `wrap_repeat` deliberately adds nesting. A three-play repeat includes
three total plays and only two gaps. These are structural edits, not rendered
media. Editing through range/text selectors, registers, macros, and effects
remain required future work.

Documents use schema 5 and store compact Repeat `iterations.runs` with
`allocation`, `first`, and `count`. Commands `wrap_repeat` and `set_repeat` still
take a total `plays` count. `insert_plays` takes `node`, `index`, and `count`;
`move_plays` takes `node`, a half-open `start`/`end` play range, and `destination`
measured after removal. Surviving identities remain stable; inserted/grown plays
use the command's new revision. The run limit is 100,000 per Repeat, checked
without expanding plays. `InstancePath` identifies the target node and its exact
ordered Repeat ancestors. Sparse play overrides are described below.
Inserting a subtree remaps its Repeat plays to the actual insertion revision,
preserving their count. An imported initial document reserves its existing
allocation names, so later revisions cannot resurrect retired identities.

## Sparse play overrides

`set_play_override` replaces one play of an authored Repeat with an ordinary
editable subtree. It takes `node`, the stable `iteration` from the document, and
`subtree` using the same shape as `insert`. For example, the command inside a
current protocol/revision envelope can be:

```json
{
  "command": "set_play_override",
  "node": "repeat-1",
  "iteration": {"allocation": "REVISION_THAT_CREATED_THE_PLAY", "ordinal": 1},
  "subtree": {
    "root": "different-pause",
    "nodes": {
      "different-pause": {
        "label": "Only this play",
        "kind": {
          "type": "hold",
          "recipe": {
            "duration": 60,
            "video": {"type": "background"},
            "audio": {"type": "silence"}
          }
        }
      }
    },
    "overrides": {}
  }
}
```

The document stores `overrides[repeat_id]` as an array of `{iteration, root}`
entries. Each root has one structural owner. Its duration may differ from the
default child; later plays move by the exact difference, with gaps only between
plays. Nested overrides are supported through `subtree.overrides`. Insertion
reallocates nested Repeat identities and remaps those override keys by play
position. Other plays retain the default child and their original identities.

`clear_play_override` takes `node` and `iteration`, removes the owned subtree,
and restores that play's default child. It fails if no override exists. Edit an
existing override's nodes with the ordinary commands; replacing its entire
subtree requires fresh node IDs. Both commands support dry-run and durable
undo/redo. Reordering preserves overrides by identity. Shrinking retires removed
overrides into undo history; newly inserted or grown plays inherit none.

Occurrence paths must target the effective child of their selected play. A path
to the default child is invalid for an overridden play, and an override child
cannot be addressed through another play. Marks attached to replaced or removed
content follow their declared loss policy. Gaps follow their preceding stable
play even when its duration changes.

This command targets an authored Repeat and one of its plays. If that Repeat
itself occurs within another Repeat, the authored change applies in each outer
occurrence using it. Use `edit_occurrence` below to edit through one complete
nested occurrence path. Partial range operators and role-only overlays remain
required work.

## Edits to one nested occurrence

`edit_occurrence` resolves a complete `instance` against the expected revision,
isolates its repeated ancestors, and applies one node operation as a single
undoable edit. For each ancestor without an override for that play, it copies the
default child's authored subtree and installs an override for only that play.
Existing overrides are reused. Other plays keep their authored content. Nested
copies include existing override subtrees and retain compact iteration orders
under fresh authored node IDs, without expanding play counts or copying media.

For example, in a tree `outer -> inner -> hold`, where both Repeats have two
plays, this command changes only the second inner play of the second outer play:

```json
{
  "command": "edit_occurrence",
  "instance": {
    "node": "hold",
    "repeats": [
      {"node": "outer", "iteration": {"allocation": "OUTER_ALLOCATION", "ordinal": 1}},
      {"node": "inner", "iteration": {"allocation": "INNER_ALLOCATION", "ordinal": 1}}
    ]
  },
  "edit": {"type": "set_hold_duration", "duration": 60},
  "identities": {
    "nodes": ["copied-inner", "copied-default-hold", "selected-hold"],
    "marks": []
  }
}
```

Use the real allocation values from the document. The host supplies a bounded
pool of fresh, distinct `nodes` and `marks`; unused identities have no effect.
Node IDs are consumed in structural preorder for each copy, outside inward.
Mark IDs are consumed in current mark-ID order for each copy. The example needs
three node IDs and assumes no owned marks need copying. A subsequent edit through
the resulting override path needs no new IDs when all its ancestors are isolated.
Dry-run returns the same nodes, overrides, and mark changes as a commit with the
same request and revision. An invalid path, stale revision, exhausted identity
pool, invalid operation, or document-limit overflow commits nothing.

Supported `edit.type` values are `insert`, `delete`, `group`, `ungroup`,
`wrap_repeat`, `set_repeat`, `insert_plays`, `move_plays`, `set_hold_duration`,
`set_hold_provider`, `rename`, `set_play_override`, and `clear_play_override`.
They use the selected node as their implicit `node` or `parent`; other parameters
match the corresponding ordinary command. Child-index and play-ID parameters
remain local to that node. The ordinary operation's structural preconditions
still apply. This entrypoint does not yet implement range deletion/splitting,
multi-target moves, text selectors, or role-only operations.

Isolation preserves local time before the edit, including exact fractions under
Retime. Existing ancestor-local marks therefore follow the selected copied
content when the actual edit transforms them. Owned Local/Source marks copy with
fresh mark IDs; their external hosts and source clocks stay explicit. Concrete
occurrence marks follow their selected copies while retaining their mark IDs.
Sequence-pinned marks remain single events at fixed project coordinates.
Unresolved marks remain unresolved and retain their last coordinates.
Inheritance follows the owner: an externally owned Local mark referencing a
copied host remains on its original authored host. It is not implicitly copied.
See [nested occurrence verification](OCCURRENCE_VERIFICATION.md).

## Picture plan inspection

```sh
cargo run --locked -p deadpan-cli -- inspect-plan /tmp/example.deadpan
cargo run --locked -p deadpan-cli -- inspect-plan /tmp/example.deadpan --frame 10
```

The first command reports the immutable revision, authored node durations, and
index storage. The second resolves one project-frame center through Sequences,
Repeat plays/gaps, and nested Retimes. It returns an original media coordinate
or the authored Hold/blank/still provider, plus stable occurrence identity and
lookup work counters. Fractions use decimal numerator/denominator strings so
wide exact values survive JSON clients. Out-of-range frames fail explicitly.
These commands work read-only alongside a writer and do not decode media,
evaluate effects, mix audio, or render a file.

## Exact boundary selection

`resolve-selection <project.deadpan> --json <selection.json>` resolves a point or
nonempty range against one immutable revision without changing the project.
It works while a writer holds the package. For the 45-frame `pause-1` above:

```json
{
  "protocol": 1,
  "request": {
    "project_id": "PROJECT_ID_FROM_DUMP",
    "expected_revision": "REVISION_ID_FROM_DUMP",
    "role": "linked",
    "selector": {
      "type": "point",
      "target": {
        "boundary": {
          "coordinate": {
            "space": "local",
            "node": "pause-1",
            "position": {"numerator": "21", "denominator": "2"}
          },
          "bias": "right"
        }
      }
    }
  }
}
```

This returns the exact project boundary and its one ties-to-even quantization
(10.5 becomes frame 10 if this Hold starts at zero). A `range` selector replaces
`target` with `start` and `end` targets. Reversed, empty, or quantization-collapsed
ranges fail with `InvalidRange`; endpoints are never silently reordered.
`role` is the requested editing role (`linked`, `video`, or `audio`), independent
of which source coordinate identified the boundary. This query does not apply
a role edit or create an attachment.

The strict types are in [`anchor.rs`](../crates/deadpan-core/src/anchor.rs):

- `local` uses a host node and exact frame position. A repeated host requires
  an explicit `occurrence` with that node and its complete ordered Repeat path.
- `occurrence` stores that `instance` directly alongside `position` and accepts
  no additional scope.
- `sequence` uses a pinned integer `frame` and accepts no occurrence scope.
- `source` uses an asset plus an original video/audio timestamp or audio sample
  index with its explicit original sample rate. It requires a Source occurrence.
  Equivalent clocks convert exactly, signed origins remain intact, and positive
  mix-clock audio offsets delay the mapped audio boundary. A held picture has no
  unique reverse source boundary and is rejected here.

All positions must fit their host and every intervening Retime mapping. End
boundaries are legal, including zero in an empty project. Local anchors never
guess a play, even for a one-play Repeat. Missing or retired iterations fail.
The index stores authored parents and sequence prefixes; seeking billions of
plays does not expand them. Repeat identity lookup scans compact runs.

`set_mark` persists one of these boundaries, its owner, label, and explicit loss
policy. Named-mark selectors are described below. Range-editing operators and
temporal attachments remain to be implemented.
[Boundary verification](ANCHOR_VERIFICATION.md) preserves the original query
evidence; [mark verification](MARK_VERIFICATION.md) covers the schema-3 extension.

## Persistent marks

A mark command uses the same protocol/revision envelope as every other edit:

```json
{
  "command": "set_mark",
  "id": "a",
  "owner": "ROOT_NODE_ID_FROM_DUMP",
  "label": "After the answer",
  "boundary": {
    "coordinate": {
      "space": "local",
      "node": "pause-1",
      "position": {"numerator": "12", "denominator": "1"}
    },
    "bias": "right"
  },
  "loss_policy": "keep_unresolved"
}
```

`set_mark` creates or replaces the named mark, including deliberately rebinding
an unresolved mark. `delete_mark` takes `id`. The owner controls lifecycle and
is separate from the anchor coordinate. Labels accept Unicode; IDs use the
existing 1–128 byte ASCII identity rule. Documents permit up to 100,000 marks
within the shared 64 MiB JSON limit.

Local and occurrence marks follow retained content through edits, using exact
fractions. At an internal boundary, left bias follows preceding content and right
bias follows following content. At a host's outside leading/trailing edge, the
corresponding left/right bias stays with that edge. A mark inside moved content
follows it within the same host; moving its host carries the mark too. Repeat
marks follow stable play identities, and gaps belong to the preceding play.
Source coordinates stay in the original asset clock, independent of ownership
or whether a timeline occurrence currently uses that moment. Sequence-pinned
coordinates stay fixed through ripple edits.

When an owner, host, play, gap, or targeted content disappears, `delete_owned`
removes the mark or `keep_unresolved` preserves its original coordinate and a
typed reason. Unresolved marks never attach automatically to a replacement at
the same timestamp or ID. `wrap_repeat` accepts `anchor_policy: "first"` or
`"unresolved"`; omission defaults to `first` for compatibility. A concrete old
occurrence can follow the first new play, while an authored Local child mark
still applies to that child across its plays. New plays do not copy concrete
occurrence marks. Every mark change is part of the same reversible transaction
as its structural edit; dry-run exposes it in `edit.forward.marks`.

Use `resolve-selection` with these selector shapes and the current project,
revision, and media role:

```json
{"type": "mark", "target": {"id": "a"}}
```

```json
{"type": "mark_range", "start": {"id": "a"}, "end": {"id": "b"}}
```

Each named target may supply `occurrence` when its stored Local/Source coordinate
needs explicit scope. Missing and unresolved marks return `MarkMissing` and
`MarkUnresolved`; the retained coordinate is not used as a fallback.

## History and checkpoints

```sh
cargo run --locked -p deadpan-cli -- project undo /tmp/example.deadpan --expected CURRENT_REVISION
cargo run --locked -p deadpan-cli -- project undo /tmp/example.deadpan --expected CURRENT_REVISION --dry-run
cargo run --locked -p deadpan-cli -- project redo /tmp/example.deadpan --expected CURRENT_REVISION
cargo run --locked -p deadpan-cli -- project checkpoint /tmp/example.deadpan
```

Undo/redo require the current revision and commit a new, never-reused revision
identity. Undo restores authored content; it cannot revive a stale command's
authorization. Editing after undo clears the redo path while retaining older
immutable revisions and their history. Named branch selection is not implemented.

Both history commands accept `--dry-run`. They return `committed: false` with the
proposed `outcome`, using the same navigation and patch validation as the write.
The proposed revision is not reserved; the document, history, and redo stack stay
unchanged. Dry runs may run while another process owns the writable session.

`project validate` checks SQLite integrity and foreign keys, then replays all
retained revisions, including edits abandoned after undo. It verifies command and
patch correspondence, inverse patches, revision identities, current cursor, and
redo order. Stored document, request, and patch JSON is limited to 64 MiB each,
measured in bytes before SQLite returns the text. Stored identity fields are
bounded to 128 bytes and revision kinds to the defined values before extraction.
Validation reads a few documents
at a time plus numeric redo IDs; its work grows with retained history. It is an
integrity check, not an authenticity signature or a repair operation.

Only one writable `ProjectStore` may own a package. Read-only dumps, validation,
and dry runs can coexist. A second writer gets `ProjectAlreadyOpen`; routing a
CLI request to an already-open GUI through a host socket is still outstanding.

A checkpoint is a consistent SQLite backup including committed WAL data, stored
under `Snapshots/`. It is not a portable project copy: the media directories are
not duplicated. Full recovery UI and portable media ownership remain open.
Unsupported database schema versions are refused without rewriting them;
future-schema read-only inspection still needs a compatibility implementation.

## Schema migration

Database schemas 1 through 7 return `MigrationRequired` when opened. Upgrade explicitly:

```sh
cargo run --locked -p deadpan-cli -- project migrate /tmp/example.deadpan
```

Migration holds the project writer lock, keeps a consistent SQLite backup under
`Snapshots/before-schema-8-*.sqlite`, and upgrades a separate candidate. It
replays all commands, undo/redo revisions, and abandoned branches with their
original revision IDs. Every snapshot and forward/inverse transaction is checked
against its strict original schema meaning. Migration goes directly to database
schema 8 and core document schema 5. Schema-7 histories already use core schema 5
and are validated without rewriting. Schemas 1 through 3 gain
empty override maps. Schema-1/2 histories also gain empty mark
maps; schema-3 mark histories retain their exact ownership, bias, and loss states.
Database schema-4/5/6 authored snapshots, commands, and patches are replayed through
the frozen core schema-4 adapter. Schema-5/6 generation requests and clocks are
validated and preserved; older databases gain empty request tables. Schema-6
attempt, candidate-receipt, and selection rows remain unchanged; older databases
gain empty tables. Migrated requests have no bridge plan and remain legacy;
the new bundle receipt table starts empty. Fields or
commands that did not exist in the old schema are rejected. SQLite atomically promotes the validated
candidate through its backup API; the main file is never renamed around a live
WAL. Existing read transactions keep their old snapshot. Failure before promotion
leaves authored contents unchanged. A current-schema project is validated with
no new backup. The result reports source/destination schemas and backup path.
Failures after backup creation include `error.recovery_backup`. Semantic migration
failures use `MigrationFailed`; disk, permission, and lock failures keep their
actionable storage error codes and still identify the retained backup.

Generated Hold acceptance/reversion semantics exist in core schema 5, but generic
project commands and initial import reject newly introduced generated artifacts
with `GeneratedAcceptanceUnavailable`. Qualified candidate admission is still
unimplemented; see [generated Hold semantics](GENERATED_HOLDS.md).

This is a database migration, not portable media copying or a recovery UI. The
immutable source specification and media references are unaffected.

Generation request storage currently has a Rust host API, described in
[generation request storage](GENERATION_REQUESTS.md). There is no CLI generation
command yet. A plain CLI edit/undo/redo fails with `GenerationRelevanceRequired`
when current requests require host context reconciliation; it cannot silently
bypass that transaction boundary. Dry-run remains read-only.

Machine-readable failures go to stderr with a schema version, stable error code,
and explanation; the process exits nonzero. A failed storage write never reports
a committed revision. Tests cover SQLite page exhaustion (`DiskFull`), rollback
after an interrupted history write, process exit inside an uncommitted database
transaction, and cross-process writer rejection.
