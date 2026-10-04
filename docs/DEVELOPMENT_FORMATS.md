# Supported development project formats

The current package uses SQLite schema 62 and core document schema 46. This
development build opens schemas 59 through 62. Schema 60 only added the
[speech activity](SPEECH_ACTIVITY.md) table, schema 61 the
[shot analysis](SHOT_DETECTION.md) table and schema 62 only adds the
[remote-original provenance](YOUTUBE_IMPORT.md#provenance) table, so the first
writer of a schema-59, 60 or 61 package creates the missing empty tables and
sets the version in one immediate transaction, without a backup; a read-only
open of such a package sees no stored activity, shots or provenance. It refuses schemas 1 through 58 with the
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

Calling `project migrate` on schema 62 performs read-only validation and reports
equal source/destination schemas with `backup: null`, including alongside a
native writer. On schema 59, 60 or 61 it takes the writer, performs the additive
upgrade and reports `from_schema: 59` (or 60, 61), `to_schema: 62`, `backup: null`. Calling it on an older package returns `SchemaUnsupported`
before creating a backup or obtaining a writer. An open native endpoint only
validates its already admitted current package.

## Current recovery remains required

Current history validation still checks complete forward/inverse agreement,
unique revisions, undo/redo, abandoned branches, registers and Compound steps.
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
