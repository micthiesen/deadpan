# Named takes

`:takes` opens saved versions of the project's edit. Enter a name and use
**Save new take** to keep the current committed revision. Select a take to
**Open selected take**, **Update to current edit**, **Rename selected take**,
or **Delete selected take**.
Names are unique, case sensitive, trimmed, single-line Unicode text of at most 128 UTF-8 bytes.
A project can have up to 256 takes. The panel uses native text editing and
Tab/Shift-Tab navigation, with takes sorted by name; Escape closes it and
returns focus to the workspace.

Opening a take restores its authored edit as one new reversible transaction.
Undo returns to the edit that was open before it; Redo opens the take again.
The saved take continues to name its original immutable revision until it is
explicitly updated. Editing after opening one does not silently advance its
saved revision. Renaming, updating and deleting labels do not create an edit,
clear Redo, move cursors or remove retained history and media.

Takes retain the complete authored document, including structure, framing,
audio, marks, targets and accepted generated providers. They do not replace
the project's immutable Original identity, current registers, analysis
corrections, media locations or operational jobs. Restoring a take does not
restart old automatic generation work. Accepted pictures remain accepted
without the model. A take is not a database backup or a portable copy; those
remain separate [recovery](BACKUPS.md) and [storage](STORAGE.md) operations.

The native service refuses take changes during temporary editing previews.
Commit or cancel the preview first. Read-only projects in the supported format
allow inspection but refuse mutations. Takes in an unsupported newer format
must be opened with that newer Deadpan. Every mutation captures the project, edit revision and
catalog version; actions on an existing take also capture its identity and
saved revision. A delayed request cannot operate on a newer label or snapshot.

## Headless interface

```sh
deadpan-cli project takes /absolute/project.deadpan
deadpan-cli project take /absolute/project.deadpan --json /tmp/take.json --dry-run
deadpan-cli project take /absolute/project.deadpan --json /tmp/take.json
```

Listing returns `{ "protocol": 1, "takes": ... }`. The catalog contains
`project_id`, the observed `revision_id`, `version` and `entries`, each with
`id`, `name` and `revision_id`. Use those observed identities in a request:

```json
{
  "protocol": 1,
  "request": {
    "project_id": "PROJECT",
    "expected_revision": "CURRENT_REVISION",
    "expected_version": 0,
    "action": {
      "action": "create",
      "id": "NEW_UNIQUE_TAKE_ID",
      "name": "First cut"
    }
  }
}
```

The other actions are:

| Action | Fields in `action` |
| --- | --- |
| `update` | `id`, `expected_snapshot` |
| `rename` | `id`, `expected_snapshot`, `name` |
| `delete` | `id`, `expected_snapshot` |
| `restore` | `id`, `expected_snapshot`, `new_revision` |

`expected_snapshot` is the entry's saved `revision_id`, which may differ from
the current edit revision. Take identities and new edit revision identities
must never be reused. Deleting a label does not release its identity.

Results contain `committed` and `outcome`. The outcome includes the resulting
catalog, `changed`, and a `commit` receipt only for a restore. Dry runs use the
same validation and return the proposed outcome without writes or identity
reservation. Mutations route through an open app's authenticated writer; closed
projects use the same store transaction. A committed revision survives a later
workspace refresh failure in the reply, so clients must not retry it blindly.

## Persistence

SQLite schema 75 stores the versioned catalog and immutable restore proofs.
The restore proof names the retained revision, independently of the take's
mutable label. Full history replay checks the captured document against that
revision and validates the ordinary forward and inverse edit patches. Renaming,
updating or deleting a label cannot invalidate an earlier restore's history.
Checkpoints, verified backups and portable copies carry the catalog and proofs.
The Original baseline and qualified-asset checks still apply to a restored edit.
