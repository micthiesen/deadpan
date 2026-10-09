# Durable measured source registration

The headless host can register a retained original as a qualified asset, or
register and insert its full selected streams in one undoable edit. Registration
uses [automatic presentation policy](PRESENTATION_BASIS.md): the first primary
picture can establish a provisional basis; timed or explicit projects keep their
clock. The native [one-Original workflow](SINGLE_ORIGINAL.md) uses this same
admission boundary. The complete format matrix remains open. See [measured timing](SOURCE_IMPORT_TIMING.md) for the exact mapping policy.

## Explicit speaker interpretation

A plain PCM WAV declares channels but no speaker layout, and audio preparation
never guesses speakers from the channel count. Registration therefore asks the
person: `AudioLayoutInterpretation::Mono` (one channel as front center) or
`StereoLeftRight` (two channels as front left, then front right). Each names
one exact native layout; no choice exists for other unlabelled channel counts.

`DecodedSourceQualification::for_registration` is the policy entry point for
the native import worker, closed-project CLI and live-project preparation. An
audio-only registration (a sound) whose stream declares no layout is refused
without a matching choice, with `AudioLayoutInterpretationRequired` and a message
naming the applicable choice; a choice for another channel count is refused the
same way. Nothing is retained, registered or added to history. A declared native
layout is used as is and a choice cannot override it, so none is stored there.
An Original video keeps its previous audio admission when no choice is given.

The snapshot stores the choice as `audio_interpretation`, beside the unchanged
measured audio index. It is omitted for declared layouts, so earlier receipts
keep their canonical bytes and identity; when present it is part of the receipt
hash. Loading rejects a choice for a declared layout, a different channel count,
an unknown value or an explicit null. `from_sessions_interpreted` reproduces an
existing receipt, including its choice, when the native app requalifies a
source for insertion or paste.

The CLI states the choice in the stream selection:
`{"type":"audio_only","stream":0,"interpretation":"stereo_left_right"}` (or
`"mono"`). The app's Add sound flow offers it as **Unlabelled channels** under
Sound import options (`Not chosen`, `Mono, 1 ch`, `Stereo L/R, 2 ch`); `Not
chosen` refuses such a file and the rail reveals the guidance.

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

Database schema 19 stores immutable receipts in `source_qualifications`, separate
from authored undo history. Core schema 13 assets bind `source_qualification` to
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

Native Original creation, retry, relink and bookmark resolution, sound admission,
editing, playback and Render now use these retained qualifications. Managed and
linked Originals have [end-to-end evidence](qualification/linked-original-2026-10-09.md).
[Integral](qualification/clean-aperture-2026-10-09.md) and
[fractional MP4 clean apertures](qualification/fractional-aperture-2026-10-09.md)
preserve exact source clocks. Receipts retain fractional bounds separately from
the decoded backing dimensions, validate their containment and bind them on
retained-source reopening. The complete source-format/creative-operation matrix
remains open. V1 excludes additional still-image/video imports. Current
product completion is tracked in [Requirements](REQUIREMENTS.md).
