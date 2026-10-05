# Supported development project formats

The current package uses SQLite schema 65 and core document schema 46. Schema
64 stores revision documents only at [keyframes](TIMING_STORAGE.md#revision-storage-database-schema-64),
records each revision's patch-chain depth and JSON size bound, and adds the
[verified history receipt](TIMING_STORAGE.md#verified-history-receipts).
Schema 65 adds the [register bank digest](TIMING_STORAGE.md#register-bank-digest-database-schema-65).
Under the 2026-09-30 development-format authorization there is no migration:
this build refuses schemas 1 through 64, including the former schema-59
through 62 additive upgrades, as `UnsupportedSchema` without changes.

History patches now record retained audio timing as granular
`AudioBindingPatch` entries rather than two complete binding states, and new
timing tables are sliced to the aliases their placements name. Both are
breaking changes to the history and document representation of new edits.
Stored literal fixtures that embedded complete binding states were converted. It refuses schemas 1 through 63 with the
store's `UnsupportedSchema` error (`SchemaUnsupported` over the CLI) before obtaining
a writer lock, creating a backup, enabling WAL, repairing directories or parsing
authored documents. The old package remains intact. Create a current project to
continue using this build.

The user confirmed on 2026-09-30 that Deadpan has no users during the current
goal and session, and authorized breaking development formats without
migrations when that simplifies implementation. Removing 32 obsolete document
adapters avoids maintaining their closed command vocabulary with each new
command. The adapters alone occupied 31,091 lines before this change. Build
speed improvements have not been isolated or measured.

## Additive vocabulary in core schema 46 (2026-10-05)

Core schema 46 also admits three additive, closed fields without a version
bump; documents without them serialize exactly as before, and older builds
refuse documents that use them (unknown field or variant):

- `PitchPolicy::Shift { semitones }` (`{"shift":{"semitones":3}}`): nonzero,
  within ±24, never on a Partition ([pitch shift](RETIME_EDITING.md#pitch-shift)).
- `AudioTreatments.saturation` with a `saturation` entry in `order`
  ([saturation](AUDIO_GAIN.md#saturation)).
- `Cutaway.removed`, a video-only delete ([role edits](ROLE_EDITS.md)).

Frozen audio contexts are schema 8: a context of schema 7 or older refuses a
pitch shift or saturation, and the frozen layout validates a shift's range and
purpose and its streaming preflight admits the shift object shape.

## Current validation and prior-format refusal

```sh
cargo run --locked -p deadpan-cli -- project migrate /tmp/example.deadpan
```

Core schema 46 retains scoped historical aliases in chronological sample clocks
for beat-owned sounds, addressed by owner and local sound ID. Database 58 stores
these documents and their reversible patches. Prior unused packages have no
supported migration, including the former schema-52 additive upgrade. Create a
current package to continue; refusal never rewrites
the old package or its media.

Calling `project migrate` on schema 65 performs read-only validation and reports
equal source/destination schemas with `backup: null`, including alongside a
native writer. Calling it on an older package returns `SchemaUnsupported`
before creating a backup or obtaining a writer. An open native endpoint only
validates its already admitted current package.

## Current recovery remains required

Current history validation still checks complete forward/inverse agreement,
unique revisions, undo/redo, abandoned branches, registers and Compound steps.
Opening recomputes only the revisions after a receipt from the same validator
build, after hashing every stored history row; `project validate` and
`project migrate` recompute all of them.
Writer ownership, consistent checkpoints, generation attempts, accepted-media
admission, original relinking and publication recovery retain their existing
boundaries. Old frozen audio-context codecs still used by current documents
remain supported. A module's historical name alone is not grounds for deletion.

Generic and multi-video projects in the supported format remain available in
their explicit compatibility workspace. Refusing an obsolete database format
does not remove generic editing capability or delete any package or media.

Historical qualification reports describe the behavior of their recorded
revision. They do not grant current support to old formats. Eventual release
migration and recovery acceptance remain required by the specification.
