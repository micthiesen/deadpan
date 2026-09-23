# Durable measured source registration

The headless host can register a retained original as a qualified asset, or
register and insert its full selected streams in one undoable edit. Registration
uses [automatic presentation policy](PRESENTATION_BASIS.md): the first primary
picture can establish a provisional basis; timed or explicit projects keep their
clock. Native import UI and the complete format matrix remain open. See [measured timing](SOURCE_IMPORT_TIMING.md) for the exact mapping policy.

## Admission and evidence

Retaining bytes is an operational step, separate from authored registration.
`DecodedSourceQualification::from_sessions` takes actual video/audio sessions
over matching verified source bytes. The host explicitly selects video alone,
audio alone, or both. Failed selected audio never becomes an implicit video-only
asset. The token has private fields and cannot be deserialized from cached JSON.

The snapshot retains the selected stream identities, exact common origin,
complete original-PTS picture index, original-sample audio index, terminal and
skip/discard evidence, and source interpretation metadata. Canonical picture
indexes use the internal asset alias `qualified-source`. Output presentation
policy does not replace source color metadata. Unknown audio priming is retained.
The current contract is `ffmpeg-8.0.3/source-decoded-v1`, snapshot version 1 and
timing policy version 1. Unsupported versions and fields fail explicitly.

`ProjectStore::register_source` checks the expected revision and obtains a fresh
verified snapshot of the retained original before admission. It binds the live
token's SHA-256 and byte length to that original's BLAKE3 ownership record.
The receipt identity hashes a versioned domain, original BLAKE3 and byte length,
and canonical qualification bytes. Asset labels, file locations, location
versions and temporary decode aliases do not change that identity.

For a native worker, [import preparation](IMPORT_PREPARATION.md) provides
`PreparedSourceRegistration::from_decoded` over a private verified original
snapshot. Canonicalization and receipt hashing run without the project writer.
`preview_prepared_source_registration` and `register_prepared_source` resolve
current revision/target/basis intent and recheck the retained original's namespace
and inventory version. Tokens cannot cross open project sessions. Synchronous
registration delegates to this path; independent read-only preview stays available.

Database schema 16 stores immutable receipts in `source_qualifications`, separate
from authored undo history. Core schema 11 assets bind `source_qualification` to
the receipt ID. The store commits a new receipt, derived asset, optional Source,
mark transforms, history and generation relevance in one SQLite transaction.
A later database failure preserves the already retained original and commits no
authored edit. An identical currently registered qualification reuses its asset;
registration without insertion then returns no edit or new revision.

Generic commands and initial snapshot creation cannot introduce qualified assets.
The dedicated host boundary supplies one exact asset admission to the shared
command reducer. `ImportSource` checks the source's roles against that asset and
uses ordinary insertion validation and inverse patches. Current generation
requests still require real host relevance resolution; preview exposes the
proposed edit so the host can resolve it before commit.

## Historical lookup and validation

Undo removes authored registration/insertion while retaining the receipt.
`registered_source(revision, asset)` and `source_video_index(revision, asset)`
resolve against the selected immutable revision. Reusing an asset ID after undo
cannot change an abandoned branch's source interpretation. Picture lookup
rebinds the canonical index to the requested historical alias without changing
its original timestamps or presentation identities.

Reopening validates every receipt, its original ownership binding, and all
qualified assets in all retained revisions, including abandoned branches.
Cached metadata is evidence, not proof of current file availability. An offline
linked original does not erase history; decoding still requires verified bytes.
Fresh admission fails when the original is missing or changed.

Each serialized index is bounded to 128 MiB; a combined qualification is bounded
to 192 MiB, with at most 100,000 receipts. SQLite checks field sizes before
returning payloads. Validation processes one receipt at a time and retains only
compact asset metadata while checking history. These are defensive limits, not
measured large-project capacity or performance claims.

Schema 1 through 15 migration replays complete history using frozen core wires.
Schema 15 uses core 10 and retains its presentation policy.
Schema 14 uses core 9 and retains its source qualifications. Schema 13 uses core 8,
including signed independent stream placements. Projects before schema 14 gain
an empty qualification table; their assets remain unqualified.
No receipt is inferred from an old asset hash or span. Unexpected modern tables
in old schemas are rejected before promotion and the backup is retained.

## Headless use and remaining work

[Headless commands](HEADLESS.md#measured-source-registration) documents the
versioned, explicit stream-selection request. The host rejects unsupported
protocols before opening the project or media. Tests use real retained CFR, VFR, offset A/V
and audio-only media, plus historical migration fixtures. [Qualification and
review evidence](qualification/source-registration-2026-09-21.md) records results.

The app still previews individual sources without editing project documents.
Native import/retry/relink controls, visible canvas previews, qualification of
existing legacy assets, still-image import, bookmark resolution, full codec/color
support, audio playback and export remain required work. No GUI or release
readiness is implied by this developer API.
