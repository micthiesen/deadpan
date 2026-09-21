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

Supported node-targeted commands are `insert`, `delete`, `move`, `group`,
`ungroup`, `wrap_repeat`, `set_repeat`, `set_hold_duration`, `set_hold_provider`,
`rename`, and `add_asset`. Their exact typed parameters are defined in
[`Command`](../crates/deadpan-core/src/command.rs). `set_repeat` changes an existing
Repeat; `wrap_repeat` deliberately adds nesting. A three-play repeat includes
three total plays and only two gaps. These are structural edits, not rendered
media. Text/range selectors, registers, macros, per-play overrides, and effects
remain required future work.

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
not duplicated. Full recovery/migration UI and portable media ownership remain
open. Unsupported database schema versions are refused without rewriting them;
future-schema read-only inspection still needs a compatibility implementation.

Machine-readable failures go to stderr with a schema version, stable error code,
and explanation; the process exits nonzero. A failed storage write never reports
a committed revision. Tests cover SQLite page exhaustion (`DiskFull`), rollback
after an interrupted history write, process exit inside an uncommitted database
transaction, and cross-process writer rejection.
