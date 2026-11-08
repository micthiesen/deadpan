# Named Original moments

Specification §9.3: the Original keeps named moments on its one source clock,
filterable by editorial tags. Browsing a moment never alters the Original or
inserts it.

## Model

A moment is `{ id, name, tags, start, end }` on the project's single Original:

- `id`: a stable 1–128 character ASCII identity (letters, digits, `-`, `_`),
  reserved permanently. A deleted moment's ID is never reused; a project can
  use at most 65,536 IDs, so a create/delete loop cannot grow the digested
  reservations without bound.
- `name`: 1–128 UTF-8 bytes, no surrounding whitespace, line breaks or control
  characters; unique within the project.
- `tags`: at most 16 tags of 1–32 UTF-8 bytes each, lowercase, without
  whitespace, commas or control characters, deduplicated and sorted. Typed
  input is lowercased first (`Reaction` becomes `reaction`); the stored and
  wire forms are strict. Tags are whatever the editor types; nothing is
  inferred.
- `[start, end)`: half-open Original picture ordinals, `start < end`.

A project holds at most 1,024 moments.

## Storage

`deadpan-store::moments` keeps the catalog in its own SQLite tables
(`moment_state`, `moment_ids`, `moments`; database schema 88), outside
authored history like [named takes](TAKES.md): no catalog write creates a
revision, touches Undo/Redo or moves a cursor. The catalog has a version that
every write must name (`expected_version`) and a SHA-256 digest over the
version, entries and every reserved ID, checked on each read, so tampering or a
partial write is reported rather than shown. Opening, validation and
checkpoints check sizes and the digest. Backup restore carries reserved IDs
forward and advances the version so an old request cannot apply.

`Create` admits a range only when the project's single Original is ready, and
only within its measured picture count (read from the stored qualification
receipt). Generic projects list an empty catalog and refuse new moments.
`Rename`, `Retag` and `Delete` name the moment by ID. Unchanged values commit
nothing and keep the version.

## Commands

| Native | Headless |
|---|---|
| `:moment NAME [tags=reaction,breath]` names the selected Original moment (Original only) | `project moment <p> --json <request> [--dry-run]` with action `create` |
| `:moment-rename NAME` | action `rename` |
| `:moment-tags a,b`, `:moment-tags none` | action `retag` |
| `:moment-delete` | action `delete` |
| `:moments [filter]`, the MOMENTS list | `project moments <p>` |

The request envelope is `{"protocol":1,"request":{"project_id":…,
"expected_version":N,"action":{"action":"create","id":…,"name":…,"tags":[…],
"start":S,"end":E}}}`. Listing returns `{"protocol":1,"moments":…}`. A project
the app has open is reached through its [live endpoint](LIVE_PROJECT.md)
(`MomentCatalog`, `Moment`), and the app's list refreshes.

## Native UI

The Original rail shows MOMENTS under REUSE: a filter field and each moment's
name, measured duration and tags, in Original order. Filter words match a name
or tag as a case-insensitive subsequence; `tag:reaction` keeps only moments
with exactly that tag; every word must match. `:moments [filter]` focuses the
filter. While it has focus, Up/Down choose a moment, Enter selects it in the
Original (finished selection `[start, end)`, cursor at `start`, Original view)
and Escape returns to the editor. Clicking a row does the same. Selecting
never inserts or edits; `y` copies the selection as any Original moment.
Rename, retag and delete act on the moment chosen in the list, captured when
command entry opens. Failures appear in the status line; nothing is written.

## Evidence

- Store: `moments::tests::*` (validation, versioning, never-reused IDs,
  duplicate names, read-only refusal, digest tampering, backup carry-forward)
  and `tests/moments.rs` (admission against a real measured Original, reopen).
- CLI: `crates/deadpan-cli/tests/moments.rs` (dry run, stale version, range
  refusal, live writer).
- App: `project::tests::moments`, `project::tests::headless::moments`, parser
  and filter unit tests, and the `moments` replay (save with tags, duplicate and
  no-selection refusals, tag filter, keyboard selection in Original with an
  unchanged edit revision, rename, retag, delete, catalog reload).

## Limits

- The first twelve listed moments show their first Original picture as a
  thumbnail, through the same bounded 48-picture card cache as the beats;
  later rows show none until the filter narrows them.
- Moments name picture ordinals only; their audio follows the ordinary
  Original moment derivation when copied.
- Macros do not record moment commands, and moments cannot be pasted by name;
  select one, then copy it.
