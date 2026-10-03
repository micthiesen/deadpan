# Supported development project formats

The current package uses SQLite schema 55 and core document schema 43. This
development build opens schema 55 and can explicitly upgrade schema 52. It
refuses schemas 1 through 51, 53 and 54 with the store's `UnsupportedSchema`
error (`SchemaUnsupported` over the CLI) before obtaining
a writer lock, creating a backup, enabling WAL, repairing directories or parsing
authored documents. The old package remains intact. Create a current project to
continue using this build.

The user confirmed on 2026-09-30 that Deadpan has no users during the current
goal and session, and authorized breaking development formats without
migrations when that simplifies implementation. Removing 32 obsolete document
adapters avoids maintaining their closed command vocabulary with each new
command. The adapters alone occupied 31,091 lines before this change. Build
speed improvements have not been isolated or measured.

## Retained schema-52 upgrade

```sh
cargo run --locked -p deadpan-cli -- project migrate /tmp/example.deadpan
```

Schema 52 already stores current authored documents. Its upgrade adds empty
register and compound-step tables. It preserves original authored and
operational rows and rejects preexisting Compound commands. Migration takes the
writer lock, retains a consistent SQLite backup under
`Snapshots/before-schema-55-*.sqlite`, validates an isolated candidate and
promotes it through SQLite's backup API. It never renames a main database around
a live WAL. Failure before promotion preserves the old authored state; failures
after backup creation identify the retained backup.

Opening schema 52 through ordinary headless access returns `MigrationRequired`.
Native writable Open uses the same backed-up upgrade on the project service.
Calling `project migrate` on schema 55 performs read-only validation and reports
equal source/destination schemas with `backup: null`, including alongside a
native writer. An open native endpoint does not migrate an older package.

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
