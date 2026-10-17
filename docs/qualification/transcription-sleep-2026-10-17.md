# Transcription sleep interruption, 2026-10-17

App-owned initial recognition and Recognize again now stop across sleep/wake
and require an explicit retry. This advances DP-10 and DP-18; it does not
qualify physical sleep delivery or close either requirement.

Base revision `ce3cbda7`, Apple M5 Max / Mac17,7, 128 GiB RAM, macOS 26.5.2,
Rust 1.97.1, qualified FFmpeg 8.0.3 native dependencies.

## Lifecycle and publication

Each run captures the coordinator's monotonic lifecycle epoch, including a
sleep/wake pair that occurs between UI frames. A scoped watcher polls every
20 ms and signals the existing cancellation token without relying on progress
callbacks or a repaint. This is a polling interval, not a guaranteed scheduling
latency. The watcher ends with its worker; native cancellation retains the
existing supervised teardown. Queued recognition never loads its model after
interruption, and an interrupted resident worker keeps its model slot until
draining. Run anyway cannot revive an invalidated run.

If cancellation was already requested when the watcher observes interruption,
cancellation takes precedence. The watcher retains its own interruption cause
so its cancellation signal is not mistaken for a user cancellation. This policy
does not claim exact chronological ordering of simultaneous clicks and native
notifications.

The rail distinguishes interruption from failure and offers Try again once
the previous worker has drained and the lifecycle is awake. Wake and model
updates do not automatically restart recognition. Recognize again retains the
published transcript and corrections while its replacement is interrupted.
Completed recognition relinquishes its lifecycle capture so a later independent
speech-activity run is not cancelled by an old recognition epoch.

The same captured epoch accompanies a transcript and speech activity produced
by recognition through the annotation queue. Stale events and unsaved results
are discarded. The project writer uses guarded SQLite saves, checking before
the upsert and immediately before commit. A failed guard rolls back a new row
or replacement and does not create history. The final successful guard orders
publication against sleep: a notification after that observation does not
retract the admitted commit. Manual/headless annotation saves and standalone
speech activity retain their separate contracts.

An in-flight save keeps its attempt identity until the authoritative receipt.
The receipt carries a typed interruption flag only for guard rejection, so a
successful commit or an unrelated storage failure arriving after sleep keeps
its actual outcome. Tests cover both receipt/UI ordering cases, initial and
replacement transcripts, sibling activity, and prior explicit cancellation.

## Verification

The scoped gate passed 55 tests and strict Clippy in normal and UI-harness
builds. Four dedicated store tests cover early/final rejection for both new
and replacement annotations, successful guarded commits, reopening and unchanged
history. Service coverage injects coalesced sleep/wake before queued saves,
checks absent annotations, retries with a fresh epoch, applies a real word
correction, and proves later interrupted replacements preserve proposals,
corrections and history after reopening. Coordinator and UI-state tests cover
queued/running interruption, retained resource ownership, prior cancellation,
delayed results, explicit retry and independent speech-activity protection.

Initial checks caught a missing replay query-trait import and a test fixture
that requested cancellation without registering a cancellation flag. The test
now uses the production registration shape and asserts that cancellation was
actually signalled. No behavior assertion or threshold was relaxed.
The first visual replay also checked before a new UI frame processed sleep;
it now waits for the interruption state and verifies another wake-only frame
does not restart it. Review caught and fixed the save-reply classification race
described above before final qualification.
Inspection of the first passing visual capture found the focused retry button
clipped below the rail. The final replay therefore checks complete button paint
after keyboard focus, in addition to focusability and Enter activation.
The scroll check waits for actual painted geometry, since the scroll offset
is applied on a later frame. Four points of trailing space avoid fractional
pixel clipping at the maximum offset. The faster performance replay also
exposed contention in its synthetic setup; it now waits for the annotation
lane and asserts submission acceptance before waiting for publication.

An explicitly invoked ignored test ran the actual native Whisper helper on
120 seconds of deterministic sine PCM. Its progress callback injected the
lifecycle sleep/wake pair at 25%, after 5.241 seconds. The scoped watcher and
supervisor drained cancellation in **356.724 ms**. A fresh capture and token
then transcribed one second successfully in **723.447 ms**, reporting
`whisper.cpp 1.8.3`, Metal and zero words. Zero words is expected as a permissible
result for this synthetic input; this test makes no recognition-accuracy claim.

The pinned base.en model is 147,964,211 bytes with SHA-256
`a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002`.
The private executed worker SHA-256 is
`bfa68068195c975709d262f6fca8c05df508b67bd31fe270f0fa43f26f2d2bee`.
The test binary identity and exact output are retained separately from the
release UI qualification. No physical sleep was requested.

The final release visual and performance `transcript` replays each pass **58
checks**, plus the production shortcut compatibility audit. The complete
interruption warning and focused retry button were visually inspected; Enter
returns to the real missing-model offer without changing the edit. The replay
then completes its existing synthetic transcript, search, word/pause navigation
and repetition checks. It does not run recognition or physical audio output.
The performance replay records input-to-state p95 **1.281 ms** across 69 inputs
and offscreen composition completion p95 **0.560 ms** across 322 samples. These
are this small replay's measurements, not broad editor latency qualification.
Existing scroll-to-reveal and settled multi-pass layout warnings remain in
the reports; neither run has failed or indefinitely repeated layout checks.

The executed release app SHA-256 is
`06bec67c53b7d2d9c7cbd6fb78d983220f70acee19e1d4ac02ade93751410860`.
Native Metal initialization, window startup and shutdown pass on that binary.
The test build emits the existing large unwind-section linker warning;
strict Clippy has no code findings. Independent review found no remaining
lifecycle/publication defect after the receipt fix.

[Evidence](../../tools/model-qualification/evidence/2026-10-17-transcription-sleep/)
retains the checks, real-worker identities/results, source identities and UI
reports. Physical notification delivery remains on the owner verification list.
Standalone speech activity, other analysis workers, recognition checkpointing
and broader inference scheduling remain separate unfinished work. No inference
checkpoint or GPU pause/resume capability was added to Whisper.
