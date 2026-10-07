# Supported development project formats

The current package uses SQLite schema 70 and core document schema 46. Schema
64 stores revision documents only at [keyframes](TIMING_STORAGE.md#revision-storage-database-schema-64),
records each revision's patch-chain depth and JSON size bound, and adds the
[verified history receipt](TIMING_STORAGE.md#verified-history-receipts).
Schema 65 adds the [register bank digest](TIMING_STORAGE.md#register-bank-digest-database-schema-65).
Schema 66 adds [analysis corrections](ANALYSIS_CORRECTIONS.md) and
[resumable shot scan progress](SHOT_DETECTION.md). Schema 67 adds
`retired_identities`, which keeps identities a [restore](BACKUPS.md#restore)
discarded from being issued again. Schema 68 adds
`generation_variant_retention` and `generation_retention_state`, the
[AI variant retention](STORAGE.md#retention-of-unaccepted-ai-variants)
records and clock watermark. Schema 69 adds explicit AI authoring scopes,
independent version clocks and audited isolation addresses. Schema 70 adds
[durable AI replacement preparations](AI_HOLDS.md), including exact claims,
retained controls and compact immutable history proofs.

Under the 2026-09-30 development-format authorization, this build refuses
schemas 1 through 69 as `UnsupportedSchema` without changes. The retained
66-to-67 and 67-to-68 migration steps do not form a complete chain to schema 70.
The [release migration runner](BACKUPS.md#release-migration-policy) remains
implemented and tested; this development build does not invent missing steps.
A schema *above* 70 is refused by writers as `NewerSchema` and opens read-only
for viewing.

History patches now record retained audio timing as granular
`AudioBindingPatch` entries rather than two complete binding states, and new
timing tables are sliced to the aliases their placements name. Both are
breaking changes to the history and document representation of new edits.
Stored literal fixtures that embedded complete binding states were converted. It refuses schemas 1 through 69 with the
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

## Per-attempt seeds in bridge receipts (2026-10-05)

Without a schema change, every attempt of a bridge generation request is a
seeded variant: attempt `n` must declare and record
`ProviderSelection::for_attempt(n)` (the request's seed plus `n - 1`, below
2^32). The store checks this when recording Ready, on acceptance and in its
whole-store validation on open. Attempt 1 is unchanged, so packages whose
Ready bundles are all first attempts are unaffected. A package whose request
has a Ready retry (attempt 2 or later) recorded under the request's own seed,
which the previous builds produced only by retrying a request, now fails
validation on open with an integrity error. No migration rewrites it; create
or regenerate the project. See [AI Holds](AI_HOLDS.md#library-chain).

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
for beat-owned sounds, addressed by owner and local sound ID. Database 70 stores
these documents and their reversible patches. Prior unused packages have no
supported migration, including the former schema-52 additive upgrade. Create a
current package to continue; refusal never rewrites
the old package or its media.

Calling `project migrate` on schema 70 performs read-only validation and reports
equal source/destination schemas with `backup: null`, including alongside a
native writer. On an older package it returns `SchemaUnsupported` before
creating a backup or obtaining a writer. An open native endpoint only
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
