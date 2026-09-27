# Playback test scheduling, 2026-09-27

This increment addresses the recurring playback test startup timeouts. It changes
test scheduling and failure diagnostics only. The production controller,
preparation, source admission, PCM readers, prefill and watchdog values are
unchanged. Core schema 28 and database schema 34 are unchanged. No product
requirement or delivery gate is promoted.

## Reproduction

The unchanged workspace test binary from the
[routed voice increment](routed-voices-2026-09-27.md) failed five of 34 tests in
75.40 seconds with full backtraces. Four failed while waiting for a fake device
to start. The fifth was the expected worker-panic recovery test: the injected
`panic!` invokes the process panic hook before `catch_unwind` can publish failure.
Full backtrace formatting adds diagnostic work before the recovery under test.

A diagnostic-only build retained the ten-second deadline and failed the same
four startup tests, passing the other 30. All four failures had `Preparing`, an
opened generation, no sample and no reported error. The failures were:

- `canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock`, device 0.
- `seek_reuses_private_pcm_but_a_new_session_must_reopen_sources`, device 0.
- `original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock`, device 0.
- `original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm`, device 1.

The same diagnostic binary passed those four tests together, with default test
parallelism, in 22.05 seconds. Its SHA-256 before and after was
`81a6a6a6755e61494567e98135050b07cd0ce31db8e908a90049d465a22b407a`.
This establishes sensitivity to concurrent suite work; it does not identify an
internal DSP hotspot. A one-second macOS `sample` attempt was denied by the
sandbox before it produced a sample. No profiler result is claimed.

## Change

Every real-media scenario in this library acquires the same PCM reservation
before fixture registration, reference PCM and playback. Direct source readers,
fake-device playback assertions and the routed voice tests participate too.
The focused reservation-lifetime test uses its own slot and blocks the factory
before any PCM preparation. The ordinary test runner command is unchanged;
pure numeric tests remain parallel.

The test engine's existing shared repaint callback owns a reservation reference.
Both workers retain that shared object, so asynchronous cancellation cannot
release the reservation early. This leaves production shutdown nonblocking.
A regression holds the device factory, drops the engine and test reference,
proves the reservation is still occupied, then releases the worker and observes
the reservation become available. Admission is bounded to ten minutes, including
queueing behind whole test scenarios. The regression also exercises timeout
while the detached worker owns the reservation and successful admission after
release, using a zero-duration bound without timing-sensitive sleeps. This
addressed the independent review's sole finding: a leaked worker must not leave
every subsequent PCM test blocked indefinitely.

Startup failures now name the caller, device index, created-device count and
last engine update. The phase helper retains unexpected updates and fails
immediately on an unexpected `Failed`. Successful startup does not consume the
`Preparing` update used by clock assertions. The injected worker failure uses
`resume_unwind`, exercising the existing unwind boundary without calling or
changing the process-global panic hook.

All existing PCM comparisons, decoder paths, full prefill, ten-second worker
waits and production watchdogs remain. Test scheduling is not a performance
qualification. Product latency, actual device behavior and acoustic review
remain open.

## Durable workflow

The project instructions now require focused checks during implementation and
one workspace gate per coherent delivery milestone after review. Small later
fixes rerun their affected targets without discarding unrelated passing results.
One session owns Cargo execution and checks an existing process after an
interruption. Capability failures are retried when the capability changes, not
in a loop. Git checkpoints replace repeated full-tree archives and copies of
gate scripts.

CI keeps both default-app and `ui-harness` lint/tests because those builds have
different runtime branches. Redundant post-test workspace build and doctor
steps were removed: integration tests already execute the normal application,
CLI, doctor and media worker. Builds remain appropriate for packaging and
native startup. Existing PCM assertions, including all 120 cold projection
reads in the retained-history test, remain intact.

## Verification

The [retained record](../../tools/media-qualification/evidence/2026-09-27-playback-waits/verification.json)
separates each run and preserves failures:

| Check | Result |
| --- | --- |
| Scheduled playback library, before the admission-bound review fix | 35 passed, 0 failed |
| Full workspace tests | 1,809 passed, 1 failed, 0 ignored |
| Workspace formatting and strict Clippy | Passed |
| Contributed UI harness tests and strict feature Clippy | 236 passed, lint passed |
| Final six affected playback cases with full backtraces | 6 passed |
| Final formatting and strict playback Clippy | Passed |
| Unchanged artifact binary after access was restored | Previously blocked test passed |

The workspace failure was
`directories_fifos_and_sockets_are_rejected_without_blocking` at
`deadpan-jobs/tests/artifact.rs:200`: the old sandbox denied Unix-socket creation.
After the user granted full access, a socket probe and the exact failed test
passed. The artifact test binary's SHA-256 stayed
`34cafef67b9059d652e183cb78741bf0807757cf8d2cd6f651c7ca104add8870`.
The failed broad run remains a failed run; the isolated result establishes that
this permission blocker was removed.

The resource helper's admission bound was added while the broad gate was in
flight. It is the only changed file among 587 source/config paths between the
gate snapshots. Final focused checks cover the four formerly flaky startup
cases, reservation timeout/release and panic recovery on that final helper.
The other 29 playback cases retain their passing broad-run evidence; the
expensive DSP suite was not repeated for this small test-only fix. Final source
hashes stayed unchanged throughout focused verification.

The independent reviewer found the reservation fix and revised workflow correct
after the bounded-admission correction. All 59 application files and all 11
ImageGen boards/prompts are unchanged from the prior checkpoint. Captured raw
logs and patches retain their bytes; Git whitespace checks exempt only those
evidence formats. Source and documentation whitespace checks remain enabled.
No new painted replay, native startup, acoustic, aesthetic or performance result
is claimed. No product requirement or gate is promoted.

Git metadata writes, remote access and GitHub ADMIN permissions were verified
after full access was restored. The contributed harness and accumulated project
work can now be committed normally.
