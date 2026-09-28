# UI feedback loop

Use application replay, inspected images and measured response times to improve
Deadpan's actual interface. The goal is a clear picture, predictable controls and
fast response to keyboard and mouse input. There is no combined UX score: a
correct action, a readable screen and a fast response are separate observations.

The [specification](spec/DEADPAN_SPEC.md), [design targets](design/README.md) and
[native workspace contract](NATIVE_WORKSPACE.md) remain authoritative. The
[interaction review](INTERACTION_REVIEW.md) records concrete improvement ideas.

## Current status

The native gain increment adds the `gain` replay and an exact envelope/mute
editor with revision-bound Before/Draft proposals. The scenario uses the real
project service, qualified source evidence and durable gain commands, with
typed delivery updates injected for comparison playback. It covers counted and
absolute trim, true mute, draft buffering and coalescing, captured-target
rejection, Apply/undo, field/IME routing, Tab containment, stale delivery,
pending-text Pause and retained picture on unchanged Cancel. Final focused run
`corrected-visual-gain` passes 266 gain checks plus one Kestrel check covering 5,456
routing cases, with 2,746 semantic frames and 123 captures. Four complete
populated Tab/Shift+Tab circuits verify 136 focused controls' paint and hit clips
at 960×640 and 1280×820 without wheel assistance. The graph captures were
compared with the gain board. On the final source, all 298 app/harness tests and
263 base-app tests pass in their respective configurations, along with strict
workspace/all-target lint in both configurations.

The screenshot-budget warning only limits intermediate images; semantic checks
and named captures continue. The earlier full visual suite retains its 1,123
checks and two failures. Corrected follow-ups pass 81 workspace, 103 room-tone,
266 gain and 16 retime checks, each with an independent passing Kestrel check.
The room-tone follow-up checks inspector gain controls through AccessKit Focus;
Normal Tab still cycles panes. Picture containment allows only 0.01 physical
pixels of numerical residue; text/control clipping remains strict.

This scenario does not prepare PCM, open an output device or establish listening
quality. A separate native CUA pass exercised exact trim/key editing, keyboard
focus reveal and cancellation, with byte-identical before/after project dumps.
It does not certify physical keyboard layouts, OS IME, VoiceOver or listening.
The separate release replay passes all 2,156 checks. Warm navigation, Repeat
and silent Hold picture p95 values are 6.356, 9.438 and 10.769 ms; 10,000-beat
navigation CPU p95 is 1.792 ms. These are small-fixture offscreen measurements,
with all original failed visual invocations retained separately. See the
[gain integration contract](GAIN_EDITOR_DESIGN.md) and
[native-gain qualification](qualification/native-gain-2026-09-28.md) for final
gates, source identities and native/performance limits.

The [sound allowance increment](qualification/sound-allowances-2026-09-27.md)
extends `sound-placement` with one sound's permission in a concrete pause,
including scoped pointer/command edits, durable undo, gap rejection, delayed
targets and native text composition. Replay found Enter blurring the command
field during IME; the field now leaves Enter to the app router. Visual review
replaced database IDs with readable pause/play context. The final complete
Metal replay passed 819 checks, including 190 sound-placement checks, with
permission and revoke controls verified at both supported window sizes.
The separate release replay passed 1,789 checks. Warm navigation, Repeat and
silent-freeze picture p95 values were 1.505, 1.660 and 2.360 ms on the small
fixture; 10,000-beat structural navigation CPU p95 was 0.332 ms.
The qualification retains the failed replay and separates simulated IME from
physical native input acceptance.

The [native sound-placement increment](qualification/native-sound-placement-2026-09-27.md)
adds `sound-placement`, with real audio-only qualification, persistent root-event
commands, separate catalog/event/beat selection, exact frame movement and native
sample entry. It covers late-completion target guards, pane focus, both window
sizes and routed timing restrictions. The record separates painted interaction
evidence from PCM, device, physical keyboard and listening acceptance.
The final complete visual run passed 775 checks; the separate release run
passed 1,745. Warm navigation, Repeat and Hold picture p95 values were 1.462,
1.612 and 2.067 ms on the retained small fixture. The minimum stopped sound
workspace retains a 125-point picture, with separate active-playback paint checks.

The [2026-09-27 layout qualification](qualification/workspace-layout-2026-09-27.md)
records restored Metal access, the expanded fourteen-scenario coverage and
focused follow-ups. It retains failed development runs as well as passing
results. The new moment, Original audition, sound audition and Retime scenarios
have now executed with actual paint. Camera visibility, command-hint clipping,
first-frame errors and sound-control clipping are fixed. The subsequent
[Repeat increment](qualification/repeat-input-2026-09-27.md) adds a bounded queue
for explicit wraps, matching completion guards, separate undo and visible
cancellation/overflow counts. Its release run passes navigation p95 4.91 ms, Repeat 6.76 ms and Hold 8.98 ms. Other busy edits remain interaction work.
The [footer increment](qualification/footer-layout-2026-09-27.md) adds same-frame
command closure and bottom anchoring, complete shortcut-pair wrapping, and
combined resize/text/cancellation checks. It retains one external update read
per frame across layout retries.
The layout release measurements pass warm
navigation (4.67 ms p95), Repeat (6.85 ms) and Hold (9.11 ms) against unchanged
targets. The earlier records below retain their original run boundaries; they
are not the current access or performance status.

The optional `ui-harness` feature provides `--ui-check`. It drives production
`DeadpanApp::ui` through egui events, real project/media services and Metal, using
private disposable projects. The initial selected file path is scripted; it does
not open a native picker or use the user's Documents directory. The complete
2026-09-26 visual run passed all nine scenarios and the shortcut audit. Its release
run passed navigation and large-project budgets but failed the edit-to-picture
budgets: Repeat p95 was 75.26 ms against 50 ms; Hold p95 was 138.65 ms against
100 ms. The [qualification record](qualification/ui-feedback-2026-09-26.md)
retains the workload, results and playback-test timeout/retry history. The loop
also found and fixed a selected-card resize defect; regression coverage now checks
that selection stays visible. The seven rejected wraps remain evidence of that
earlier behavior; the current [interaction finding](INTERACTION_REVIEW.md#observed-development-findings)
records the scoped Repeat fix.
Existing qualification records keep their original scope.

The [Original moment increment](qualification/moment-paste-2026-09-27.md) adds
`original-moment`, exercising selection, copy, paste before/after, one-step undo
and pointer controls through production input. Its 212 app/harness tests passed.
The visual and release attempts stopped at Metal adapter creation before any
scenario steps, assertions, captures or timing samples. The first audit detected
Kestrel source drift; a reviewed help-description-only change required refreshing
the digest, with every reservation row unchanged. Both final refreshed audits
passed all 3,472 routing cases against 62 reservations. That increment left its
painted workflow and design comparison unverified; the layout qualification
above supplies the later visual evidence. Moment-specific latency remains open.

The editing replay now also inserts and undoes a pause before a Repeat. Its
2026-09-26 core-24 rerun stopped at Metal adapter creation in the current sandbox;
none of that scenario's new visual assertions ran. The shortcut audit passed.

The [Original audition increment](qualification/original-audition-2026-09-27.md)
adds `original-playback`, covering Space, selection loops, exact resume and
failure feedback while preserving Sequence editing context. Its verification
record separates production UI replay from headless actual-PCM tests and native
listening/visual acceptance.
The earlier nine-scenario host result above does not cover these added steps.

The [sound audition increment](qualification/sound-audition-2026-09-27.md) adds
`sound-playback`: real audio-only registration, separate catalog selection and
sample clock, keyboard/pointer controls, stale/fault handling and retained
picture/editor context. Its injected delivery is UI state evidence only. The
new [ImageGen board](design/boards/sound-audition-board-v1.png) is the visual
target; the qualification record distinguishes actual rendering from adapter
failures and headless PCM tests. All 236 app/harness tests passed. Both visual
and separately built release replay passed the live 3,472-case shortcut audit,
then stopped at missing Metal before any sound scenario steps, captures or
timings. The layout qualification above now records the painted interaction and
design comparison; native listening acceptance remains separate.

The [structural speed increment](qualification/retime-editing-2026-09-27.md)
adds `retime`, covering inspector entry/cancellation, exact duration feedback,
preserve/tape adjustment, explicit nesting and undo to the Original baseline.
Its qualification record separates headless command and decoded-PCM evidence
from the actual Metal replay attempt. Earlier screenshots do not establish the
new inspector's appearance or keyboard flow.

The core-25 [interior pause increment](qualification/interior-insertion-2026-09-26.md)
also replays a pause inside an Original fragment before a Repeat and checks one
undo. Its current visual attempt again stopped at Metal adapter creation before
any editing checks or captures. The shortcut audit passed 3,472 routing cases
against 62 reservations with no conflicts. All 196 app/harness unit and integration
tests passed, including the separate native VFR service case, but the new painted
keyboard path and its aesthetics remain unverified.

The core-26 [nested Sequence pause increment](qualification/nested-sequence-2026-09-26.md)
adds `nested-pause`, a production keyboard replay through two framed groups,
with captured cursor, visible parent selection, and atomic undo/redo checks.
All 198 app/harness tests and strict feature lint passed. Its visual attempt
stopped before app construction with `No adapter found`, so zero scenario steps,
assertions or captures ran. The separate shortcut audit passed 3,472 routing
cases against 62 reservations. Nested Hold inspector navigation remains
unavailable; the new painted interaction remains unverified.

The [repaint-wait correction](qualification/repaint-wake-2026-09-26.md) replaces
timer polling and adds worker-phase timestamps. Its final checks passed 193 unit
and 2 integration tests plus strict feature lint. Visual and release replay
attempts again stopped at missing Metal before any scenario steps or timing
samples; they do not establish improved latency or a new aesthetic review.

The [retained evidence summary](../tools/ui-feedback/evidence/2026-09-26/summary.json)
includes final run identities and measurements. The focused painted-playback and
full-report baseline comparison also passed; first-frame error clipping remains
an explicit finding. Feature tests finished with 180 unit and 2 integration tests
passing. The full workspace invocation had a playback timeout, followed by a
passing focused retry; its cause remains unestablished.

The media fixture is `cfr-bframes.mp4`, a small 120-frame source. The large-project
scenario adds 10,000 silent Background Holds. Playback service updates are
explicitly injected: no PCM preparation or audio device is exercised. These
fixtures do not qualify full-size decode/playback, physical presentation, native
accessibility or the complete editor.

## Run the loop

Prepare the pinned FFmpeg prefix as described in [Development](DEVELOPMENT.md).
Use a fresh output directory for each run so a report cannot pick up stale images.

```sh
cargo run -p deadpan-app --features ui-harness --locked -- --ui-check --output /tmp/deadpan-ui-visual-NEW
cargo run -p deadpan-app --release --features ui-harness --locked -- --ui-check --mode performance --output /tmp/deadpan-ui-performance-NEW
```

The first command is visual mode. Performance mode requires `--release` and
submits the full UI and picture composition without screenshot readback. Both
modes require Metal. The output directory must not already exist.

The current unrestricted host exposes Metal; earlier sandboxed failures remain
in qualification records. If a run reports no adapter, retain that result and
resolve Metal access before retrying. A software or mock renderer cannot satisfy
this loop's GPU evidence requirement.

| Option | Meaning |
| --- | --- |
| `--mode visual` | Default mode, with PNG captures. |
| `--mode performance` | Release-only timing run without PNG capture. |
| `--scenario NAME` | Run one scenario from the table below; the Kestrel audit still runs. Omit to run all scenarios. |
| `--hz 60` or `--hz 120` | Simulated replay timestep; default 60. This does not pace execution or prove display frame rate. |
| `--kestrel-source /path/to/Shortcuts.swift` | Compare the current Swift source digest with the checked-in evaluated reservation fixture as well as auditing production routers. Source drift fails until the fixture is deliberately reviewed and refreshed. |
| `--baseline /path/to/previous-output` | Visual mode only. Compare matching named checkpoints with a previous report; baseline/output directories must be separate and non-nested. Differences produce review warnings and images, never automatic baseline acceptance. |
| `--retain-projects` | Keep each scenario's private `Documents` root at `OUTPUT/projects/SCENARIO/Documents` for native QA after replay releases its writer. The report records the absolute root; packages are beneath its `Deadpan` directory. Default: temporary storage is deleted when the scenario ends. |
| `--help` | Print usage without running scenarios. |

For local shortcut drift checks, the current source is
`/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift`. The default
audit uses the repository fixture and needs no dotfiles checkout.

Each run writes `report.json` and `report.html`. Visual mode also writes
`SCENARIO-NNN.png`. The HTML contains contact sheets, a capture scrubber and
per-step state. Its playback advances between captures; it is not a real-time
video or a frame-rate measurement. Baseline comparison adds copies and difference
images under `comparison/`. Review warnings even when the process succeeds.

A scenario filter narrows iteration; it does not replace the applicable suite
before handoff. Scratch outputs stay outside the repository. Keep evidence worth
retaining in a dated qualification record with representative images and the exact
report identity.

For every meaningful UI change:

1. Add or update a scenario for the actual user action, including its input method
   and the important intermediate state. Use keyboard and pointer paths where both
   expose the changed behavior.
2. Run visual mode. Read failures and the state report before interpreting images.
   Inspect the contact sheet, then open the affected full-size frames, including
   pending, disabled, error or cancelled states touched by the change.
3. Compare with the design boards and the intended workflow. Fix confusing focus,
   clipping, misplaced controls, weak hierarchy, stale feedback and unnecessary
   movement even when assertions pass. Review pointer targets and hover affordances
   as well as keyboard hints.
4. Run release performance mode when the change can affect input, layout,
   rendering, decoding, service work or repeated interaction. Inspect distributions
   and failed samples, not only an average. Keep visual capture separate.
5. Run focused native checks for unresolved OS or physical-display behavior.
   Record what replay proved, what image review found, and what remains untested.

Do not automatically bless new screenshot baselines. A comparison can detect
change; it cannot decide that the new layout is good. Do not weaken an assertion
or substitute fake content to make an application failure pass.

## Replay the actual interface

Scenarios must enter through production input handling and draw production
widgets. Pointer steps use actual control geometry or stable semantic targets;
typing reaches the focused field. Avoid calling the edit method directly to stand
in for clicking its button or pressing its shortcut. Fixture preparation may use
the project API, but the measured action must take the user's path.

Use the real project service, decoder and shared GPU renderer for video scenarios.
The captured viewer must contain the submitted video texture and the same framing
and display transform as the app. A placeholder image, independently redrawn UI
or state-only snapshot does not prove the displayed result. Retain exact fixture,
project revision, requested picture and displayed picture identities.

Keep replay deterministic where practical: fixed viewport, scale, fixture content
and event sequence; stable capture checkpoints; bounded deadlines for real work.
Replay time controls UI time-dependent behavior. Monotonic elapsed time measures
latency. Neither clock may substitute for the other. Waiting for a settled frame
must not erase evidence of intermediate stale, missing or misleading feedback.

## Implemented scenarios

Every UI scenario starts by creating and displaying the small Original through
the real app. The initial viewport is 1280×820 points at 1×. The code lives in
[`preview/harness.rs`](../crates/deadpan-app/src/preview/harness.rs),
[`scenarios.rs`](../crates/deadpan-app/src/preview/harness/scenarios.rs),
[`scale.rs`](../crates/deadpan-app/src/preview/harness/scale.rs) and
[`edit_latency.rs`](../crates/deadpan-app/src/preview/harness/edit_latency.rs).

| Name | Current replay and assertions |
| --- | --- |
| `kestrel-shortcuts` | Always runs. Checks evaluated global reservations against Normal prefixes, Camera, text/IME, inspector, room-tone and Gain routing. The four corrected visual runs each pass all 5,456 cases without live-source drift. The Ghostty-only Cmd-N reservation is excluded from Deadpan. Optional source digest checking detects drift; physical interception remains native work. |
| `workspace` | Pointer frame navigation, repeated `,i` Original reuse, selected-card visibility, resize transitions at 960×640/1×, 1492×929/2× and 1280×820/1×, and a real monitor-slider drag that must not create a revision. Checks actual picture mesh bounds against the fitted canvas and unclipped navigation text, including Original at the minimum size. |
| `editing` | Counted Repeat, pointer opening of its setter, same-batch text submission, undo, exact pause insertion before a Repeat and before the Original, and Hold-duration editing. Checks duration and selection. Split, delete and redo are not yet part of this replay sequence. |
| `camera` | Pointer opening and numeric preview, Cancel, keyboard reopening/zoom and pointer Apply. Reach clipped inspector controls with real wheel input before clicking. Checks unchanged revision during preview, restoration of the submitted entry framing/source frame after Cancel, and authored framing after Apply. |
| `menus` | File-menu ownership of edit keys, help opening, keyboard/wheel changes to scroll offset and painted content, text containing edit keys and punctuation, and cancellation back to the exact active pane's focus. |
| `delayed-preview` | Holds a real decoder reply at a controlled delivery boundary, advances intent, resizes, releases the stale reply, checks newest-picture recovery, then injects a decoder failure and recovers. |
| `rapid-input` | Requires all eight explicit Repeat wraps in one batch to commit separately, with exact nested results and per-wrap undo. Delays delivery of real writer updates to check a partial `rr` through queued commits, pointer context cancellation, sixteen-waiting capacity, explicit overflow and Escape. Checks painted notices at default/minimum sizes, 30 frame-navigation inputs, final intent and idle repaint. Restores the Original before navigation. Performance mode warms 16 back/forward inputs, then measures exactly 120 more. |
| `playback-feedback` | Pointer Play/Pause/Cancel with injected preparation, delivery, stale-update and device-failure states. Requires complete error text on its first paint and wrapped errors on the first resize frame. Checks feedback and picture routing only; no device, PCM or listening claim. |
| `large-project` | Opens a real SQLite fixture with 10,000 root Background/Silence Holds. Tests end navigation, pointer wheel/selection, minimum-size selection visibility and bounded rendered cards. Alternates near-end `j/k` 64 times visually or 160 times in performance mode. No media or large asset inventory is stressed. |
| `edit-latency` | Dispatches cached `rr`, waits for its committed picture, then undoes; repeats with `:hold 11f` and undo. Checks one matching commit and picture completion per edit, the exact Repeat or Freeze/Silence structure, and restoration of the authored Original baseline with a fresh revision. Visual mode runs two cycles per type. Performance mode warms four cycles per type and measures 40 more per type. |
| `original-moment` | Checks active-empty Visual guidance, selects Original [10,24) through v and counted h/l, copies with y, cancels selection with Escape, returns to Your edit, pastes after with p, undoes once, then pastes before with P. Also selects/copies and pastes through actual buttons. Checks exact range, unchanged copy revision, pane cues, destination, selected Source and exact restored structure. |
| `original-playback` | Uses production Original Space and Shift+Space input, adjustable context, exact loop resume, pointer controls, stale update rejection, navigation stop and failure feedback. Injected device updates exercise UI routing only; actual canonical PCM has separate headless tests. |
| `sound-playback` | Registers two measured audio-only sources, selects by pointer and j/k, exercises Space pause/resume and Shift+Space full-sound loops, rejects stale/faulted delivery, stops on pane/source changes and retains native text input. Pointer Pause must change state on release and paint Resume on the next frame. Pinned controls and status must remain fully painted on the first resize frame. Asserts no sound-driven picture request or editor-clock/selection mutation. Delivery is explicitly simulated; separate backend tests compare real AAC PCM. |
| `sound-placement` | Imports measured catalog audio, places by pointer and `,s`, preserves picture duration and editor targets, checks exact sample entry and frame-nudge count equivalence, gain/edges, event selection, deletion and undo. Tests Original-to-Sounds keyboard/pointer focus, absent/stale command targets across held real writer completions, overflow, both window sizes and route-preserving gain with rejected movement. Does not start PCM preparation or a device. |
| `room-tone` | Copies an Original range with v/motions/y, opens the captured Hold's source-range sheet, edits exact native samples, prepares and auditions without history, then explicitly applies or cancels. Checks silence/undo, missing and stale targets, superseded range preparation, native text/IME and minimum-size paint clips. AccessKit Focus reveals saved Hold gain controls; :gain/Tab/Escape preserve room tone and history. The corrected run passes 103 scenario checks. Source delivery is explicitly injected; no device or listening claim. |
| `gain` | Counted/absolute beat gain and true mute, wrong-focus rejection, buffered exact trim/envelope/key/mute fields, coalesced writer proposals, Before/Draft at exact delivered samples, stale/faulted updates, one Apply/undo, unchanged Cancel picture continuity and native text/IME. Full populated Tab/Shift+Tab circuits check focused paint and hit clips at both sizes; pending text leaves Pause available. The corrected run passes 266 gain checks and its graph captures were reviewed. Uses real qualified source evidence/history; delivery is injected, with no PCM preparation, device or listening claim. |
| `retime` | Opens/cancels speed entry by pointer, checks the resolved-duration preview, creates a Preserve Retime by command, adjusts the same stage to tape pitch through ordinary text editing, explicitly nests another stage, undoes all three edits and confirms Original context stays unchanged. Uses real project history and picture preparation; it does not measure acoustic quality. |
| `nested-pause` | Seeds two framed Sequence groups with typed store commands, reopens the actual project, navigates to frame 17 with keys, inserts `:hold 11f`, and checks the nested Hold, exact freeze, retained child crop, live ancestor scopes and cursor. Undo/redo compares nodes and audio bindings. Enter drills through breadcrumbs to the Hold; Inspector Enter changes its duration, history preserves scope, Camera commits only its framing, Backspace selects exited groups, and a group-edge pause fails without mutation. |

The trace is bounded to 6,000 frames per scenario, with 15-second waits for real
work and a 5-second GPU completion wait. Visual waits discount screenshot work
from the wait deadline; performance waits use elapsed wall time unchanged. The
report retains actual wall time separately from simulated replay time.

After 120 images, intermediate captures stop with an explicit warning and semantic
steps continue. Named checkpoints and failure captures retain capacity up to a
160-image hard limit; exceeding that limit fails explicitly. Check the report's
capture warnings before claiming visual review. A semantic step without a PNG is
not a capture. These bounds keep the developer tool finite, not real-time.

## Coverage to grow with the product

The table is an acceptance map, not a claim that every scenario exists today.
Report unsupported cases explicitly. Add capabilities to this loop as they ship.

| Area | Useful scenario and observations |
| --- | --- |
| Layout | Empty, loaded, selected and error states at supported minimum and larger windows. Inspect picture dominance, readable values, clipping, control spacing and stable layout during updates. |
| Keyboard | Slow prefixes, rapid batches, counts, held navigation, rejected operators and global-binding conflicts. Assert pending text, valid next keys, scope and the resulting action. |
| Pointer and wheel | Select a beat, switch panes, open a menu, scroll lists/help and edit a parameter. Check hit targets, selection reveal, focus transfer and no accidental structural edit during text or pointer ownership changes. |
| Editing | Seek, Split, Repeat, pause, Camera, undo and redo through their UI paths. Check the committed revision, selected node, cursor and rendered result together. |
| Preview | Cold and warm seeks, repeated navigation, superseded decode, render delay and recovery. Compare requested and displayed identities and retain the last valid picture while replacement prepares. |
| Text and focus | Type and submit in the same batch, cancel, click away, open help/menu and return. Normal shortcuts must not consume field text or committed IME input. |
| Feedback | Pending, disabled, invalid, unavailable, completed and cancelled actions. Inspect both visible wording and state so progress cannot masquerade as completion. |
| Scale | Large source/sound lists, long names and many root beats, including large compact Repeats. Measure bounded work and selected-item visibility rather than timing only tiny fixtures. |

Native IME event delivery, accessibility navigation and physical keyboard layout
behavior still need native checks. Injected events establish only their handling
after they reach the app.

## Assertions and evidence

Prefer a precise failure such as “Camera cancel changed the committed revision,”
“selected beat is outside the visible strip,” or “displayed caption names a newer
frame than the submitted texture.” Report the scenario, input step and expected
versus observed values. Geometry checks should identify the clipped control or
overlapping bounds. Preserve failure artifacts instead of capturing only success.

Visual output should associate each frame with its input checkpoint, viewport,
scale, selection, focus and picture identity. Contact sheets make transitions
reviewable; full-size frames establish readable detail. Assertions must use
production state or accessibility/geometry observations, not a second hand-built
model of what the screen ought to show.

Do not treat an accessible label in a headless tree as proof that VoiceOver can
navigate it. Do not treat a screenshot as proof of hit testing, event ownership,
motion quality or response latency. Each report should name its evidence boundary.

## Performance interpretation

[Specification Section 25](spec/DEADPAN_SPEC.md#25-performance-requirements-and-instrumentation)
defines targets, including p95 below 8 ms from key event to command-state update,
below 50 ms for a cached ordinary edit to visible preview, and below 80 ms for a
warm indexed seek on the primary reference machine. These remain targets until
the corresponding real workload is measured.

Name every measured interval. Event dispatch to a state change, service commit,
GPU submission and GPU completion are different endpoints. Offscreen completion
does not measure when a physical display scans out the image. PNG encoding,
contact-sheet assembly and readback for screenshots must not be charged as app
response time or silently included in an otherwise incomparable benchmark.

The current report separates UI-frame CPU, input-frame CPU, request-to-decoder
delivery, request-to-picture submission/completion, input-to-observed-commit and
UI composition submission/completion intervals. Successful picture receipts also
separate request-to-worker-start, worker execution, finished-to-publication and
publication-to-UI-delivery time. Their four intervals sum to request-to-decoder
delivery for the same ticket. The worker interval includes all `perform` work,
which can include source preparation and plan resolution as well as decoding.
Picture telemetry retains the
request ticket and distinguishes success, failure, supersession, stale delivery
and retained/repeated submissions. A resize or Camera redraw of retained content
cannot complete a newer request or add a duplicate successful latency sample.
Expected injected failures remain explicit trace evidence outside successful
latency distributions.

`input_to_state_ms` spans the entire egui input frame. It is an upper bound on CPU
time to its state update, not a timestamp at the exact mutation. The
`input_to_picture_complete_ms` endpoint follows the actual offscreen composition's
GPU completion wait. It excludes native compositing and physical scanout.
Visual-mode intervals can include earlier capture work between stages, so use
the separate performance run for claims.

After 16 warm-up inputs, `rapid-input` measures exactly 120 back/forward inputs
and requires one completed CPU and picture sample for each. It gates
`warm_navigation_input_cpu_ms` p95 below 8 ms and
`warm_navigation_input_to_picture_complete_ms` p95 below 80 ms. Import, setup,
idle, rejected edits and warm-up samples do not enter those two distributions.
`large-project` separately gates its navigation-frame CPU p95 below 8 ms.

`edit-latency` measures the dispatching input through the real service commit,
decode and offscreen GPU composition completion. After four warm-up cycles per
type it records 40 cached Repeat and 40 silent-freeze Hold edits. It gates
`cached_repeat_input_to_picture_complete_ms` p95 below 50 ms and
`hold_fallback_input_to_picture_complete_ms` p95 below 100 ms. Separate
`cached_repeat_input_to_commit_ms` and `hold_input_to_commit_ms` samples retain the
matched commit interval. Every edit must produce exactly one matched commit and
picture sample; undo restores the exact authored baseline but is excluded from
the edit's latency subset. Visual mode performs two cycles per type for inspection.

The [2026-09-26 release run](qualification/ui-feedback-2026-09-26.md#performance-results)
measured warm navigation CPU/picture p95 at 1.32/21.16 ms and 10,000-beat navigation
CPU p95 at 1.23 ms. Repeat/Hold picture p95 missed their targets at 75.26/138.65 ms.
These are small-fixture checks of specific Section 25 targets; they do not qualify
full-size editing, all effects or physical display latency. Keep measured misses
visible and inspect raw samples, warnings and workload boundaries.

Record hardware, OS, revision and dirty-tree identity, build profile, dependency
versions, fixture, viewport, scale, warm/cold state and sample count. Preserve
timeouts and failed samples. Use release builds for latency claims. Compare like
workloads and report stage distributions so a regression is attributable. Tiny
fixtures, an idle machine or an offscreen renderer do not qualify full-size
playback, audio-device deadlines or memory pressure.

## Harness checks in CI

The repository CI includes the optional feature's lint and test coverage:

```sh
cargo clippy -p deadpan-app --features ui-harness --all-targets --locked -- -D warnings
cargo test -p deadpan-app --features ui-harness --locked
```

These check the harness, telemetry, report and shortcut contracts. They do not
execute the full Metal scenario suite or inspect its images. Run the applicable
visual and release performance modes separately and record the actual result.

The [native room-tone qualification](qualification/native-room-tone-2026-09-27.md)
records 909 full visual checks and 1,879 release performance checks on the same
frozen source, including 90 new room-tone checks and the 4,464-case Kestrel audit.
Full-size draft and saved-state captures were reviewed at both window sizes.
The HTML contact-sheet check remains outstanding because the required browser
role was unavailable; physical input and acoustic acceptance remain separate.

## Repaint waits and worker timing

The [paired trace and timer diagnostic](qualification/composite-insertion-2026-09-26.md#retained-preview-latency-diagnostic)
found that requested 1 ms sleeps can last about 64 ms at p95 in this environment.
The earlier `Driver::wait_for` polling delay could therefore dominate UI
observation time. Preserve the earlier measured misses, but do not attribute
their entire request-to-receipt interval to decoder work. Those reports did not
timestamp worker completion.

The harness now waits on a bounded repaint scheduler using a mutex and condition
variable. Its callback is installed on the fresh kittest context before app
construction. Never replace native eframe's callback in `DeadpanApp::new`:
`set_request_repaint_callback` replaces the single callback that native eframe
uses to notify winit. Keep the existing project-service and preview-worker
`request_repaint()` producers intact. The callback runs under egui's context
write lock, so it may update bounded wake state and notify, but must not call
context APIs or touch app state.

Two slots retain the earliest deadlines for the current and preceding egui pass;
older passes expire. Each UI step services due notifications and preserves
eligible future deadlines, including one-shot delayed repaints across an early
input-driven step. Requests arriving during that step also survive. The final
viewport output schedules
outstanding or delayed repaints. The driver rechecks readiness after each step
and waits only while more work is needed, under the same mutex used by
notification. The 15-second timeout and visual capture-time exclusion remain.
There is no unconditional polling sleep. Replay `--hz` still sets virtual UI time,
not wall pacing or physical display frequency.

Feature-gated worker timestamps record start, finish and accepted mailbox
publication before its repaint notification. The UI records receipt; held
delivery preserves all worker timestamps. Four successful phase samples require
complete monotonic order from request through receipt. Missing or invalid timing
does not produce fabricated zero-duration phases. Failed, stale and repeated
receipts retain timestamped trace evidence but add no successful worker-phase
samples. Worker events arrive with the receipt, so consumers must use their
`wall_ms` and ticket rather than assuming trace array order is chronological.

Reports identify this behavior with `wait_strategy: egui_repaint_callback_v1` and
`picture_worker_timing: request_start_finish_publication_receipt_v1`. Compare
versions explicitly. The [2026-09-27 host measurement](qualification/workspace-layout-2026-09-27.md#release-responsiveness)
now passes navigation and edit budgets. Preserve the earlier failed reports;
these runs are not a controlled attribution of the gain to one change, and
small-fixture offscreen timings do not establish full-size or native latency.

## Native evidence

Use native computer interaction for window lifecycle, macOS panels, global
shortcut interception, physical key layouts, IME, VoiceOver, OS scaling settings,
physical presentation and real-use questions that offscreen replay leaves open.
The normal development loop should already provide reproducible state, pictures
and timing before spending time on those checks. Native failures become replay
scenarios when their cause can be reproduced within the application boundary.

See [Development](DEVELOPMENT.md#native-application-smoke-test) for the lifecycle
smoke test. Record native findings separately from offscreen checks and preserve
any skipped evidence in the handoff.
