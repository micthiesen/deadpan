# Background jobs

`crates/deadpan-app/src/jobs.rs` is the app's job coordinator (DP-18,
specification §18.3 to §18.5). Every background job registers with it. It
lists them, decides when each may proceed, and releases them on shutdown. The
Jobs panel (`:jobs`, Deadpan › Jobs…) shows what it holds.

The coordinator is a small shared board (`Jobs`, one per `ProjectService`,
read through `service.jobs()`), not a plugin framework:

- It runs no work and owns no threads.
- It never takes the project writer.
- It does not replace any job's own contract. Session and revision capture,
  stale-result rejection, durable attempt and render records, retries and
  each job's own shutdown stay exactly as before.

A job holds a `JobHandle`. The handle can wait for admission, wait at safe
boundaries, report progress and observe cancellation. Dropping the handle
removes the job and admits the next waiter.

A job is admitted when its resource has room and no admission condition
holds it ([Admission conditions](#admission-conditions)). Once admitted, it
runs to its own safe boundaries; conditions never stop it.

## Priority classes

| Class | Work | Relation to jobs |
| --- | --- | --- |
| `Interactive` | Realtime audio, current-frame preview and seek | Never a job. Jobs yield to it (see Yield rules). |
| `Edit` | Commands on the project writer | Never a job and never queued behind one. |
| `Render` | User-requested render | Admitted at once. |
| `Requested` | Tracking, face detection, AI pause generation, YouTube import, model install, portable copy | Analysis or transfer the person asked for and is waiting on. |
| `Background` | Transcription, pause detection, shot detection, seek proxies | Automatic work. |

When a resource frees, the highest-priority waiter is admitted first. Equal
priorities are admitted in registration order.

The specification lists generation with "remaining background analysis".
Here, a generation the person asked for precedes automatic transcription
for the model slot. Neither ever precedes rendering, editing or preview.

## Resource budgets

| Resource | Capacity | Kinds | Why |
| --- | --- | --- | --- |
| `Inference` | 1 | AI pause generation (LTX/MLX), transcription (whisper.cpp with Metal) | §18.4: one large model at a time. Two resident models compete for unified memory and the GPU. |
| `Scan` | 1 | Shot detection, pause detection | Automatic whole-Original decodes of the same file. |

Jobs without a resource are admitted at once. These are renders, tracking,
face detection, proxies, downloads, model installs and copies. Tracking is
never queued behind a background scan. The scan yields to tracking instead.

A job that cannot run is **queued** rather than refused. Its row reads
`Queued #n, waiting for <holder>`. A queued job holds no resource and can be
cancelled.

A **deferred** registration (`JobSpec::deferred`) runs at once without its
resource and claims it only around the step that needs it
(`JobHandle::claim`/`acquire`), giving it back with `JobHandle::release`.
Transcription and AI pause generation are deferred, so their CPU phases and
publication never hold the model slot.

Some jobs already serialize themselves and still refuse a second instance:

- one AI pause job per pause, and at most four AI pause jobs per project
  (`MAX_LIVE_GENERATIONS`); further ones queue for the model slot
- one tracking job
- one face detection
- one YouTube import
- one model job
- one portable copy

Admission only orders different jobs.

Where each job waits for admission:

| Job | Wait point |
| --- | --- |
| AI pause | Each attempt claims the model after conditioning and allocation, just before its worker starts, and releases it as soon as the worker has exited, before the writer publishes the run. Reading the boundary pictures (about one second) and recording the request happen immediately, so edits made while queued cannot stale the captured pictures. The attempt is durably recorded while queued; the service's phase is `Queued` ("Queued for the AI model"). Once admitted, the job rereads the request (read-only); if edits made it stale meanwhile, the model is not loaded and the attempt ends `Cancelled` with a note to generate again. A cancellation or shutdown while queued ends the attempt `Cancelled` without launching a worker. |
| Further AI pauses | Generating another pause while one runs starts a further job beside it, with its own thread, conditioning, recorded attempt and Jobs row. It waits for the model slot like any other model job and is cancelled from its own row (X) or `:cancel-ai` on its pause. The service processes it by swapping it into the same single-job state, so records, refusals and per-request rules (one nonterminal attempt per request) are unchanged. Inserted AI pauses (`,a`) keep their durable preparation queue and start one at a time. |
| Transcription | Claims the model only before whisper starts; reading the sound and detecting pauses (CPU) run first and hold nothing. |
| Shot and pause detection | Before opening the Original. |

## Admission conditions

Heavy work also waits at its admission boundary while the machine asks for
restraint (§18.4). Conditions are fixed per job kind (`Gates` in
`JobKind::rules`); memory estimates come from the selected model pack:

| Resource | Waits while | Why |
| --- | --- | --- |
| `Inference` (AI pauses, transcription) | on battery power, in Low Power Mode or under thermal pressure; while Original or edit playback runs; under memory pressure (warning or critical); when estimated available memory cannot cover the selected model plus the OS/editor reserve | A model load is seconds of GPU and gigabytes of unified memory. |
| `Scan` (shot and pause detection) | under critical memory pressure | A whole-Original decode is smaller and resumable. |

- **Observation.** Power and thermal state are the seek proxies' own reading
  (`deadpan_cli::system_conditions::power_guard`, from `pmset`). Memory pressure is the
  kernel's `kern.memorystatus_vm_pressure_level` (1 normal, 2 warning, 4
  critical) read through `/usr/sbin/sysctl` with the same bounded runner. No
  new unsafe code or OS binding is involved. Only a waiting job thread
  samples (`Jobs::set_probe`, with a 15-second cache invalidated by model
  release, outside the board lock), never the UI or the writer. Until a
  reading exists the job shows "Waiting while Deadpan checks power and memory".
- **Native launches only.** `DeadpanApp::follow_system_conditions` installs
  the probe from `main`. Tests and replays inject observations with
  `Jobs::set_conditions` and never read the Mac's power state.
- **Visible reason.** The row reads, for example, `Waiting while running on
  battery · 0:12 waiting · O runs it anyway`; the AI inspector and the
  transcript section say the same.
- **Run anyway.** O (or the row's Run anyway button) lifts power, heat,
  playback and memory-pressure conditions for that job only
  (`Jobs::run_anyway`). It cannot bypass the memory byte budget or a missing
  memory observation. Those rows offer Cancel, without a Run anyway hint.
  Resource capacity still applies: two models never load together.
- **Playback.** A queued model does not start while anything plays. Running
  AI picture generation requests a cooperative pause at its next supported
  inference boundary. The panel distinguishes the request from the worker's
  acknowledgment after dispatched GPU work drains. This is not instantaneous
  kernel preemption. Playback never waits for the acknowledgment. Transcription
  still has no running-worker pause policy.
- **Memory admission.** Before claiming inference, the selected pack's
  `memory_bytes` estimate must fit in observed headroom after reserving the
  greater of 8 GiB or one eighth of physical RAM for the OS, editor and other
  applications. Missing estimates or unreadable headroom hold the job. The
  reserve is a policy, not a measured bound on future application allocations.
  `sysctl hw.memsize` supplies physical RAM; `vm_stat` supplies free,
  speculative and inactive pages. Their sum estimates reclaimable headroom.
  Apple's displayed free count excludes speculative pages, so these are added
  once. Compressed and purgeable counts are not added. Pressure remains an
  independent gate. Model release invalidates the observation so a waiting
  thread samples again outside the board lock. No running job is evicted when
  headroom falls, and its estimate is not subtracted from headroom again.
  Run anyway cannot override this admission budget. Pack estimates are not
  hard allocation limits or proof of worst-case footprints across every
  supported input.
  The calculation follows the available-memory approximation in
  [psutil's macOS collector](https://github.com/giampaolo/psutil/blob/master/psutil/arch/osx/mem.c),
  adjusted for [Apple's printed `vm_stat` counters](https://github.com/apple-oss-distributions/system_cmds/blob/main/vm_stat/vm_stat.c).

## Yield rules

A running job waits out these conditions at its own safe boundaries. Nothing
is preempted mid-step, and GPU kernels are not interrupted.

| Job | Yields while | Boundary |
| --- | --- | --- |
| Shot detection | the edit plays, a render runs, tracking or face detection runs | Between pictures (`JobHandle::checkpoint` in the scan's progress callback). Its ten-second checkpoints still save. |
| Seek proxy | the edit plays, a render runs, an AI model runs (generation or transcription), tracking or face detection runs | The existing monitor suspends the worker's process group (SIGSTOP/SIGCONT). It now combines `JobHandle::pause_reason` with the UI's busy signal and the power guard. |
| AI generation | playback or a render | Cooperative stage, denoising-step and decoded-chunk boundaries in the MLX adapter, after synchronizing dispatched GPU work. |
| Transcription, pause detection, tracking, faces, render | nothing | These workers do not implement cooperative scheduling holds. |

AI scheduling controls are identity/token-bound and monotonically sequenced.
The host bounds and coalesces pending changes; cancellation remains available
while paused. A paused model retains its inference slot and its loaded weights.
Only acknowledged pause time is excluded from the computation deadline;
cancellation and terminal-exit grace periods remain real monotonic deadlines.
Total acknowledged pause time is bounded at two hours. These holds are transient:
restart still requires a new attempt, and sleep still interrupts rather than
resumes generation. No pause checkpoint is written to project history.

A cancelled job that is still draining keeps holding its yields as well as
its resource: a cancelled render or model is still using the machine until
its worker stops.

Paused time does not use up a proxy build's deadline. The conversion
supervisor extends its deadline by the time the worker group was suspended,
verification by the time it waited, and the build's overall deadline by the
total paused time the app's monitor records (`BuildControl::paused`).

The UI sets the foreground state (`Foreground { playback }`) every frame from
its transport.

## Cancellation and drain

- **From the Jobs panel.** For a job registered with its own cooperative
  cancel flag (the same `Arc<AtomicBool>` its existing cancel uses),
  `Jobs::cancel` sets the flag and shows the row `Cancelling`. AI pause and
  tracking also get their existing request (`:cancel-ai`, `:track-cancel`)
  so their phases stay exact.

  Jobs whose cancellation is only a request have no flag (`CancelOutcome::NeedsRequest`):

  | Job | Request |
  | --- | --- |
  | Render | The Render window's cancel |
  | YouTube import | `Jobs::cancel` (the import's own) |
  | Model install | `Manager::cancel` |

  The panel sends the request and marks the row `Cancelling`
  (`Jobs::mark_cancelling`) only once it was accepted. A refused request
  (for example a render already finishing) is reported in the panel and the
  row keeps its state.
- **Draining keeps the slot.** A cancelled running job keeps its resource
  until its thread returns. Two models never load together.
- **Session change.** Each frame, `cancel_sessions_before` cancels the
  flagged jobs of project sessions the UI has left; each job's reset also
  does this. Session identities only increase, so a job the service starts
  for a session the UI has not shown yet is never cancelled. A render is
  cancelled by the service's own session change, never marked here.
  App-wide jobs (YouTube, model packs) continue.
- **Shutdown.** Window close and `on_exit` call `Jobs::shutdown` before the
  existing per-job shutdowns. It sets every cancel flag, refuses new
  admission and wakes every queued or paused waiter with
  `Stopped::ShuttingDown`. Each job's existing shutdown then joins a thread
  that is already draining.
- **Not cancellable.** Portable copies have no safe cancellation yet. The
  panel says so instead of pretending.
- **Proxies.** A proxy cancelled from the panel is not rebuilt at once. It
  shows "cancelled; :proxies retry continues it from its completed ranges":
  the ranges already encoded are kept and reused
  ([preview proxies](PROXIES.md#resuming)).
- **Shots.** A cancelled shot scan shows Try again.

## Sleep and model work

Native workspace sleep and wake notifications advance an atomic lifecycle
epoch. The callback does not take the coordinator or project-writer lock.
The writer cancels affected AI generations through their existing cooperative
stop and bounded worker teardown. Queued generations do not load their model;
running generations retain the resource until their worker drains.

A sleep/wake pair remains observable even if the writer resumes only after
wake. Affected attempts and pending automatic preparations become durably
Interrupted, retaining their retry inputs and intent. Wake never restarts them.
An explicit retry validates the inputs and creates a new attempt; this backend
does not claim an inference checkpoint. An earlier user cancellation keeps its
Cancelled outcome. Run anyway cannot bypass sleep interruption.

Publication checks the captured epoch between object promotions and at the
final Ready transaction boundary. If interrupted there, it rolls back the
provisional Ready record. The final successful guard observation orders
publication against sleep: a notification after it does not retract the
SQLite commit already admitted at that boundary. Ready variants and accepted
providers remain intact.

App-owned transcription and Recognize again also capture the lifecycle epoch.
A scoped watcher checks it every 20 ms and signals the existing supervised
worker cancellation path without requiring UI frames or progress callbacks.
The model slot remains owned until the job drains. A cancellation already
requested when interruption is observed retains its cancellation outcome;
this does not assert exact ordering for simultaneous notifications and clicks.

The rail shows interruption and an explicit retry; wake does not restart work.
Delayed progress/results and unsaved recognition output are rejected. Both the
transcript and speech activity produced by recognition carry the captured epoch
through the annotation queue. Their SQLite writes check it before writing and
immediately before commit, rolling back rejected inserts or replacements.
The final successful guard orders publication against sleep, as above. Already
stored transcripts, corrections and accepted media remain intact. Independent
speech-activity runs and other job kinds retain their existing lifecycle
contracts. See [transcription sleep qualification](qualification/transcription-sleep-2026-10-17.md).

## Jobs panel

Open the panel with `:jobs` or Deadpan › Jobs…. While it is open it owns the
keyboard. Editor bindings never see these keys; IME composition is observed
and never acts.

| Key | Action |
| --- | --- |
| Up/Down, or J/K | Choose a row |
| X | Cancel the chosen job |
| O | Run the chosen waiting job anyway |
| R | Retry the chosen interrupted attempt |
| D | Discard the chosen interrupted attempt |
| Escape | Close the panel |

The same actions are buttons; each button's accessible name includes its
row, for example "Cancel AI pause pictures · Pause (X)". A held key acts
once. Keys are read by their typed character through the shared mode-router
normalization (`navigation::mode_key`), so non-Latin layouts reach J, K, X,
R and D at their physical positions.

The chosen row is kept by identity (job ID, or request and attempt), not by
position. Rows reorder as jobs are admitted, finish or appear; an action
always applies to the row the person chose, and says so if it has finished.
Keyboard row movement scrolls the complete selected row and its actions into
view. The list keeps a visible scrollbar when lower rows extend past its panel.

Each row is one accessible sentence, for example: `AI pause pictures · Pause:
Running · Generating pictures 50% · 0:42 elapsed · about 0:40 left in this stage`. Queued
rows show their position, what they wait for and the waiting time; rows held
by a condition say why and offer O. Rows of an earlier session that is still
draining say "(closing project)". Progress comes from the job's own measured
stage: steps, frames or bytes. Without a measurement it is shown
indeterminate (stage only).

**Remaining time** is shown only from measured progress: the rate from the
first fraction reported in the current stage to the latest one. It needs at
least 5% progress over at least five seconds, and hides while the progress
is indeterminate, paused, requeued, complete, in a new stage, or once the job
has outlasted its own projection. Nothing is estimated from history or from
other jobs.

The AI cancel of a row addresses exactly that job (its ticket is the row's
tag), so several queued generations are cancelled one at a time.

## Quitting with jobs

A window close, ⌘Q or Dock Quit while work the person asked for (a render,
AI pause pictures, tracking, face detection, a YouTube import, a model
install or a portable copy) is running or queued asks first:

| Choice | Effect |
| --- | --- |
| Quit when they finish (focused) | The window stays open with a banner; Deadpan quits once exactly those jobs have finished. Quitting again asks again. |
| Cancel them and quit | The existing close path: `Jobs::shutdown`, each job's own drain, then release of the writer once cleanup is known. |
| Go back (Escape) | Stay open; a pending automatic quit is withdrawn. |

Automatic work (transcription, shot and pause detection, seek proxies) never
asks: it stops on quit and resumes next time. The question owns input above
every draft, like the unsaved-previews question, which still comes first.
On macOS the native quit guard (`deadpan-lifecycle`) cancels system
termination while such work is live and hands it to this question
(`QuitGuard::set_background`, `take_deferred`); no new unsafe code was
added. Waiting to quit changes nothing about how jobs run, drain or record.

**Interrupted AI attempts.** A writable open marks abandoned attempts failed
`interrupted` (`GENERATION_ATTEMPTS.md`). The store now lists them with
`ProjectStore::interrupted_generation_attempts`. An attempt is listed when
all of these hold:

- it failed with host code `interrupted`
- it is still the newest attempt of a current request
- the person has not dismissed it

The list survives acknowledging the recovery report.

- **R (Retry)** generates one variant for that pause through the same path as
  `:generate`. Unchanged boundary pictures add a new attempt to the same
  request, reusing its validated inputs and a new attempt ID. Changed
  pictures record a new request that makes the old one stale. Either way the
  entry leaves the list.
- **D (Discard)** calls `dismiss_interrupted_generation`. The attempt keeps
  its failed record. The dismissal is an operational file beside the
  recovery report (`Reports/dismissed-interruptions.json`). It is outside
  authored history and survives reopening. Each dismissal prunes entries
  that no longer hide an offered attempt (one per current request), and the
  record is capped at 1,024 entries and 256 KiB.
- **An unreadable record hides nothing.** A missing, corrupt, oversized
  (checked before parsing, never truncated) or non-file record is read as no
  dismissals. Every offered attempt is listed and the panel shows why; the
  next Discard rewrites the record. Losing it can only show an entry again,
  never change project state.
- An attempt whose pause is no longer in the edit offers only Discard.

## Verification

- **Coordinator unit tests** (`jobs::tests`) cover:
  - priority order
  - the one-model and one-scan budgets
  - highest-priority admission into a freed slot
  - yield reasons and resumption
  - queued and running cancellation; a cancelled runner keeps its slot and
    its yields while it drains
  - owner-request cancellation, marked only after acceptance and never by a
    session change
  - deferred claims, release and reacquire around a model step
  - the waiter's own cancel flag
  - shutdown wakeups
  - earlier-session cancellation
  - uncancellable rows
  - progress clamping and row order
  - power, thermal and Low Power holds for models only, Run anyway still
    bounded by the model slot, no stop of running work
  - playback holding a queued model while a running one continues
  - memory pressure levels (warning holds models, critical holds scans)
  - the probe sampled only by a waiting thread, never at registration
  - remaining time only from measured progress over enough time, hidden
    when too early, indeterminate, paused, in a new stage or outlasted
  - requested jobs and owner tags for the quit question
- **Panel tests** (`preview::jobs::tests`) cover the accessible row sentences
  and selection by identity across reordering.
- **Store tests** cover listing after a writable reopen, supersession by a
  retry, a durable dismissal, independence from recovery acknowledgement,
  staleness after a new request, pruning, and corrupt or oversized records
  (nothing hidden, a warning, rewritten by the next Discard).
- **Conversion test** (`suspended_time_extends_the_deadline`): a worker
  suspended longer than its whole deadline still finishes.
- **Service concurrency tests** (`project::tests::jobs`) run these together
  with deterministic scripted backends:
  - the real AI pause job and tracking job
  - stub transcription (deferred, claiming the model like production), shot
    scan and proxy jobs that honor the same admission, checkpoint and pause
    rules without decoding

  They assert:
  - edits commit through the writer while all five jobs are live, including
    a model run that never ends and a scan held paused, so a job holding or
    waiting on the writer would time out (no timing budget is asserted)
  - transcription is queued, not refused
  - the scan yields to tracking, then to playback
  - the proxy yields to either model, then resumes and progresses (waited
    on deterministically)
  - cancellation drains tracking and generation durably (attempt `Cancelled`)
    and admits the waiter
  - a tracking result for a replaced revision is refused and nothing is saved
  - shutdown releases every waiter and the board empties

  Further cases cover a generation queued behind another model, cancelled
  while queued with a durable `Cancelled` attempt and no worker, and the
  interrupted list after reopening a crash copy taken through SQLite's
  backup API (`jobs::crash_copy`), with Discard.

  `project::tests::jobs::queue` adds:
  - two pauses generating: the second records its attempt and queues, each
    is cancelled from its own row, a second job for the same pause is
    refused even after the primary slot empties while a secondary job still
    runs, the queued one is admitted when the model frees, and a retry
    is a second attempt of the same request
  - battery, memory pressure and playback holding a recorded attempt at
    admission, Run anyway, and edits throughout
  - chaos under load: the scripted worker's real child process is killed
    with SIGKILL while a second generation waits, tracking runs, a
    transcription queues and a shot scan yields. The attempt ends durably
    `Failed`, the waiting generation runs, and every edit commits within a
    five-second budget.
- **What the stubs do not exercise.** The real transcription, shot and proxy
  threads are not run here (they need the model pack and full decodes).
  Their coordinator calls are small and covered by their own replays
  (`transcript`, `shots`, `proxy-seek`). The shot scan's own six-hour
  deadline still counts time paused at checkpoints.
- **Replay `jobs`** (`cargo xtask replays --scenario jobs`) drives the panel
  through real keys:
  - the running and queued rows
  - X cancels generation and admits the queued job
  - Escape closes the panel
  - a crash copy reopens with its interrupted attempt
  - R retries it as a new attempt of the same request
  - a second crash copy is discarded durably with D
  - an injected sleep/wake interrupts running and queued AI attempts without
    restarting either; R explicitly starts a new attempt of the retained request
  - J reveals the lower interrupted row and its complete Retry/Discard buttons,
    verified against actual paint clips and opaque overlap; K returns to the first
  - on an injected battery reading, two AI jobs wait with the reason and
    Run anyway; O starts one, the other waits its turn for the model, and X
    cancels only it
  - a window close asks about the running generation; Quit when they finish
    shows the banner and a second close's Go back stays open

## Remaining

These are not implemented or not qualified:

- **Byte budgets.** Admission follows the kernel's pressure level; there is
  no per-job byte budget from measured model footprints, and the one-model
  and one-scan counts are fixed.
- **Running inference during playback.** AI picture generation has cooperative
  scheduling; physical playback interference and the other model workers remain
  unqualified.
- **Sleep.** AI pause generation has the policy above. Sleep policy for other
  model jobs and physical sleep/wake qualification remain outstanding.
- **Historical estimates.** Remaining time uses only the current run's
  measured rate.
- **Physical quit paths.** ⌘Q and Dock Quit through the native guard are
  not exercised by replays (they push a window close).
- **App-wide coordinator.** A coordinator shared across several app
  instances. Each app process has its own; proxies keep their per-user
  encoder lock.
- **Unregistered work.** Closed-project CLI commands (`generate-hold`,
  `render`, `track`) run in their own process and do not register; with the
  project open, `generate-hold` runs the app's own registered job. Thumbnail
  and waveform preparation do not register either. Thumbnails and
  waveforms already yield to the main picture and to playback through their
  own owners.
- **Physical qualification.** Physical interference measurements between
  inference, preview and export.
- **Shot scan deadline.** Time paused at checkpoints counts against the
  scan's six-hour deadline.
