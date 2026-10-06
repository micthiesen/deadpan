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

- one AI pause job per project service
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
| Transcription | Claims the model only before whisper starts; reading the sound and detecting pauses (CPU) run first and hold nothing. |
| Shot and pause detection | Before opening the Original. |

## Yield rules

A running job waits out these conditions at its own safe boundaries. Nothing
is preempted mid-step, and GPU kernels are not interrupted.

| Job | Yields while | Boundary |
| --- | --- | --- |
| Shot detection | the edit plays, a render runs, tracking or face detection runs | Between pictures (`JobHandle::checkpoint` in the scan's progress callback). Its ten-second checkpoints still save. |
| Seek proxy | the edit plays, a render runs, an AI model runs (generation or transcription), tracking or face detection runs | The existing monitor suspends the worker's process group (SIGSTOP/SIGCONT). It now combines `JobHandle::pause_reason` with the UI's busy signal and the power guard. |
| AI generation, transcription, pause detection, tracking, faces, render | nothing | These are isolated worker processes without a qualified safe pause point. §18.4 forbids promising an instantaneous AI pause. |

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
  shows "cancelled; :proxies retry builds it again".
- **Shots.** A cancelled shot scan shows Try again.

## Jobs panel

Open the panel with `:jobs` or Deadpan › Jobs…. While it is open it owns the
keyboard. Editor bindings never see these keys; IME composition is observed
and never acts.

| Key | Action |
| --- | --- |
| Up/Down, or J/K | Choose a row |
| X | Cancel the chosen job |
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

Each row is one accessible sentence, for example: `AI pause pictures · Pause:
Running · Generating pictures 100% · 0:42 elapsed`. Queued rows show their
position, what they wait for and the waiting time. Rows of an earlier session
that is still draining say "(closing project)". Progress comes from the
job's own measured stage: steps, frames or bytes. Without a measurement it
is shown indeterminate (stage only).

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

## Remaining

These are not implemented or not qualified:

- **Measured budgets.** Memory and pressure budgets measured from available
  memory (§18.4). Capacities are fixed counts, not measurements.
- **Inference during playback.** Inference is not reduced during playback.
  The workers have no safe boundary.
- **Battery, thermal and sleep policy for model jobs.** Only proxies observe
  them.
- **Measured estimates.** Historical estimates for progress.
- **Quit choice.** A quit dialog offering "keep running".
- **Queued generation per project.** More than one queued AI generation per
  project. The service still runs one generation job and refuses a second.
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
