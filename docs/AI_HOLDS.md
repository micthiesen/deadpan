# Local AI Holds

`deadpan_cli::generation` runs the complete local chain that fills a pause
(Hold) with generated pictures: conditioning, request and attempt bookkeeping,
the supervised worker, host qualification, publication, Ready, and explicit
acceptance. It replaces the developer examples as the host path; the examples
remain qualification harnesses. Generation only proposes pictures. Only
acceptance edits the project, as one undoable command.

The only provider is the development LTX-2.3 q4 MLX runtime qualified in
[`tools/model-qualification`](../tools/model-qualification/README.md). It is a
developer runtime, not a distributed or installed one.

## Library chain

The steps keep store writes on the writer thread and the long worker run on a
job thread. Every step after allocation is truthful about failure: the store
ends in Ready, Failed or Cancelled.

| Step | Thread | Function |
| --- | --- | --- |
| Conditioning | job (read-only store) | `generation::conditioning::prepare(package, revision, hold, cancelled) -> BridgeInputs` |
| Allocation | writer | `generation::attempt::allocate(&mut store, AllocateInput) -> Allocated` |
| Worker and qualification | job, no store | `generation::attempt::run_worker(&Allocated, &BridgeRuntime, progress, records, cancelled) -> WorkerRun` |
| Durable transitions | writer | `generation::attempt::record(&mut store, &Allocated, &AttemptRecord)` |
| Publication or failure | writer | `generation::attempt::finish(&mut store, &Allocated, WorkerRun) -> Finished` |
| Acceptance | writer | `generation::acceptance::accept(&mut store, &RequestId, new_revision) -> CommitOutcome` |

- `allocate` records a bridge request with the manifest hash as its context and
  `development_provider(seed)`, then begins a fresh attempt. A new request makes
  the Hold's earlier requests stale.
- `run_worker` creates a private temporary directory (`runtime.json` beside
  `worker/{inputs,outputs}`), writes the two prepared PNGs and the context
  manifest, pins the workspace and captures the inputs with
  `capture_bridge_conditioning` before launch. It launches the environment's
  Python with `-I`, a cleared environment and the offline variables through the
  shared process supervisor (30-minute deadline, 5 s cancellation grace), and
  applies the job lifecycle. Stage and terminal messages go to the caller as
  `AttemptRecord::Worker`, a host cancellation as `AttemptRecord::CancelRequested`;
  Progress stays in memory and reaches the `progress` callback. A native
  declaration that differs from the plan or provider fails the attempt. After a
  clean exit it runs `qualify_bridge` with `deadpan-media-worker` and builds the
  `BundleValidationReceipt` with admission evidence (validator `native-ffv1`,
  `bridge-3`). An error from `records` cancels the worker and fails the attempt.
- `finish` publishes the native and sampled masters, the provenance envelope
  and the three retained inputs, then records Ready. A publication failure is
  recorded as a host failure. Failures the store has not already recorded become
  `fail_generation_attempt`; cancellation becomes Cancelling then Cancelled.
- `accept` takes the request's selected Ready bundle, derives fresh asset IDs
  from the new revision (`ai-hold-<revision>-native`/`-sampled`), previews the
  edit, and builds the relevance plan with `BoundaryContextResolver` for every
  current request, so the accepted request stays `Resolved` with its own hash.
  `relevance_plan(store, before, after)` is public for other explicit writes.

## Headless commands

```sh
deadpan-cli generate-hold <project.deadpan> --hold <node-id> [--seed N]
deadpan-cli accept-hold <project.deadpan> --request <request-id>
```

`generate-hold` prints progress as JSON lines on stderr and one JSON report on
stdout: request, attempt, plan, final state, failure, per-step timings, the
Ready objects and measured spans, and the worker log tail on failure. It exits
nonzero unless the attempt is Ready. SIGINT/SIGTERM cancel cooperatively. Both
commands hold the project writer and refuse with `GenerationRefused` when the
project is open in the app; routing through the app's writer is not
implemented. Error codes: `GenerationUnavailable` (runtime missing),
`GenerationInputsUnavailable`, `GenerationRefused`, `GenerationCancelled`,
`GenerationFailed`, and store codes.

## Native app workflow

Select a pause (Hold) in Your edit. The inspector's **AI PICTURES** section and
the footer teach the actions:

| Action | Keys | Effect |
| --- | --- | --- |
| Generate | `,a`, `:generate` | Start one background job for the selected pause |
| Cancel | `:cancel-ai` (Esc never cancels) | Cancel cooperatively; the attempt ends Cancelled |
| Preview | `:preview-ai`; Esc leaves it once nothing else owns Esc | Show the candidate's pictures in the viewer at the edit cursor |
| Accept | `:accept-ai` | One undoable edit; the pause stays selected |
| Discard | `:discard-ai` | Hide the candidate for this session; nothing is written |

`project::generation` defines the requests and published state, and
`project/service/generation.rs` runs them:

- **Start** captures session, revision and Hold. The service resolves
  `BridgeRuntime::from_environment` first. A missing runtime ends the job as
  Unavailable with the runtime's own text, records nothing and starts no thread.
- One bounded job thread per project (`deadpan-ai-pause`) runs
  `conditioning::prepare` (read-only store) and then `run_worker`. Allocation,
  every `AttemptRecord`, `finish` and acceptance run on the service's writer
  thread. The job sends its records through a bounded channel the service loop
  drains every iteration, then waits for each acknowledgement. The wait has no
  timer, so a long writer task (such as a preview's six-object verification)
  cannot fail the attempt; a refused record or a stopped writer (disconnected
  channel) makes `run_worker` cancel and fail it. Progress is display-only and
  may drop a step when the queue is full. The final run has its own one-slot
  channel, so it is never dropped and its qualified workspace reaches `finish`.
- `,a` captures its session, revision and pause at the first `,a` ancestor and
  `:generate`, `:preview-ai`, `:accept-ai` and `:discard-ai` at command entry,
  including absence. Cancel, Preview and Discard bypass the editing command
  path, so Camera drafts, Repeat chains, playback and pending keys are kept.
- Allocation requires the head to still be the captured revision; an edit during
  the (about one second) conditioning fails the job with a request to generate
  again. Ordinary edits after allocation keep the request reconciled.
- Progress (stage, step k/n when the worker reports it, elapsed time) appears in
  the inspector and in a footer row that stays visible with other selections.
  The development worker reports stages only; it sends no inference steps.
- Ready candidates are read from the store, not from the job: every current
  request with a selected Ready bundle whose Hold has not accepted those
  pictures. They therefore survive reopen and reappear after an Undo of the
  acceptance. A new request for the Hold supersedes its candidate.
- **Preview** asks the store for the acceptance preview
  (`preview_generation_acceptance`, which verifies the six retained objects)
  and publishes the resulting uncommitted document as a service-issued
  `CandidatePreview`. The preview worker admits it only against the exact
  committed session and revision it was issued for, compiles its plan, refuses
  any timing change and decodes the generated sampled master through the shared
  generated-picture path. Presentation records it as a separate location
  (“Showing AI preview frame N”), so Camera cannot target it. Edits, Undo,
  session changes and a superseded candidate end the preview.
- **Accept** is the only authored change: `acceptance::accept` through the
  ordinary edit receipt, selecting the Hold and keeping the cursor.
- Stale session, revision, ticket and request identities are refused. Close,
  Open, New and shutdown are deferred while a job runs: the service cancels it,
  waits until the worker is reaped and its outcome recorded, and only then
  releases the writer, as it does for render work.

### Test seam

`project::generation::Backend::Scripted` exists only in tests and the
`ui-harness` build. It keeps conditioning, request allocation and every durable
transition real and replaces only the model worker with a deterministic script
(an unavailable runtime, progress steps, then cancellation or a worker failure).
It cannot produce Ready pictures: Ready, Preview and Accept are exercised only
with real bundles (`ai-pause-ready` replay and the opt-in service test).

### Verification

- Service tests (`project::tests::generation`): unavailable runtime records
  nothing; progress then cancellation records Cancelled; immediate cancellation
  leaves no live attempt; worker failure is recorded; stale session, revision,
  target, ticket and request are refused; Close and shutdown drain the job
  before the writer is released.
- `real_worker_generates_a_candidate_that_acceptance_commits` is ignored unless
  `DEADPAN_BRIDGE_REAL=1`. It needs `deadpan-media-worker` beside the test
  executable (for example a link in `target/debug/deps`) and accepts
  `DEADPAN_BRIDGE_REAL_PROJECT` (a scratch copy). On 2026-10-04 (M5 Max, debug)
  it ran on a copy of `interview.deadpan` with a 30-frame pause at frame 297:
  stages Preflight 0.7 s, RuntimeLoading 14.1 s, ModelLoading 15.0 s, Inference
  19.7 s, Decoding 91.9 s, Encoding 93.2 s, WorkerValidation 93.4 s, Qualifying
  93.9 s, Ready at 96.0 s; Preview and Accept passed; `/usr/bin/time -l`
  maximum RSS 14.6 GB including the reaped worker.
- UI replay: `ai-pause` (scripted worker) and `ai-pause-ready --project` (a
  copy of that accepted project). See [UI feedback](UI_FEEDBACK.md).

## Development runtime

`BridgeRuntime::from_environment` locates each part and reports every missing
one in a single error beginning "AI pauses need the development model runtime:".

| Variable | Default |
| --- | --- |
| `DEADPAN_BRIDGE_RUNTIME_SOURCE` | `~/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d759…`, else `/private/tmp/deadpan-ltx2src-3392/ltx-2-mlx-3392d759…` |
| `DEADPAN_BRIDGE_PYTHON` | `<runtime source>/.venv/bin/python3` |
| `DEADPAN_BRIDGE_MODEL_CACHE` | `~/Library/Caches/Deadpan/ltx-qualification` |
| `DEADPAN_BRIDGE_FFMPEG` / `DEADPAN_BRIDGE_FFPROBE` | `/opt/homebrew/bin/ffmpeg` / `ffprobe` |
| `DEADPAN_BRIDGE_WORKER` | `tools/model-qualification/worker.py` in the build's checkout |

`deadpan-media-worker` must sit beside the running executable. The Python path
keeps its virtual-environment symlink; resolving it would select the base
interpreter without the environment's packages. Discovery checks presence only,
including the source's `packages/` tree and both pinned model snapshots; the
worker verifies all 139 source files, the imported package roots and every
model file hash on each attempt.

macOS periodically removes old files under `/private/tmp`. On 2026-10-04 the
original checkout had lost its source tree and site-packages while its
directories remained, so the durable checkout is preferred. Recreate it with:

```sh
git clone https://github.com/dgrauet/ltx-2-mlx ~/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4
cd ~/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4
git checkout 3392d75934120b7e69eefbe55893f7ef82be92a4
UV_PYTHON_DOWNLOADS=never uv sync --frozen --python 3.12.13
```

That environment matched the recorded
[runtime inventory](../tools/model-qualification/evidence/2026-09-20-smoke/runtime-inventory.json)
exactly (38 packages, same versions; `uv.lock` and `pyproject.toml` hashes equal).

## Measured run (2026-10-04)

Apple M5 Max, 128 GB, debug build. A copy of the `interview` project (640×360,
30 fps synthetic test pattern, 595 frames) received a 30-frame (1 s) Background
Hold at frame 300 through `command` `insert_time`. Seed 1.

| Measure | Result |
| --- | --- |
| Plan | 30 project frames at 30 fps from 25 native frames at 24 fps, 768×320; boundary 1 s against 31/30 s requested |
| Conditioning (two boundary decodes, PNG) | 621 ms |
| Workspace and input capture | 20 ms |
| Worker, launch to clean exit | 100.6 s (worker-reported backend 86.6 s) |
| Qualification (FFV1 conversion and decode) | 1.42 s |
| Publication and Ready | 0.49 s |
| Command wall time | 103.3 s |
| Worker peak RSS (2 s `ps` sampling) | 13.2 GiB |
| Worker peak footprint (2 s `footprint` sampling, includes Metal) | about 21 GiB |
| `/usr/bin/time -l` maximum RSS (includes reaped worker) | 14.5 GB |

Ready objects: native FFV1 master 25 frames at 24 fps, PTS 0–1000 ms with a
41 ms last frame, span `[0, 1041)` ms, 4,910,509 bytes; sampled master 30 frames
at 30 fps, span `[0, 1000)` ms, 5,933,395 bytes; provenance 44,713 bytes; the
manifest and both PNGs. `accept-hold` committed one revision that registered
both assets and made the Hold `generated` with a Background fallback. A full
`render` of that revision verified and published 625 frames at 640×360, 30 fps
(188.9 s debug render); frames 300–329 show the generated bridge.

Two earlier attempts on the same Hold failed and were recorded as Failed: the
cleared `/private/tmp` checkout, then a canonicalized interpreter path that
bypassed the environment. Both led to the discovery changes above.

That first render showed the generated frames smaller than their neighbours,
with black bars on all sides: conditioning contained the picture inside the
2.4:1 native raster, and presentation then fitted that raster into the 16:9
canvas. The raster policy is now explicit and symmetric:

- Conditioning fits each boundary picture whole inside the project canvas's
  region of the native raster (`canvas_region`; 569×320 for 16:9), as the
  canvas presents it, on black.
- Store acceptance records that canvas on the artifact as `content_aspect`.
  The boundary-context resolver includes the canvas, so an accepted request's
  canvas is the one it was conditioned for.
- Presentation crops a generated bridge picture, centered, to the recorded
  `content_aspect` (`picture::fill_canvas_aspect`) in both the shared project
  picture path (render/export) and the app preview worker. Artifacts without
  it present uncropped. Both sides round through one integer helper,
  `picture::aspect_region`, so the region and the crop agree to the pixel,
  including odd and anamorphic canvases.

The crop removes exactly the conditioning bars. A re-render of the same
accepted revision (`render2/cropped.mp4` in the run directory) shows frame 288
filling the canvas like frame 285 before it.

## Superseding and partial allocation

Generating again for the same Hold records a new request, which makes the
previous one stale, including an unaccepted Ready candidate; there is one
candidate per Hold until variants exist. Request recording and the first
attempt are separate store writes: if beginning the attempt fails, the request
remains current with no attempt, and the next generation supersedes it.

The CLI's first SIGINT/SIGTERM cancels (also after the worker exits, before
publication); a second exits at once with status 130.

## Remaining work

Variants and a durable Discard, sound audition of the candidate (Preview is
pictures only; acceptance does not change the Hold's audio), candidate
thumbnails in the inspector, a Generate entry inside scoped Repeat/Retime
inspection, routing the headless commands through an open project's writer,
composed framing in conditioning, source/colour context qualification, native
physical-input, VoiceOver and real-window checks of the AI controls, and a
distributed runtime and model installation.
