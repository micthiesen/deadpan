# Local Original and sound acceptance, 2026-10-11

DP-15 is Complete under specification §29.1's reference-Mac scope. This audit
maps §15.1 and §20.3 to implementation and verification, including the sound
repair completed immediately before it. Other requirements and delivery gates
remain open. Implementation review and this audit were performed by the sole
working agent; no independent reviewer or live native window was used here.

## Requirements audit

| Requirement | Implementation and evidence |
| --- | --- |
| One immutable Original; complete container and measured picture/original sound; atomic full-source baseline | [Single-Original workflow](../SINGLE_ORIGINAL.md), `initialize_prepared_source`, the authoritative Ready profile and [linked creation qualification](linked-original-2026-10-09.md). Store tests verify full-source initialization, Undo floor, reopen, receipt tampering, transaction rollback and refusal of a second picture even after deleting every beat. |
| Import progress, independent analysis/proxy state, retry and verified promotion | [Background preparation](../IMPORT_PREPARATION.md) retains, qualifies and registers off the writer through bounded workers. Native progress separates retained bytes, media qualification and source registration; analysis and proxies have separate lifecycles. Store tests verify content checks, cancellation, retained publication after database failure, deduplication on retry, session binding and changed-source refusal. A retained object alone never establishes a Ready baseline. |
| Initial picker cancellation creates nothing; interrupted preparation remains explicitly recoverable | [Native linked creation](linked-original-2026-10-09.md) verifies picker cancellation and Documents creation. App service tests `interrupted_initialization_is_recoverable_and_never_overlaps_after_project_switch` and `failed_original_qualification_reopens_as_an_incomplete_project_for_retry` exercise actual qualification and recovery. Closed headless creation publishes its explicit destination only once Ready. |
| Later imports are audio-only, with explicit stream selection; no added time or replacement of original sound | Native Add sound captures its stream, ownership and speaker interpretation. Sound import options exposes Automatic or an explicit container index. The service test `sounds_are_audio_only_catalog_entries_and_bad_streams_leave_the_edit_intact` qualifies actual WAV and MP4 audio. This audit additionally registers MP4 stream 1, refuses stream 0 as audio, compares all authored nodes/basis and verifies the unchanged emitted picture and mix. |
| Supported audio files; truthful profile failures | Qualified [WAV](pcm-sources-2026-10-09.md), [MP3](mp3-sources-2026-10-09.md), [Opus](opus-sources-2026-10-09.md) and AAC paths retain measured clocks and actual release/packaged export evidence. Unlabelled WAV requires explicit speaker interpretation. Unsupported streams/layouts fail before authored registration; format labels or picker extensions are not admission proof. |
| Deduplication uses complete content and qualified stream identity | Immutable content and qualification hashes, historical receipt binding and store registration tests. The current release reimport supplies a different proposed asset ID and label but reuses the registered sound without a commit; the complete dump remains identical. The same container's picture-plus-audio and audio-only qualifications remain distinct. |
| Managed default with APFS clone or verified copy; explicit linked path/bookmark | [Original ownership](../ORIGINAL_MEDIA.md), [linked creation](linked-original-2026-10-09.md), actual clone/copy evidence and current original-media/preparation tests. Native New and Add sound expose separate captured ownership choices; whole-container hashes protect both. |
| Moves and repairs verify identity; filenames cannot replace content | Bookmark tests verify moved/renamed files and refuse changed bytes. [Cross-volume relinking](relink-volumes-2026-10-08.md) exercises detached private APFS volumes. [Catalog sound recovery](sound-recovery-2026-10-11.md) verifies managed damage repair, linked moves, wrong-byte/stale-target refusal and unchanged history, selection and placement. |
| Readable missing-media state; no accidental silent placeholder in final export | Native Original/sound recovery reports name the missing media and preserve the edit; the final recovery replays cover both. Current release renders refuse a missing placed sound and a same-length corrupt sound with `render_audio_failed`, no publication and confirmed cleanup. Restoring identical bytes makes Render succeed without an authored change. |
| Portable ownership preserves linked media | Earlier linked-Original portable export and this audit's linked-sound portable export pass after their external files become unavailable. Both portable ownership records are managed without linked paths, full validation passes and the authored document is unchanged. |

## Current release run

Source `7fa6dd062dd75c94f94cef6cc2d306cc0f4c86e6`, Apple M5 Max with 128 GiB,
macOS 26.5.2, Rust 1.97.1 and qualified FFmpeg 8.0.3. Release CLI SHA-256:
`843934fc640fcd5a2169e4ba3a25cd686e75bbeb125764679448acbda4de2e9d`.
The [retained evidence](../../tools/media-qualification/evidence/2026-10-11-local-import/README.md)
includes scripts, command results, full authored comparisons, reports and movies.

A real 120-frame CFR/B-frame Original at 30000/1001 fps receives a linked
48 kHz stereo WAV at Edit frame 10. Catalog registration preserves every picture
node and the Original asset. Removing the scratch WAV makes Render fail in
0.715 s; changing its first byte without changing its length fails in 0.710 s.
Both report qualified source audio unavailable, retain no published movie,
confirm cleanup and preserve the complete authored dump. Repository fixtures
were never changed.

Restoring identical bytes verifies the retained source and publishes in 1.713 s.
A portable copy then publishes in 1.728 s with the external WAV absent; the
source package still refuses to render, proving the test did remove its input.
Both normal Render reports verify 120 pictures and 192,192 canonical samples.
Separate `verify-export --every 1` runs each compare all 120 pictures and three
audio windows. A third comparison after audio-only catalog registration proves
that unplaced imported sound changed neither picture nor mix: **360 picture
checks, six signal windows with zero measured offset and three silent windows
pass**. No thresholds or alignment policies changed.

The current focused store run passes **36 tests, zero skipped** across
`single_source`, `original_media`, `original_preparation`, `prepared_relink`
and `bookmarks`. The adjacent sound-recovery milestone supplies the current
1,106 distinct passing app tests, strict Clippy and final Metal replays. This
audit changes documentation/evidence only and does not repeat that full gate,
sanitizers or bundle build. The first inspection command omitted `--json` and
returned usage; the corrected invocation passes, with both results retained.

## Scope reconciliation

The old DP-15 remainder included already implemented native retry, ownership,
bookmark and sound-repair paths. This audit closes those with their evidence.
§15.1 makes drag-and-drop optional. Development-format compatibility work is
governed by the owner's breaking-format permission, without removing existing
compatibility behavior. Broader format/creative-operation and media-engine
qualification remain in DP-02 and DP-16; this audit does not close them.

Actual File Provider eviction, rights-cleared external camera/phone footage,
physical keyboard/input methods, VoiceOver speech and second-Mac behavior
remain on [To verify (owner)](../REQUIREMENTS.md#to-verify-owner) under §29.1.
Their absence does not stand in for unimplemented import behavior. No native
window, browser, listening test or fresh packaged-app run was used in this audit.
