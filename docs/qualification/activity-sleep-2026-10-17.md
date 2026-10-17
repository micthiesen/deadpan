# Standalone pause detection across sleep, 2026-10-17

Standalone speech-activity detection now owns the same sleep interruption and
guarded publication policy as recognition, through its own run capture. This
advances DP-10 and DP-18; physical sleep notification delivery and other analysis
workers remain separate qualification work.

Base revision `a9888d1b`, Apple M5 Max / Mac17,7, 128 GiB RAM, macOS 26.5.2,
Rust 1.97.1 and qualified FFmpeg 8.0.3 dependencies.

## Behavior

Sleep invalidates queued and running detection. A scoped watcher signals the
existing supervised cancellation path without waiting for UI frames. The scan
slot remains owned until the job drains. Wake and model changes do not restart
the old attempt; Try again creates a fresh run only while awake and after drain.
An already requested cancellation keeps its own outcome.

Detection has a separate capture from recognition. A completed transcription
cannot accidentally cancel a later detector. A detector produced alongside
recognition retains that recognition epoch. Late events and buffered activity
are rejected after interruption. Queued saves carry their epoch to guarded
SQLite publication; the final successful guard orders a commit against sleep.
Typed receipts distinguish interruption from successful admitted commits and
unrelated storage errors. Stored words, corrections and edit history remain
unchanged.

The native worker reports a zero-valued phase marker only after loading the
detector model, immediately before entering its synchronous detection call.
The host can observe that marker through `detect_speech_with_progress`; the
existing `detect_speech` API ignores it. This does not claim incremental
inference progress, a kernel interruption point, or resumable detection.

## Qualification scope

The detector file is pinned by the approved Whisper pack: 885,098 bytes,
SHA-256 `2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`.
Qualification uses a private download of that file, not an installation into
the user's model store. No physical sleep is requested.

## Verified results

The scoped gate passes **56 tests** and strict Clippy in normal and UI-harness
configurations. Coordinator tests cover queued/running invalidation, retained
resource ownership and prior cancellation for transcription and speech
activity. Activity tests cover buffered and delayed output, no wake/model-change
restart, awake-and-drained retry, stale recognition isolation, and typed save
receipts for successful commits, interruption, cancellation and real errors.
The selection also exercises the existing guarded annotation transaction and
hostile analysis-worker supervision tests. Independent review found no
remaining lifecycle or publication defect.

The explicitly invoked ignored real-worker test uses 120 seconds of synthetic
sine PCM. The worker's model-loaded notification arrived after **5.129 s**;
the test injected lifecycle sleep/wake from that notification, and supervised
cancellation drained in **514.775 ms**. A fresh one-second run completed in
**643.521 ms**, returning 32 activity hops with `whisper.cpp 1.8.3`, CPU. This
proves cancellation after loading and a fresh retry, not a particular point
inside synchronous inference or classification accuracy on real speech.

The executed helper SHA-256 is
`4369d0f1917b3ac2e7692dad72b29ebaf55521dc75369a4be7200a2a9bbafe2e`.

The release visual and performance `transcript` replays each pass **60 checks**
plus the production shortcut audit. They inject standalone detection sleep
after storing a transcript, verify complete warning/button paint with keyboard
focus, activate retry with Enter, and prove the same transcript/correction
objects and edit revision remain. The captured UI was visually inspected:
stored words remain available above the complete warning and focused retry.
The existing transcript/search/pause workflow also passes. These replays use
synthetic annotations and a missing model store, not real recognition or audio.

The executed release app SHA-256 is
`3235da4a9e1e06f713406a83d726b4941fdef92be05e25d63be19409e65a5086`.
Native Metal initialization, window startup and shutdown pass. Reports retain
the existing scroll-to-reveal and settled multi-pass layout warnings; no
layout failure is ignored. The test build's existing large unwind-section
linker warning remains separate from the clean strict Clippy checks.

[Evidence](../../tools/model-qualification/evidence/2026-10-17-activity-sleep/)
retains the source and binary identities, checks, real-worker inputs/results,
release reports, inspected capture and verified file inventory. Physical
notification delivery remains on the owner list. Other analysis workers,
resumable detection and broader DP-18 scheduling remain unfinished.
