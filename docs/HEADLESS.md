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
`rename`, `add_asset`, `set_mark`, and `delete_mark`. Their exact typed parameters are defined in
[`Command`](../crates/deadpan-core/src/command.rs). `set_repeat` changes an existing
Repeat; `wrap_repeat` deliberately adds nesting. A three-play repeat includes
three total plays and only two gaps. These are structural edits, not rendered
media. Editing through range/text selectors, registers, macros, per-play overrides, and effects
remain required future work.

Documents use schema 3 and store compact Repeat `iterations.runs` with
`allocation`, `first`, and `count`. Commands `wrap_repeat` and `set_repeat` still
take a total `plays` count. `insert_plays` takes `node`, `index`, and `count`;
`move_plays` takes `node`, a half-open `start`/`end` play range, and `destination`
measured after removal. Surviving identities remain stable; inserted/grown plays
use the command's new revision. The run limit is 100,000 per Repeat, checked
without expanding plays. `InstancePath` identifies the target node and its exact
ordered Repeat ancestors. Sparse occurrence overrides remain open.
Inserting a subtree remaps its Repeat plays to the actual insertion revision,
preserving their count. An imported initial document reserves its existing
allocation names, so later revisions cannot resurrect retired identities.

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
policy. Named-mark selectors are described below. Range-editing operators,
temporal attachments, and sparse occurrence overrides remain to be implemented.
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

Schema-1 and schema-2 projects return `MigrationRequired` when opened. Upgrade explicitly:

```sh
cargo run --locked -p deadpan-cli -- project migrate /tmp/example.deadpan
```

Migration holds the project writer lock, keeps a consistent SQLite backup under
`Snapshots/before-schema-3-*.sqlite`, and rewrites a separate candidate. It
replays all commands, undo/redo revisions, and abandoned branches with their
original revision IDs. Every snapshot and forward/inverse transaction is checked
against its strict original schema meaning. Migration goes directly to schema 3
and introduces an empty mark map in each old snapshot and patch. Fields or
commands that did not exist in the old schema are rejected. SQLite atomically promotes the validated
candidate through its backup API; the main file is never renamed around a live
WAL. Existing read transactions keep their old snapshot. Failure before promotion
leaves authored contents unchanged. A current-schema project is validated with
no new backup. The result reports source/destination schemas and backup path.
Failures after backup creation include `error.recovery_backup`. Semantic migration
failures use `MigrationFailed`; disk, permission, and lock failures keep their
actionable storage error codes and still identify the retained backup.

This is a database migration, not portable media copying or a recovery UI. The
immutable source specification and media references are unaffected.

Machine-readable failures go to stderr with a schema version, stable error code,
and explanation; the process exits nonzero. A failed storage write never reports
a committed revision. Tests cover SQLite page exhaustion (`DiskFull`), rollback
after an interrupted history write, process exit inside an uncommitted database
transaction, and cross-process writer rejection.
