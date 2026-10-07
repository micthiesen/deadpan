# Local AI Holds

`deadpan_cli::generation` runs the complete local chain that fills a pause
(Hold) with generated pictures: conditioning, request and attempt bookkeeping,
the supervised worker, host qualification, publication, Ready, and explicit
acceptance. It replaces the developer examples as the host path; the examples
remain qualification harnesses. Generation only proposes pictures. Only
acceptance edits the project, as one undoable command.

The only provider is the LTX-2.3 q4 MLX route qualified in
[`tools/model-qualification`](../tools/model-qualification/README.md). A
packaged `Deadpan.app` carries its private runtime and takes the weights from
the installed `ltx-2.3-q4-bridge` [model pack](MODEL_PACKS.md); development
builds may still point at a developer runtime ([Runtime](#runtime)).

## Library chain

The steps keep store writes on the writer thread and the long worker run on a
job thread. Every step after allocation is truthful about failure: the store
ends in Ready, Failed or Cancelled.

| Step | Thread | Function |
| --- | --- | --- |
| Conditioning | job (read-only store) | `generation::conditioning::prepare_scoped_with_options` captures the exact authoring address; `prepare` supplies empty Repeat ancestry |
| Allocation | writer | `generation::attempt::allocate_scoped_with_provider` records that address; `allocate_variant` adds an attempt of a current request |
| Worker and qualification | job, no store | `generation::attempt::run_worker(&Allocated, &BridgeRuntime, progress, records, cancelled) -> WorkerRun` |
| Durable transitions | writer | `generation::attempt::record(&mut store, &Allocated, &AttemptRecord)` |
| Publication or failure | writer | `generation::attempt::finish(&mut store, &Allocated, WorkerRun) -> Finished` |
| Acceptance | writer | `generation::acceptance::accept(&mut store, &RequestId, new_revision) -> CommitOutcome` |

- `allocate` records a bridge request with the manifest hash as its context and
  `development_provider(seed)`, then begins a fresh attempt. A new request makes
  that authoring scope's earlier requests stale.
- `allocate_variant` begins another attempt of a current request with its exact
  constraints, plan and conditioning inputs (it refuses inputs whose manifest
  hash, constraints or plan differ). Every attempt of a request is a seeded
  variant: attempt `n` runs with `ProviderSelection::for_attempt(n)`, the
  request's seed plus `n - 1` below 2^32 (attempt 1 uses the request's own
  seed). The worker request, its declaration, the provenance and the store's
  bundle receipt all carry that attempt's provider, and the store checks it
  against the attempt ordinal. A retry is therefore a new variant, never a
  repeat of the same seed. Seeds of different requests can coincide: variant
  `k` of a request with seed `s` uses the same seed as variant 1 of a request
  with seed `s + k - 1` (with the same inputs the pictures can match). The
  seed is provenance, not identity; the attempt and its objects identify a
  variant.
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
  `bridge-8`). An error from `records` cancels the worker and fails the attempt.
- `finish` publishes the native and sampled masters, the provenance envelope
  and the three retained inputs, then records Ready. A publication failure is
  recorded as a host failure. Failures the store has not already recorded become
  `fail_generation_attempt`; cancellation becomes Cancelling then Cancelled.
- `accept` takes the request's selected Ready bundle, derives fresh asset IDs
  from the new revision (`ai-hold-<revision>-native`/`-sampled`), previews the
  edit, and builds the relevance plan with `BoundaryContextResolver` for every
  current request, so the accepted request stays `Resolved` with its own hash.
  `relevance_plan_for_requests` consumes the store preview's mapped request addresses for explicit writes that can isolate Repeat contents.

### Candidate checks

Before Ready, the host inspects every adjacent native picture pair for gross
motion and abrupt lighting changes. Motion is block displacement measured in
the authored Hold clock, with limits following Still/Subtle/Moderate. Low
texture, ambiguous matches and motion beyond the bounded search remain
unavailable. The inspector shows this coverage; all candidates still need
audition before acceptance.

Host provenance retains the measurements, thresholds and coverage.
Previously accepted schema-3 footage remains readable and is labelled as
lacking these checks. Rejection records `OutputValidationFailed`, preserves
earlier Ready selection, and leaves the committed pause unchanged. See
[measurement and verification](qualification/bridge-quality-2026-10-07.md).
The host also compares the retained conditioning PNGs with the sampled master's
first and last pictures inside the captured presentation crop. A separate gross
endpoint guard rejects broad RGB discontinuity before Ready. Host schema 8
retains these reports and binds measurements to the crop and immutable
objects. Older schema 3/4/5/6/7 artifacts remain readable with their original evidence.
The existing Smooth/Noticeable/Jump readings remain advisory. See
[endpoint checks](qualification/bridge-endpoints-2026-10-07.md).

The separate `deadpan-track inspect-landmarks` helper runs Apple Vision face
landmarks request revision 3 over every canonical native picture and both
retained boundary PNGs. Conservative unique associations connect face boxes
and eye/nose geometry to both boundaries. Mouth aperture is measured only
within continuous native-frame segments, including faces that enter later.
A later occlusion cannot erase an earlier measured rejection. Missing, weak,
ambiguous or out-of-crop observations remain explicitly unavailable; they do
not establish identity or silence. The inspector exposes this coverage.
See [face and mouth checks](qualification/bridge-landmarks-2026-10-07.md).
An optional saved target also enables a continuous non-face region check.
`target=ID` captures that target in the exact decoded Original pictures on
both sides. Vision object tracking revision 2 follows the left PNG, every
native picture and the right PNG without restarting after loss. Gross center
or size/aspect drift over two consecutive native frames rejects the candidate.
The report retains raw boxes, confidence, exact PTS, capture identity and
recomputed coverage. Missing, lost, interpolated, weak, off-source or too-small
authored regions remain unavailable. Crop rectangles never supply a subject.
See [selected-region checks](qualification/bridge-region-2026-10-07.md).
The versioned engineering thresholds need calibration on the owner corpus
under §29.1.

### Source and colour context

Conditioning writes context manifest schema 5 (`deadpan_models::BridgeContext`,
[bundles](GENERATION_BUNDLES.md)). For each side of the pause it records what
the committed picture path showed at the origin revision: for an Original
frame, the asset, receipt, measured index ordinal and exact source PTS plus the
decoder's measured codec, pixel format, geometry, SAR, rotation, bit depth,
transfer, primaries, matrix and range; for an accepted generated Hold, its
sampled master and provenance objects; for an authored Background, authored
black. It states the model-input conversion of each picture and declares the
model colour space (full-range sRGB, BT.709 primaries, RGB). Conditioning
retains sRGB codes and converts BT.709 codes with the renderer's inverse
BT.709 OETF and sRGB encoding curve, rounded once to RGB8 before fitting.
It refuses pictures that this qualified SDR conversion does not cover
(rotated, PQ/HLG, 16-bit, linear, BT.2020 or Display P3 primaries), and
qualification and stored-evidence admission fail with a colour interpretation
mismatch when the declared model space differs from the canonical FFV1
masters. Schema-1 contexts stay admissible for already retained bundles; a
request conditioned with schema 1 gets new variants only through a new
request. `tests/bridge_conditioning.rs` checks the record against an
independent decoder and a fresh picture-path preparation. Earlier retained
inputs explicitly marked `rec709_codes_as_srgb` remain readable and labelled
as approximate; new inputs record `rec709_to_srgb`. The model's own colour
handling remains a worker claim.

## Repeat and Retime scopes

The native inspector captures the selected Hold and every explicit Repeat
Default or stable Play choice when a command begins. Generate fills the full
authored Hold duration before outer Repeat/Retime sampling. Acceptance in one
play isolates only that play in the same undoable transaction; Default changes
the shared definition while retaining existing overrides. Several selected
plays currently refuse with an explicit message.

Conditioning uses the highest ordinary Sequence containing the Hold below its
nearest Repeat/Retime ancestor, or the project root. For local Hold interval
`[s,e)`, the boundary samples are exactly `s - 1/2` and `e + 1/2` in that
definition clock. A missing boundary refuses generation; an outer neighbor or
an arbitrary visible play cannot substitute for it. Context 5 retains the
project, origin revision, definition and rational positions. The worker and
stored admission validate that origin and the exact duration between samples.
Historical contexts 2–4 retain their Project-clock interpretation.

A request keeps its immutable worker binding and origin authoring address.
Its current address follows only isolation maps rederived from validated
history commands. Each authoring scope has an independent monotonic request
clock, even when several scopes initially share one Hold node. Undo/Redo moves
addresses through those same maps without reviving stale requests or attempts.
Schema 70 deliberately rejects earlier unused development packages.

Preview and audition separately capture a concrete occurrence and its root
frame window. Changing scoped selection cancels pending previews and clears
cached comparisons. The inspector states the intrinsic picture count and that
join readings describe the definition before outer sampling. Editorial Camera,
captions and gain do not change the raw conditioning pictures.

For the CLI, `--scope` takes a strict `ScopedNodeTarget` JSON value, for example:

```json
{"node":"pause","repeats":[{"repeat":"repeat","branch":{"type":"play","iteration":{"allocation":"repeat-revision","ordinal":1}}}]}
```

The stable iteration comes from the observed document. Omitted scope means
empty Repeat ancestry; it never adopts the app's current selection. Live job
status includes the captured scope.

## Lengthening accepted pauses

Shortening an accepted pause, or extending it within the retained sampling
interval, reuses its pictures. Extending beyond that interval commits the new
duration and captured fallback immediately. The same transaction records a
replacement preparation with its exact authoring scope and prior generation
controls. Typed commands, native duration entry and semantic compounds share
this path. A copied accepted pause recovers its controls from verified retained
provenance when no original request exists.

The app processes queued preparations through its existing single AI job thread.
Boundary decoding and provenance reads stay off the writer. Fulfilment records
the prepared request and its first attempt atomically, only while the claim and
revision remain current. Unrelated edits preserve the intent and prepare again
against the new revision; changed boundaries, deleted pauses, Undo and explicit
discard invalidate late replies. Redo creates a fresh preparation. Ready pictures
still require explicit acceptance; background generation never changes the
committed fallback.

A missing runtime, model or conditioning input leaves an Unavailable entry in
Jobs. **R** retries the selected preparation and **D** discards it without
changing the saved timing. Closing during preparation leaves an Interrupted
entry for explicit retry. Work that has already recorded an attempt uses the
ordinary attempt recovery controls. Installing models remains an explicit
action with license acceptance.

The queue admits at most 4,096 unfinished preparations and 32 MiB of reserved
record space. Each admission includes space for the longest later failure
message, revision and isolated identities, so status updates cannot exhaust
the budget. A new duration edit can displace older waiting or retryable work
before claimed work; the saved edit and Jobs report that cancellation. The
latest 256 terminal records remain directly readable when space permits.
Older results compact to immutable birth and controls proofs for history
validation and Redo; `--id` then returns no full record. Historical documents
continue to retain their generated media.

`ai-replacements` lists the unfinished queue in pages of 64. `--id` reads a
particular preparation, including a recent terminal result. With the project
closed, `--run` owns the writer and processes one queued item synchronously;
`--limit 1-16` bounds a batch. SIGINT/SIGTERM cancel cooperatively. The open app
owns its automatic queue; status, retry and discard work through the shared
headless path. These operations never accept a candidate.

```sh
deadpan-cli ai-replacements <project.deadpan>
deadpan-cli ai-replacements <project.deadpan> --id <preparation-id>
deadpan-cli ai-replacements <project.deadpan> --retry <preparation-id> --expected <revision>
deadpan-cli ai-replacements <project.deadpan> --discard <preparation-id> --sequence <observed-sequence>
deadpan-cli ai-replacements <project.deadpan> --run --limit 1
```

## Headless commands

```sh
deadpan-cli generate-hold <project.deadpan> --hold <node-id> [--scope JSON] [--seed N] [--variants 1-4] [--motion still|subtle|moderate] [--instructions TEXT] [--another]
deadpan-cli accept-hold <project.deadpan> --request <request-id> [--attempt <attempt-id>]
```

`generate-hold` prints progress as JSON lines on stderr and one JSON report on
stdout. With the project closed it holds the writer for the whole job: the
first variant records a new request (or, with `--another`, joins the Hold's
current request, conditioned from that request's own revision), and each
further variant is another attempt of the same request. The report describes
the last attempt (request, attempt and ordinal, seed, plan, final state,
failure, per-step timings, the Ready objects and measured spans, the worker log
tail on failure) and lists every attempt under `variants`. It stops at the
first attempt that is not Ready and exits nonzero unless the last one is Ready.
SIGINT/SIGTERM cancel cooperatively. Without `--seed` a new request gets a
random seed below 2^32, as in the app; `--seed` with `--another` is refused,
because variants of an existing request derive their seeds from its own.
`accept-hold` names the head revision it observed before sending, so an edit
made meanwhile refuses with `RevisionConflict`; `--attempt` selects that Ready
variant before accepting, otherwise the request's selected variant is
accepted.

When the app has the project open, both commands route through its
[authenticated live endpoint](LIVE_PROJECT.md) instead of refusing.
`generate-hold` starts the app's own AI job (the same job as `:generate N`,
shown in the app), polls it every 250 ms, prints its stage, variant and steps
on stderr, and cancels exactly that job on SIGINT/SIGTERM or after one worker
deadline per variant, then waits up to five minutes for the app to confirm.
Its report has `routed: "live_project"`, the job, request, ready count,
outcome and note; per-attempt timings and objects stay in the app and the
store. The app keeps a concluded live job's result (up to eight, ten minutes)
even after the UI starts another job, until the CLI releases it after writing
its report. A failed observation reports `HostOutcomeUnknown` with the job
number and the advice to cancel it in the app; nothing is replayed. The app
decides between a new request and another variant from the pause's boundary
pictures, so `--another` is implied when they are unchanged; an explicit
`--seed` that would land on an existing request ends the job refused rather
than ignored. `accept-hold` sends one `AcceptHold` command that the
owner runs in a single transaction. A lost reply reports `HostOutcomeUnknown`
and is never replayed. Error codes: `GenerationUnavailable` (runtime missing),
`GenerationInputsUnavailable`, `GenerationRefused`, `GenerationCancelled`,
`GenerationFailed`, `GenerationUnknown` (the observed job is no longer the
app's current one), host codes, and store codes.

## Generation controls

The inspector shows requested motion, the selected region target and optional
plain-language guidance. **Generation controls…** opens a command prefilled with the current choices;
Enter generates and Escape closes it without starting work. For example:

```text
:generate 2 motion=subtle target=hand text=Keep the hands still.
```

Motion is `still`, `subtle` or `moderate`. `text=` consumes the rest of the
command as literal model input, including spaces. Guidance must be nonblank,
at most 512 UTF-8 bytes, and contain no control characters. Explicit controls
replace the previous choices; `:generate motion=still` also clears old text.
`target=ID` names a saved attention target; `target=none` clears it. Omitted
target retains the captured choice, including its absence. The first request
has no target. A target correction or removal makes its request stale.
Bare `,a`, `:generate N` and Retry retain the current request's choices.
Changed controls start a new request, so old candidates are never relabelled.
Timing, pause sound and acceptance remain unchanged. These are requests to the
model; review its result before accepting.

The closed-project CLI accepts `--motion`, `--target ID|none` and `--instructions`; a fresh request
defaults to Still without guidance. `--another` retains the stored controls
and refuses those flags or `--seed`. Through an open app, absent controls
retain the current choices as native Generate does. Reports and `ai-variants`
include the captured options. Failed or cancelled recorded requests retain
them across reopening.

Adapter `ltx-mlx` `0.15.8+deadpan5` uses `deadpan-hold-2`: a short locked-camera,
preserved-identity/composition, no-speech/no-new-objects base, one motion
sentence, and optional literal guidance. It uses no prompt enhancer. The exact
prompt, version and token count are retained in worker provenance. Gemma's
actual token count must fit 1,024; overlong prompts fail instead of truncating.
The same prompt reaches the pipeline. Model bytes are unchanged. Schema-4
pack manifests declare the three motion values and the guidance byte limit;
signed data updates cannot expand the shipped adapter's capabilities.

## Native app workflow

Select a pause (Hold) in Your edit. The inspector's **AI PICTURES** section and
the footer teach the actions:

| Action | Keys | Effect |
| --- | --- | --- |
| Generate | `,a`, `:generate`, Generate another | Start one background job for the selected pause that adds one variant |
| Generate several | `:generate N` (1 to 4) | One job that generates N variants, one attempt after another |
| Motion and guidance | `:generate [N] motion=subtle text=Keep the hands still.` | Replace the generation controls and generate; `text=` is optional and comes last |
| Cancel | `:cancel-ai`, or X in the Jobs panel (`:jobs`) (Esc never cancels) | Cancel cooperatively; the running attempt ends Cancelled, earlier variants stay Ready |
| Choose | `:next-ai`, `:prev-ai`, `:pick-ai N`, a click on a variant row | Choose which variant Preview, Audition and Accept use; stored as the request's selection, never an edit |
| Preview | `:preview-ai`; Esc leaves it once nothing else owns Esc | Show the chosen variant's pictures in the viewer at the edit cursor |
| Audition | `:audition-ai`, Audition; Space while previewing | Play the pause with the chosen variant's pictures and the pause's own sound |
| Compare | `,x`, `:compare-ai [before\|N]`, Compare before / after; `,n` next variant | Switch the viewer and audition between the pause as committed (Before) and a variant at the same frame and heard sample |
| Accept | `:accept-ai` | One undoable edit with the chosen variant; the pause stays selected |
| Discard | `:discard-ai` | Durably discard the chosen variant; not undoable, the pause is unchanged |
| Install the model pack | Install AI models…, `:models` | Open the Models panel on the bridge pack; shown when it is not installed |

`,a` keeps its single-variant meaning. Comparison adds `,x` (`ai.compare`)
and `,n` (`ai.next`) to the comma family with the same contract (Normal Edit
only, no count, no key repeat, yielding to text and composition); see the
[compatibility record](KEYBINDING_COMPATIBILITY.md). The rest of the workflow
is command-only. Headless, `ai-variants` lists the offered variants (with
`--joins`, the comparison's join readings) and `select-hold`, `keep-hold`,
`discard-hold` and `dismiss-attempt` make the same choices, on the app's
writer when it has the project open ([headless](HEADLESS.md#choosing-keeping-and-discarding-variants)).

`project::generation` defines the requests and published state, and
`project/service/generation.rs` runs them:

- **Start** captures session, revision, Hold and the variant count. The service resolves
  `BridgeRuntime::from_environment` first. A missing runtime ends the job as
  Unavailable with the runtime's own text, records nothing and starts no thread.
- **Queueing.** The job registers with the [job coordinator](JOBS.md). Its
  model run needs the single AI model slot: while transcription (or another
  model) holds it, the job conditions and records its attempt at once, then
  shows `Queued for the AI model` until admitted. Each attempt holds the slot
  only while its worker runs and releases it before publication. After
  waiting, the job rereads its request; if edits made it stale, the model is
  not loaded and the attempt ends Cancelled with a note. Edits continue meanwhile;
  `:cancel-ai` or the Jobs panel cancels a queued job, whose attempt ends
  Cancelled without a worker. After a crash, the Jobs panel lists the
  interrupted attempt with Retry (a new variant through `:generate`) and
  Discard.
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
- **Variants.** When the conditioned manifest, constraints and plan equal the
  Hold's current bridge request, the job adds attempts to that request
  (`allocate_variant`); otherwise its first attempt records a new request,
  which makes the earlier request and all its variants stale. After each Ready
  attempt the writer finishes it (publication and Ready) and allocates the next
  variant from the same inputs until the count is reached; a failure,
  cancellation or session change stops the job. The job reports which variant
  runs (`Variant 2 of 3`, footer `variant 2/3`) and how many are Ready. The job
  thread receives one allocation per attempt and returns when the writer drops
  its allocation channel. Earlier Ready variants are never lost to a later
  outcome: if the next variant cannot start (for example the pause changed
  and the request went stale), the job ends Ready with a note naming why; if a
  later variant fails or the job is cancelled, it ends Failed or Cancelled with
  a note counting the Ready variants kept. Notes appear in the inspector, the
  status message and live-endpoint statuses.
- Progress (stage, step k/n when the worker reports it, elapsed time) appears in
  the inspector and in a footer row that stays visible with other selections.
  The development worker reports stages only; it sends no inference steps.
- Ready candidates are read from the store, not from the job: for every current
  bridge request, its Ready attempts whose bundle is Present and whose sampled
  master is not the Hold's accepted one, oldest first. The chosen variant is
  the store's selection when it is among them, otherwise the newest. They
  survive reopen and reappear after an Undo of the acceptance. A new request
  for the Hold makes all its earlier variants stale; Undo never revives them.
- **Choose** writes the request's operational selection
  (`select_generation_bundle_variant`); store acceptance admits only the
  selected attempt, so Preview and Accept select their captured variant first.
  Choosing while previewing previews the newly chosen variant.
- The inspector lists each variant with a thumbnail of its sampled master's
  middle picture, its number, seed and state (chosen, showing). The dedicated
  thumbnail worker reads the object through the workspace's generated-media
  handle (BLAKE3-verified snapshot), decodes it with `open_candidate_master`
  (checks the receipt's raster, frame count and canonical FFV1 sRGB
  interpretation) and crops the conditioning letterbox to the canvas like
  presentation. Thumbnails are keyed by attempt and the request's origin
  revision, so edits do not re-decode them; they never touch the edit's
  picture path.
- **Preview** asks the store for the acceptance preview
  (`preview_generation_acceptance`, which verifies the six retained objects)
  and publishes the resulting uncommitted document as a service-issued
  `CandidatePreview`. The preview worker admits it only against the exact
  committed session and revision it was issued for, compiles its plan, refuses
  any timing change and decodes the generated sampled master through the shared
  generated-picture path. Presentation records it as a separate location
  (“Showing AI preview frame N”), so Camera cannot target it. Edits, Undo,
  session changes, a superseded candidate and a discarded or no longer offered
  variant end the preview.
- **Audition.** The same Preview reply carries the proposed document admitted
  for playback with `deadpan_playback::Snapshot::proposed_generated` against
  the workspace's committed snapshot: it may add only video-only assets with no
  source qualification (the two generated masters) and keeps every base asset
  and its qualified source evidence, so its audio is admitted from exactly the
  committed revision's sources. Accepting pictures leaves the pause's audio
  policy (silence, room tone, tail) unchanged, so this is the sound the edit
  will have. While previewing, playback of Your edit (Space, Shift+Space,
  `:audition-ai`, the Audition button) uses that snapshot, with its own
  `Proposed` content identity, and every heard frame inside the pause shows the
  candidate's picture through the preview worker. The preview's `Proposed`
  draft identity comes from the workspace's single monotonic proposal counter
  that Gain, Trim, Slip and Splice drafts also use (the UI sends it with the
  Preview command), so a preview never shares a playback identity or cache
  entry with another draft on the same revision. `:audition-ai` loops the
  pause with the ordinary 500 ms lead-in and 750 ms follow-through
  (`:audition-context`), previewing the chosen variant first when needed; issued
  while that loop plays (captured before command entry pauses it), it pauses.
  Choosing another variant during the loop restarts it with the new pictures.
  Leaving the preview stops its audition. Nothing is saved.
- **Comparing variants.** <a id="comparing-variants"></a> `,x` (or
  `:compare-ai`) switches between the pause as committed, *Before* (its
  freeze fallback, or pictures accepted earlier), and the chosen variant;
  without a preview it previews the chosen variant first. `,n`, `:next-ai`,
  `:prev-ai`, `:pick-ai N` and `:compare-ai N` show another variant;
  `:compare-ai before` shows Before. Every switch keeps the edit cursor, so
  the viewer shows the same frame. A playing audition (`:audition-ai` or
  Space while comparing) stops and restarts the newly shown content in the
  same window from the exact content sample heard at the switch; a paused one
  keeps its exact sample (`transport::Resume::retarget`), and Space resumes
  there. Before plays the committed snapshot; a variant plays its proposed
  acceptance snapshot, so both carry the pause's own, identical sound and only
  the pictures change. The previously shown variant's admitted preview stays
  ready (up to eight, same request, session and base revision), so switching
  back is immediate and only writes the store's selection; a variant not yet
  admitted is prepared by the service first while the current audition keeps
  playing, then continues from the sample heard when it arrives. Edits, Undo,
  session changes and a discarded or no longer offered variant drop retained
  previews; Esc leaves the comparison. The footer names the state (`AI
  COMPARE · BEFORE · NOT SAVED` or the variant) with the `,x`/`,n` hints.
  Choose, Accept, Keep and Discard act on the variant shown (or being
  prepared), not on a store selection still in flight, so two `,n` in one
  input batch advance twice. Only one selection write is in flight; a newer
  switch waits and is sent when it is answered. If the store refuses the
  selection, the view returns to what was shown before and says why.
  The comparison is a toggle at one frame, not a split screen: both pictures
  are never on screen at once.
- **Discard** removes the chosen variant from the list for good with one store
  transaction (`discard_generation_bundle_variant`): its bundle becomes
  Evicted, so it is never offered again, also after reopening, and Undo does
  not restore it; only if it was the request's selection does the newest
  other present Ready variant become selected, otherwise the selection is
  kept. Its files stay under `Media/Generated` until a
  [storage cleanup](STORAGE.md) (the automatic pass below, or `:storage`,
  then P and R) removes them, once no retained revision or live receipt names
  them and the grace period has passed; stale variants of a superseded request
  are removable the same way.
- **Retention.** Offered variants follow a visible retention policy (§19.2,
  [storage](STORAGE.md#retention-of-unaccepted-ai-variants)). A variant that is
  neither kept (`:keep-ai`, Keep variant, `GenerationOperation::Keep`,
  `deadpan-cli keep-hold`), picked by the person (Choose, Preview, `select-hold`
  or `accept-hold --attempt`; protection moves only with another pick or a discard), its
  request's chosen variant, nor accepted stops being offered 7 days after it
  became Ready; each row shows `kept`, `picked by you` or `expires in N days`,
  and the service warns when a newer variant takes the selection from one only
  that selection protected. The service checks when idle after opening and
  every 6 hours (deferred while an import, render, AI pause, tracking job or
  backup runs); a clock behind recorded times or more than one retention
  period past the last check is treated as a clock anomaly that expires and
  removes nothing until it is confirmed: after a long gap since the last check, the Storage panel's E shows which variants would stop being offered (count and bytes, planned off the writer) and a second E confirms the clock, as `project storage --confirm-clock` (or `--clean`) does; a clock behind the project's records is shown but never confirmed, and one check
  expires at most 32 variants. Each check expires due variants (recorded as
  `expired`, separately from `discarded`) and then removes their unreferenced
  `Media/Generated` masters through the reference-tracked cleanup after the
  24-hour grace period. Keep is operational, durable across reopen and not
  undoable. Accepted media, including media history names after Undo, is
  never expired or removed. The Storage panel's AI VARIANTS rows and `project
  storage` show the period, offered, kept, chosen and expiring counts, the
  next expiry, discarded and expired variants awaiting cleanup with their
  bytes, and the last automatic pass.
- **Colour.** The chosen variant's row notes how its conditioning colour
  reached the model (`ConditioningColour::describe` of the retained
  manifest), including BT.709 converted to sRGB for new inputs or the stated
  approximation of older retained inputs; `generate-hold` reports it as `colour`.
- **Joins.** Each variant row also shows an advisory reading of its two joins
  with the Original (`joins smooth / jump`; the hover gives both mean RGB
  differences). `generation::joins::measure_request_joins` compares the
  committed pictures at f-1 and f+N (decoded at the request's origin revision)
  with the sampled master's first and last frames, cropped as presentation
  crops them, in one comparison region shared with `generate-hold`, on a
  background thread, with both sides converted to sRGB before fitting, one
  variant at a time and at most once per session
  (cancelled when the session ends; a panic or failure records an error
  instead of retrying) (classes below
  6 and 20 out of 255; the thresholds are uncalibrated). It is a §12.5
  heuristic only: it never accepts, rejects or orders a variant.
  `generate-hold` reports the same reading per Ready attempt as `ready.joins`.
  The joins themselves are hard cuts at exact frames; see
  [source joins](GENERATED_HOLDS.md#source-joins).
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
per attempt: an unavailable runtime, progress steps, then cancellation, a
worker failure, or Ready. The Ready ending runs
`generation::attempt::synthetic`: it writes native footage that blends the
attempt's own two conditioning pictures with a band whose colour follows the
seed, encodes it like the development worker's lossless RGB intermediate with
an external `ffmpeg` (`DEADPAN_BRIDGE_FFMPEG`, else the Homebrew path), and
writes a schema-2 worker provenance whose claims name the synthetic origin
(zero revisions, `backend: synthetic`). Host qualification by
`deadpan-media-worker`, publication of all six objects, Ready, preview,
audition admission, acceptance and discard are then production code. Without
`ffmpeg` or the media worker the attempt fails and says so, and the tests and
the `ai-variants` replay that need it report a skip. It is compiled only for
`deadpan-cli` tests and its `synthetic-worker` feature, which the app enables
for its tests (dev-dependency) and its `ui-harness` build; the shipped CLI and
app do not contain it, and no host offers it as a provider. The CLI's
`offline_portable` integration test declares the feature as required, so run
it alone with `cargo test -p deadpan-cli --features synthetic-worker --test
offline_portable`; `cargo xtask gate` enables it for the workspace runs.

### Verification

- Service tests (`project::tests::generation`): unavailable runtime records
  nothing; progress then cancellation records Cancelled; immediate cancellation
  leaves no live attempt; worker failure is recorded; stale session, revision,
  target, ticket, request, attempt and variant count are refused; Close and
  shutdown drain the job before the writer is released. With store fixtures:
  three variants are listed oldest first with distinct seeds and masters, the
  newest chosen; choosing writes only the store selection; previewing another
  variant selects it and carries a `Proposed` audition snapshot of the same
  document; discarding the chosen variant chooses the newest remaining one, is
  refused for reuse and survives reopening; lengthening the pause makes every
  variant stale and Undo does not revive them. With the synthetic worker:
  `:generate 2` yields two Ready variants of one request with consecutive seeds
  and different pictures, accepting the first keeps the second offered, and a
  later Generate adds a third attempt to the same request. Live-endpoint tests
  start, observe, refuse a stale revision, cancel exactly the observed job and
  accept a variant through `AcceptHold`.
- Store (`generation_bundles::seeded_variants_share_one_request_and_discard_is_durable`):
  attempts carry seeds 7, 8, …; a declaration repeating the request's seed is
  refused for attempt 3; acceptance admits only the selected variant; eviction
  survives reopening; a new request makes all variants stale.
- CLI: `synthetic_variants_publish_distinct_ready_bundles_for_one_request`
  (real qualification of two synthetic variants) and the
  `live_generation` integration test (the actual `deadpan-cli` binary routing
  `generate-hold --variants 2 --seed 9` and `accept-hold --attempt` to an
  authenticated owner).
- Playback: `generated_acceptance_proposal_adds_only_video_assets_and_keeps_base_audio`
  admits the acceptance document, refuses added audio or changed base assets,
  and reads the same limited PCM as the base.
- `real_worker_generates_a_candidate_that_acceptance_commits` is ignored unless
  `DEADPAN_BRIDGE_REAL=1`. It needs `deadpan-media-worker` beside the test
  executable (for example a link in `target/debug/deps`) and accepts
  `DEADPAN_BRIDGE_REAL_PROJECT` (a scratch copy). On 2026-10-04 (M5 Max, debug)
  it ran on a copy of `interview.deadpan` with a 30-frame pause at frame 297:
  stages Preflight 0.7 s, RuntimeLoading 14.1 s, ModelLoading 15.0 s, Inference
  19.7 s, Decoding 91.9 s, Encoding 93.2 s, WorkerValidation 93.4 s, Qualifying
  93.9 s, Ready at 96.0 s; Preview and Accept passed; `/usr/bin/time -l`
  maximum RSS 14.6 GB including the reaped worker.
- On 2026-10-05 (M5 Max, debug, same opt-in test on the `cfr-bframes`
  fixture with a 30-frame pause) `:generate 2` ran the real worker twice for
  one request: variant 1 Ready at 81.9 s (inference 18.1–79.7 s), variant 2
  started at 83.5 s, reloaded the runtime and model in its own worker process
  (99.1–104.2 s) and was Ready at 173.3 s. Seeds 1359878095 and 1359878096
  produced different sampled masters (`a953f79e…`, `68cd3e4f…`). Preview and
  Accept of variant 1 passed and variant 2 stayed offered. `/usr/bin/time -l`
  maximum RSS 14.5 GB. Generated pictures were not inspected for quality.
- `ai-compare` replay (scripted Ready variants through real qualification,
  simulated delivery): `,x` previews and toggles Before/variant at one frame
  with the committed picture shown for Before; while auditioning, `,x` and
  `,n` restart Before, the variant, a newly prepared variant and a retained
  one from the exact injected heard sample in the same window; a paused
  switch retargets the exact sample and Space resumes Before there; Esc leaves
  without an edit. Navigation tests cover `,x`/`,n` routing, counts, domains,
  text/IME and `:compare-ai` parsing.
- On 2026-10-06 (M5 Max, debug, the same opt-in test on `cfr-bframes`, 30
  frames) the real worker accepted the schema-2 colour-context manifest:
  variant 1 Ready at 85.5 s, variant 2 at 184.8 s (seeds 1262662453 and
  1262662454, sampled masters `fee483c3…` and `411a04c0…`); Preview and
  Accept of variant 1 passed; maximum RSS 14.5 GB. Pictures were not
  inspected for quality.
- UI replay: `ai-pause` (scripted worker), `ai-variants` (scripted Ready
  variants through real qualification: list with thumbnails, choose, preview,
  audition with simulated delivery, accept, durable discard, Undo) and
  `ai-pause-ready --project` (a copy of a real accepted project). See
  [UI feedback](UI_FEEDBACK.md).

## Runtime

`BridgeRuntime::from_environment` resolves the runtime for the running
executable (`generation::runtime::lookup`):

| Lookup | When | Python, LTX source, worker, ffmpeg/ffprobe | Model data |
| --- | --- | --- | --- |
| Bundled | Packaged `Deadpan.app` | `Contents/Resources/ai-runtime/{python/bin/python3.12, ltx-2-mlx, worker/worker.py, bin/ffmpeg, bin/ffprobe}` | The installed `ltx-2.3-q4-bridge` pack in the models root |
| PackagedExplicit | Packaged app with `DEADPAN_DEVELOPER_BRIDGE=1` | Explicitly set `DEADPAN_BRIDGE_*` only | `DEADPAN_BRIDGE_MODEL_CACHE` |
| Development | Cargo builds and developer wrappers | `DEADPAN_BRIDGE_*`, then development defaults (below) | `DEADPAN_BRIDGE_MODEL_CACHE`, else the installed pack, else the qualification cache |

The bundled lookup reads no environment variable, default, Homebrew path or
checkout. It also requires the macOS version the runtime's MLX build targets
(`runtime.json` `minimum_macos`, currently 26.0; the macOS 15 MLX build was
measured 2.2 times slower, see [dependencies](DEPENDENCIES.md#private-ai-runtime)). Its errors begin "AI pauses are not ready:"; when only the pack is
missing the error says so (`needs_model_pack`) and names its size and how to
install it, which the inspector turns into an install offer. A damaged bundle
names each missing part. The bundled runtime is described in
[Packaging](PACKAGING.md#ai-runtime); the worker it runs is the same adapter,
launched `python -I -B` (no user site, no bytecode writes into the signed
bundle) with a cleared environment and the offline variables. `doctor` reports
the lookup, the bundled runtime's `runtime.json` identity, the resolved paths,
readiness and anything missing.

`BridgeRuntime::check` runs `worker.py --check`, the bridge pack's smoke test:
it verifies the 139 pinned source files, imports the pipeline and Gemma
encoder modules from them, runs a Metal calculation, checks every receipt
file's size and parses every safetensors header, without inference.

### Development runtime

Development builds locate each part and report every missing one in a single
error beginning "AI pauses need the development model runtime:".

| Variable | Default |
| --- | --- |
| `DEADPAN_BRIDGE_RUNTIME_SOURCE` | `~/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d759…`, else `/private/tmp/deadpan-ltx2src-3392/ltx-2-mlx-3392d759…` |
| `DEADPAN_BRIDGE_PYTHON` | `<runtime source>/.venv/bin/python3` |
| `DEADPAN_BRIDGE_MODEL_CACHE` | the installed bridge pack, else `~/Library/Caches/Deadpan/ltx-qualification` |
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

Generating again for the same authoring scope adds variants to its current request while
the pause's boundary pictures are unchanged; when they changed, the request is
already stale and generation records a new one. Request recording and the
first attempt are separate store writes: if beginning the attempt fails, the
request remains current with no attempt, and the next generation adds its
first variant to it.

The CLI's first SIGINT/SIGTERM cancels (also after the worker exits, before
publication); a second exits at once with status 130.

## Remaining work

Generation at definition edges, multi-play generation/acceptance and calibrated
join thresholds remain open. A dormant definition can be generated and accepted
when both local boundary pictures exist; viewer preview requires a visible
concrete occurrence.

Listening, physical input, VoiceOver speech, real-person quality review and
the §13.4 corpus, and a clean second Mac are To verify (owner) under §29.1.
Native installation, generation, acceptance, restart and Render have passed
on this Mac; see [installation](qualification/native-install-2026-10-06.md).
Developer ID signing and notarization are outside the personal-app scope.

Possible refinements include split-screen comparison, reversible Discard and
a configurable retention period. The specification requires keyboard variant
comparison and a visible retention policy, which the same-frame comparison
and displayed seven-day policy already provide. Conditioning deliberately
precedes editorial zoom as §12.4 requires; composing that zoom into generated
pixels would prevent the accepted artifact from supporting later reframing.
