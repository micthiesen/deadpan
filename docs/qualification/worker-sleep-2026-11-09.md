# Worker sleep policy, 2026-11-09

Every registered background job kind now has a sleep policy. Recognition,
standalone pause detection and AI pause generation already had one
([transcription](transcription-sleep-2026-10-17.md),
[pause detection](activity-sleep-2026-10-17.md),
[generation](generation-sleep-2026-10-17.md)). This record adds tracking,
target proposals, shot detection and seek proxies, and records why the
remaining kinds keep their existing contracts. It advances DP-18; physical
sleep and native notification delivery stay on the owner list.

Base revision `ae5febc81`, Apple M5 Max / Mac17,7, 128 GiB RAM, macOS 26.5.2,
Rust 1.97.1.

## Method

No second sleep observer was added. The native workspace notifications still
advance the one `jobs::Lifecycle` epoch. The per-run capture that recognition
used (`RunCapture`, with its 20 ms scoped watcher and its own interruption
cause) moved from the transcript module to `jobs::lifecycle`, unchanged, so
every worker shares it. Each newly covered run captures the epoch on command
entry, and the watcher signals the job's existing cancellation flag, so
teardown is the job's own supervised path (Vision worker, media worker group,
decoder). Tracking, target proposals and shot detection also joined
`JobKind::interrupted_by_sleep`, so their coordinator boundaries
(`checkpoint`, `wait_admitted`) stop them and Run anyway cannot revive them.

Tests inject the sleep/wake pair through the lifecycle atomics, including a
coalesced pair the job sees only after wake and a run started while asleep.

## Behavior by kind

| Kind | At sleep | After wake | Durable state |
| --- | --- | --- | --- |
| Tracking | Interrupted; the worker is torn down | Only an explicit `:track` or Camera's T | Target, history and revision unchanged; the writer checks the capture immediately before committing a finished path |
| Target proposals | Interrupted; the worker is torn down | Detecting again | No edit; proposals from a run that crossed sleep are never shown |
| Shot detection | Running and queued scans stop at the next picture or admission | Resumes automatically from its cached ranges | Range cache kept; a result from a run that crossed sleep, including one buffered for a busy service, is discarded; sleep is not remembered as a failure |
| Seek proxy | The media worker group is stopped rather than suspended across sleep with a VideoToolbox session | Rebuilds automatically, reusing completed ranges | Completed ranges kept; a proxy verified and published before the notification stays; Jobs-panel cancellations and failures are not retried |
| Render, YouTube import, model install, portable copy, backups, storage cleanup | Unchanged | Not restarted | Their own durable records and verification already report a sleep-induced fault truthfully |
| Thumbnails, waveforms, playback warming | Not registered jobs | Requested again | Request-bound caches; playback already stops at sleep |

Tracking and target proposals are requested analysis on a model worker
(Vision), like recognition: a new run may not be wanted after wake, and none
can resume mid-run, so retry is explicit. Shot ranges and proxy ranges are
valid checkpoints of cheap, rebuildable work (specification §18.5), so
resuming costs only the unfinished ranges and publishes nothing to history.
A cancellation already requested when the watcher observes sleep keeps its
outcome in every kind.

Shot saves are not epoch-guarded inside SQLite as transcript saves are. The
analysis is a deterministic content-keyed function of the immutable
Original; the capture only keeps a run that crossed sleep from publishing.
A proxy finished before sleep is kept for the same reason: it passed full
verification, and the next build checks it again before use.

## Verification

Focused tests (`cargo nextest run -p deadpan-app --bin deadpan-app`):

- `jobs::tests::sleep_stops_shots_and_requested_analysis_but_leaves_proxies_to_their_monitor`
- `jobs::lifecycle::tests::sleeping_native_work_is_cancelled_without_ui_or_progress_callbacks`
  and `cancellation_already_requested_takes_precedence_over_coalesced_sleep`
  (moved with `RunCapture`)
- `project::tests::targets::sleep::sleep_interrupts_tracking_without_saving_and_only_an_explicit_track_retries`:
  the real service job with the scripted Vision seam; outcome Interrupted,
  the message names `:track`, no stored samples or provenance, head
  revision unchanged, the Jobs row released, an edit commits, a writer
  round trip leaves the job Interrupted, and an explicit `:track` saves
- `project::tests::targets::sleep::cancellation_requested_before_sleep_keeps_its_cancelled_outcome`
- `project::tests::targets::sleep::tracking_started_while_asleep_stops_at_once_without_saving`
- `project::tests::targets::picture_proposals::sleep_interrupts_detection_without_showing_proposals_and_detecting_again_retries`
  and `detection_started_while_asleep_proposes_nothing`: stored document,
  register bank and history rows identical before and after
- `preview::shots::tests::sleep_stops_a_running_scan_and_wake_resumes_it_from_cached_ranges`,
  `a_buffered_result_from_a_run_that_crossed_sleep_is_never_saved`,
  `cancellation_before_sleep_stays_a_failure_with_try_again`
- `preview::proxies::tests::sleep_stops_a_running_build_and_wake_resumes_it_without_a_failure`
  and `sleep_keeps_a_verified_proxy_and_never_masks_cancellation_or_failure`

The 43 sleep, interruption, shot and proxy tests pass, as do the 230 tests
selected by `targets|jobs|transcript|shots|proxies|camera|refinement`, which
include the earlier recognition, detection and generation sleep coverage.
`cargo xtask check` passes (format, strict Clippy, changed-package tests).

Replay `targets` (`cargo xtask replays --scenario targets --jobs 1`) now
injects sleep while background tracking runs from Camera. It waits for the
Interrupted outcome, checks that the target stays untracked, the revision is
unchanged, nothing runs, Camera stays open, and the message
"Tracking Target 1 stopped when the Mac slept; the target is unchanged.
Track it again with :track." is painted fully visible; then Shift-T starts a
fresh run and the existing save, follow and correction checks pass: 74 checks.
The capture was inspected: the footer carries the full message and the
rail shows Target 1 as untracked. Two passing runs measured 6,040 and 6,445
frames because
tracking repaints continuously while its worker runs, so its trace budget
is now 8,000 frames. Existing layout-retry warnings remain warnings.

## Limits

- Physical lid close, Apple-menu sleep and native notification delivery are
  not exercised; see the owner list in [requirements](../REQUIREMENTS.md#to-verify-owner).
- The real Vision tracker, the real shot scan over a full Original and a real
  VideoToolbox proxy build are not run under injected sleep. The service tests
  keep range resolution, the verified Original copy, the tracking policy and
  the guarded save real; the shot and proxy tests drive the same watcher and
  cancellation flag their workers use, not a decode.
- The watcher polls every 20 ms; this is not a scheduling guarantee before
  the system actually sleeps.
- If a proxy build failed for a reason caused by sleep before the watcher
  saw the notification, the failure would be remembered as an ordinary one
  until `:proxies retry`. The notification precedes sleep, so this needs a
  fault inside that window; guarding it would mean classifying worker errors
  by timing.
- Render keeps its contract: a sleep-induced encoder fault fails the attempt
  and final-file verification refuses a damaged movie, with explicit retry.
  It is not proactively interrupted.
