# Interaction review

Deadpan should respond predictably to keyboard and mouse input, keep the picture
central and make the next action apparent. This review identifies concrete ways
to improve that experience. It supplements the [normative specification](spec/DEADPAN_SPEC.md)
and [design contract](design/README.md); it does not reduce their required scope.
Verify changes with the [UI feedback loop](UI_FEEDBACK.md). No numerical UX score
can establish that the editor feels good to use.

## Current policy and evidence

The app already has a useful base: typed reversible commands, bounded services,
explicit requested/decoded/displayed picture identities, persistent prefixes,
visible pane focus, text/IME gating and temporary Camera state. Its native help
describes the implemented subset. The full [keyboard reference](spec/KEYBOARD_REFERENCE.md)
describes the finished grammar, including operations that remain unavailable.

The personal configuration review found a concrete compatibility defect:
Deadpan advertised Cmd+Return for insertion while Kestrel globally consumed it to
raise Ghostty. Routing, prefix hints, buttons and help now use `,i`; `:insert`
remains available. The `workspace` replay covers that insertion path, and every
UI run audits production routing against the checked-in evaluated Kestrel
reservation fixture. `--kestrel-source` additionally checks source drift.
The 2026-09-26 complete visual run passed 3,472 routing cases against 62 reservations with
no source drift. Configuration and injected events do not prove physical key
delivery. [Qualification](qualification/ui-feedback-2026-09-26.md) records the
executed builds, measurements and remaining evidence boundaries.
The [native gain increment](qualification/native-gain-2026-09-28.md) extends the
passing audit to 5,456 cases against the same 62 reservations and separately
checks native macOS command, focus, field and cancellation behavior.

Personal configuration is evidence for familiar behavior, not a runtime dependency
or permission to import all its bindings. These sources were reviewed on
2026-09-26; their current contents can change independently of Deadpan:

| Source | Verified convention | Consequence for Deadpan |
| --- | --- | --- |
| [Kestrel shortcuts](/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift:25) | Directional focus uses OPT+H/J/K/L; related modifier combinations move, swap and resize. Cmd+Return launches Ghostty. | Audit the entire global registry. Avoid assigning those chords to application actions. |
| [Kestrel feedback policy](/Users/michael/.dotfiles/kestrel/POLICY.md:296) | “Visible success stays quiet”; hidden changes and unavailable outcomes get concise feedback; newer messages replace older ones without a queue. | Use visible selection and picture changes as feedback. Explain failures and hidden modes without flooding ordinary navigation with messages. |
| [Vim configuration](/Users/michael/.dotfiles/vim/.vimrc:8) | Incremental search, highlighted matches and visible partial commands are enabled. | Keep prefixes, matching results and scope visible while input is incomplete. |
| [Vim pane keys](/Users/michael/.dotfiles/vim/.vimrc:29) | Space is the leader; arrows move between panes. | Record deliberate differences: Deadpan uses Space for immediate playback, comma for actions and arrows for frame/beat navigation. Do not copy bindings blindly. |
| [Shell configuration](/Users/michael/.dotfiles/zsh/.zshrc:82) | Emacs mode explicitly avoids accidental Escape entry into command mode. | Preserve native editing inside fields. Familiar Vim editing does not imply modal text entry everywhere. |
| [Kestrel help](/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift:113) | J/K and arrows scroll; Space/Shift-Space and page keys page; Escape closes. | Keep help navigable, preserve return focus and consider familiar paging where it does not compete with field input. |

The factual review followed the local
[Kestrel skill](/Users/michael/.dotfiles/.agents/skills/kestrel/SKILL.md). Its source
and policy were inspected; no Kestrel configuration or live state was changed.

## Observed development findings

The replay has exposed concrete interaction problems. The
[2026-09-27 layout qualification](qualification/workspace-layout-2026-09-27.md)
records the latest fixes, review and retained failures:

- **The viewer was shallower than the design target.** Compact status and beat
  margins and frame navigation beside the context tabs increase its height from
  about 242 to 330 points at 1280×820. The latest replay checks actual fitted
  picture bounds and unclipped navigation at each size, including Original at
  960×640. Static comparison against the enlarged ImageGen target confirms the
  improved hierarchy. The card strip still lacks the pictured thumbnails.

- **Selection disappeared after resizing.** In the 10,000-beat scenario, selected
  beat 9994 left the visible strip when the viewport changed from 1280×820 to
  960×640. The card spacing changed, but the old absolute scroll offset was
  clamped to the new end. [`cards.rs`](../crates/deadpan-app/src/preview/cards.rs)
  now reveals the selected card when card spacing or viewport width changes.
  The focused regression and the final visual large-project replay pass for this
  transition.
- **Camera was partly below the inspector's visible area.** Camera and pause
  actions now precede descriptive fields and are visible in default and minimum
  captures. Duration and speed stay near the top; optional framing presets have
  a disclosure. The real Camera replay passes without its former scroll warning.
  Keep real wheel input for controls that need it; clicking a clipped
  accessibility rectangle would hide the usability problem.
- **Rapid Repeat wraps now retain intent.** The layout run rejected seven of
  eight batched wraps while busy. The [Repeat increment](qualification/repeat-input-2026-09-27.md)
  retains up to sixteen waiting explicit wraps behind one submitted edit. Each
  consumes a matching committed wrapper before using a fresh target/revision,
  and each has a separate undo. Replay sends all eight in one batch without
  inserted waits. Visible counts explain waiting, overflow and cancellation.
  Setters and unrelated commands retain their normal busy rejection.
- **Command exit now resolves layout within the same frame.** The
  [Repeat qualification](qualification/repeat-input-2026-09-27.md) retains the
  old blank band after `:sequence`. The [footer increment](qualification/footer-layout-2026-09-27.md)
  retries changed footer geometry before presentation and checks both its bottom
  edge and painted command mode. Enter, Escape, pointer dismissal and combined
  resize/text/cancellation do not rely on an extra replay wait. Independent
  review also exposed clipped Original shortcuts at 960×640; complete measured
  key-label pairs now wrap together. The minimum-size picture remained smaller
  than the design target's emphasis; the [2026-10-15 review](qualification/gui-review-2026-10-15.md)
  raises it from 163 to 205 points in Your edit and 149 to 179 in Original.
  It is still smaller than the target's proportion, especially during command
  entry; responsive beat and playback layout remains improvement work.
- **The earlier replay missed edit feedback budgets.** On 2026-09-26, cached
  Repeat input-to-picture p95 was 75.26 ms against 50 ms, and the 11-frame Hold
  fallback was 138.65 ms against 100 ms. Their observed commit p95 values were
  3.26 ms and 69.65 ms. Preserve those failed reports.
  A [paired analysis](qualification/composite-insertion-2026-09-26.md#retained-preview-latency-diagnostic)
  of those same 40 cycles places most post-commit time before receipt of the
  decoded picture: request-to-receipt p95 was 67.77/69.03 ms for Repeat/Hold.
  This interval includes worker work and UI delivery; it does not identify
  decoder time alone or prove a new performance result.
  A separate headless timer probe observed requested 1 ms sleeps taking 63.66 ms
  at p95 in the earlier sandbox. The harness now uses repaint notifications and
  distinguishes worker start, finish, publication and receipt. The fresh
  2026-09-27 host run passes: Repeat p95 is 6.85 ms and Hold is 9.11 ms, with
  commit p95 of 3.08/4.95 ms. This qualifies the current small-fixture path;
  it does not isolate how much gain came from the wait correction, host access
  or other changes. Physical display and full-size workload latency remain open.
- **An output error was clipped on its first painted frame.** A premeasured
  notice panel now fits the exact text immediately, including wrapped errors on
  the first resize frame. The replay makes clipping a failure. The empty panel
  remains in the UI tree to preserve subsequent pointer widget identities.
- **Sound controls scrolled out of view.** The expanded replay exposed a clipped
  Loop button. Sound status, exact clock, Play/Pause/Resume and Loop now occupy
  a measured panel below the scrolling catalog. First-resize checks at 960×640
  and 1280×820 require their actual text to remain visible and paused context to
  remain unchanged. Catalog content still scrolls in the available space.
- **Gain fields could receive focus outside their paint clip.** The gain draft
  now reveals the complete newly focused label/input, keeps comparison and
  Apply/Cancel fixed, and wraps heading/Cancel without an empty Tab stop. Four
  populated forward/reverse circuits pass at both window sizes. A native macOS
  pass confirms field reveal and cancellation without a project change. General
  gain controls follow node-specific inspector actions so they do not displace
  the saved Hold audio recipe. See the [gain qualification](qualification/native-gain-2026-09-28.md).
- **Waveform placement separated exact fields from their gain curve.** Initial
  wide captures showed the fields above the visible curve, leaving empty space
  beside it. The waveform now occupies an earlier row; curve and fields share
  the next row. Final captures and a direct simultaneous-paint assertion verify
  the correction. The measured stereo reference retains separate amplitude
  scales and committed-owner identity. Native Retry and both keyboard directions
  pass after normal layout settling, with complete paint/hit checks. See the
  [waveform qualification](qualification/gain-waveform-2026-09-28.md).
- **Copied Original controls crowded the minimum normal picture.** The
  corrected room-tone capture at 960×640 shows the copied range, paste, audition
  and monitor controls plus an empty 64-point Placed sounds strip. Its picture
  was approximately 77 points high. The implemented compact layout now puts
  the empty Sounds entry beside the beat heading at window heights below 700
  logical points in the single-Original Your edit view, retaining its visible focus cue, pane identity
  and `,s` hint. Breadcrumbs scroll in the measured remaining width. The empty
  panel stays in the UI tree at zero height; copied-range, paste, audition and
  monitor rows keep their existing allocation. Room tone overlays this same
  background without adding or removing 64 points. Populated sounds, default
  size, Source, Camera and Gain retain their existing layouts. Scoped replay
  passes the 140-point minimum assertion, complete text/hit clips, native scale
  transitions, nested navigation, first placement/undo and exact pause/resume.
  The copied Hold viewer measures 143 points at minimum size; other tested
  phases measure 169 or 175 points. Final default, minimum, 2x, nested and
  populated captures were inspected. These results do not guarantee that height
  for all populated lists or larger fonts. See the
  [compact-workspace qualification](qualification/compact-workspace-2026-09-28.md).
- **A view change must not submit a picture for a discarded panel allocation.**
  Compact Sounds placement is captured once per layout pass and rechecked
  immediately before picture submission, including after viewer-tab actions.
  A native command's guaranteed footer close marks its pass for discard before
  rendering, while command execution still follows native text processing.
  Passing release-frame checks inspect submission count, final target dimensions
  and retained picture identity for pane/tab entry and command-plus-resize.
  The release build and all 2,323 full performance replay checks pass. These scoped
  layout results do not establish full-editor, device or listening qualification.

The [2026-09-26 visual run](qualification/ui-feedback-2026-09-26.md) passed all nine
scenarios and the shortcut audit, with the Camera and rapid-input warnings retained.
The release run passed navigation budgets and failed both edit-to-picture budgets.
The [retained summary](../tools/ui-feedback/evidence/2026-09-26/summary.json) and
[first error frame](../tools/ui-feedback/evidence/2026-09-26/playback-error-entry.png)
preserve the measured and visible findings; the focused painted-playback and
baseline-comparison follow-up passed without erasing them.
These results establish the tested behavior and identify work still needed; they
do not establish overall usability or native accessibility.

## 2026-10-15 GUI review

The [GUI review record](qualification/gui-review-2026-10-15.md) replayed the
workspace, editing, recipes, Gain, Place slice (`:splice`), Trim, Camera,
transcript, corrections, Help/Keys (`menus`), keymap, layouts and
accessibility scenarios at default and minimum sizes. Representative captures
are under [`tools/ui-feedback/evidence/2026-10-15-gui-review`](../tools/ui-feedback/evidence/2026-10-15-gui-review/).

Fixed, with before/after replay evidence:

- **Minimum-size picture priority.** Below 700 points the header uses 6-point
  vertical margins, the displayed-picture caption shares the single-line
  clock row, and compact Your edit reserves the clock row's real text height.
  Your edit grows from 163 to 205 points and Original from 149 to 179 at
  960×640; every measured compact phase gains about 30 points. The
  `workspace` replay now requires 200/175 points with the caption, clocks and
  transport fully painted. Widget IDs stay stable across the threshold.
- **Header status no longer wraps at 960 points.** Equal header thirds left
  about 40 points for the status, so `Draft preview` in Trim, Camera and Slip
  added a second header row. The side groups now use the space beside the
  centered title. Trim's minimum junction pictures grow from 115 to 147 points.
- **Camera Cancel** is a left-aligned full-width row like Apply above it.
- **The start card settles in time after `:backups`.** Its cached height is
  now kept per compact state and width; the baseline `backups` replay painted
  one frame after an ignored layout retry.

The Gain waveform visibility failure recorded on 2026-10-11 was already
resolved by the [scroll-sequencing correction](qualification/gain-scroll-2026-10-11.md);
release 120 Hz runs pass on both the baseline and final binaries.

The [follow-up record](qualification/gui-findings-2026-10-15.md) then fixed
the open layout findings, each with a replay assertion that fails on the
previous code:

- **Compact cards show thumbnails** (70 × 52 at 960 points) while keeping
  their exact text width.
- **Strip edges say what they hide.** The cursor band counts wholly hidden
  beats (`‹ 4`, `2 ›`) and a card the edge cuts fades out.
- **Loading is not black.** A pending thumbnail shows `Loading…` on a raised
  panel tone and a failed one `No preview`; only a Background is black.
- **Side-pane scroll bars sit in the pane margin**, clear of Camera's Apply and
  Cancel and every other full-width row.
- **Trim's empty junction error row takes no height**; the band above the
  waveform drops from 39.8 to 16.5 points and the pictures grow to 170 points.
- **Help's key line uses keycaps** while browsing and while searching.
- **The narrow inspector wraps** its title and word values instead of eliding.
- **A quiet compact notice joins the footer's mode row** when it fits; the
  minimum Your edit picture grows from 205 to 231 points.
- **Gain's scroller frames its waveform heading and graph together** (123 of
  133 points); the draft status moved to the command row.
- **Populated-sound compact Your edit** uses the compact viewer: 145 to 201
  points.

Still open: the sound event inspector's fixed 104-point label column makes
long values wrap at 960 points, and `audio-warming` still times out waiting
for its real PCM horizon on both the baseline and final binaries.

## Keys palette (2026-10-15)

The Keys sheet's search now runs actions (specification §8 and §7.5: open
Place slice "through the searchable palette or `:splice`"). One result is
highlighted; Up/Down or Ctrl N/Ctrl P move it, Enter or a click runs it.
Verbs without arguments run as typed, verbs with arguments open the command
line with the verb and its usage, key-only actions use the editor's own
binding dispatch, and an action that cannot run here keeps Help open with a
sentence naming where it works and its keys rather than doing nothing. Escape
still leaves the field before it closes Help, and input typed after Enter in
the same batch reaches whatever the action opened. The `palette` replay drives
`/splice` plus Enter in one batch into Place slice, Down/Ctrl P, `:jobs`,
`v` (Visual), the `d` operator's pending prefix, `:hold` with its usage, a
Trim key's explanation and a clicked row. See [bindings](KEYMAP.md#action-registry).

Still open: a two-verb or two-direction action runs its first form only
(`:undo`, next beat), and VoiceOver speech of the highlighted row and physical
key delivery are unverified.

## Jobs: waiting reasons, queued pauses and quitting (2026-10-16)

AI models now wait before loading on battery power, in Low Power Mode,
under thermal or memory pressure and during playback. The Jobs row says
which (`Waiting while running on battery · O runs it anyway`) and offers a
Run anyway button and the O key; the AI inspector repeats the reason.
Generating a second pause no longer refuses: it queues behind the running
one with its own row and Cancel, and X cancels exactly that row's job.
Running rows add `about M:SS left in this stage` only from measured progress. Closing or
quitting while requested work runs asks Quit when they finish (focused),
Cancel them and quit, or Go back (Escape); waiting shows a banner below the
header that keeps the project name visible. The `jobs` replay drives each
path through real keys and inspects the captures ("AI jobs waiting on
battery", "Run anyway; second generation queued", "Quit while jobs run",
"Waiting to quit"). Visual and isolated release runs each pass 29 checks;
the final captures were inspected against the workspace target. See
[qualification and retained limits](qualification/jobs-2026-10-16.md).

Still open: VoiceOver speech of the held rows and the quit question, ⌘Q and
Dock Quit through the native guard on a physical Mac, and the remaining-time
text on a real model run (the replay's scripted run is too short to show it).

## Priority improvements

| Priority | Desired interaction | Current boundary and evidence needed |
| --- | --- | --- |
| First | Every advertised shortcut reaches its action in the user's environment. | `,i` replaces Cmd+Return. The evaluated Kestrel reservation audit covers globals and the known Ghostty-specific exception; source drift fails explicitly when supplied. Complete native physical-delivery evidence and maintain the audit as bindings change. |
| First | Focus, selected content, Original/Your edit context and requested/displayed positions remain distinguishable. | Replay checks selected-card visibility and exact pane focus after text cancellation. Inspect loaded, pending and error frames and confirm equivalent meaning through native accessibility. |
| First | Common inspector actions are visible and reachable without discovery by scrolling. | Camera and pause now precede metadata; actual default/minimum captures and Camera replay qualify this change. Preserve pointer-wheel replay and verify actual clipped bounds as more actions arrive. |
| First | A committed edit reaches its picture feedback budget. | The Repeat increment passes release Repeat/Hold p95 at 6.76/8.98 ms against unchanged 50/100 ms targets. Preserve the earlier misses and qualify full-size workloads and physical display latency. |
| First | Rapid commands preserve intent or make rejection clear. | Eight batched explicit Repeat wraps now commit separately through a bounded queue, with cancellation and overflow feedback. Other command types retain busy rejection and need their own interaction review. |
| First | New errors are readable on the first painted frame. | Premeasured notices now pass first-paint and wrapped first-resize assertions. Preserve exact paint checks and stable widget identities as status content grows. |
| First | Escape has one predictable meaning in the current owner. | Replay checks menu ownership and Camera cancellation restoring the submitted entry composition without a revision. Preserve pending-input, text and native-dialog ownership; Escape must not undo a committed edit. Extend mixed and same-frame coverage with new modes. |
| First | Continuous navigation stays visually coherent. | Delayed-preview replay checks retained and stale pictures. Ticket-bound telemetry distinguishes completed, failed, superseded and repeated work. Inspect intermediate frames and the isolated warm input-to-GPU-completion measurements. |
| Next | Parameters are quick to discover and adjust by mouse or keyboard. | Show units, current value, scope, valid range, Apply/Cancel policy and disabled reasons. Current Hold/Repeat command entry is not live preview; Camera is. Review field focus and pointer targets at supported sizes. |
| Next | The interface explains unavailable actions without becoming noisy. | Use concise local reasons for invalid scope, missing preparation or unsupported operations. Keep ordinary visible success quiet; never report completion from queued work. Test recovery and replacement of stale feedback. |
| Next | Search finds editorial content without focus repair. | `/` currently searches sounds or legacy sources. Transcript/moment search and `n/N` remain required. Add them with incremental matches, clear context and reliable return focus when real analysis exists. |
| Next | The editing language composes consistently. | Counts and current-depth `rr`/`dd` work; ordinary Sequence navigation and Original `v`/`y` with `p`/`P` are implemented. Text objects, full Visual replacement, persistent registers, macros, occurrence navigation and semantic dot-repeat remain required. Extend one typed grammar and its discoverability. |
| Ongoing | Layout remains quiet and readable as real data grows. | Review long names, errors, many beats, narrow windows, inspector changes and scrolling. Preserve picture area, explicit durations and a stable status region. Assess hierarchy manually alongside geometry assertions. |

## Shortcut compatibility policy

Keep plain `h/j/k/l`, counts, `gg/G`, `u`, Ctrl-R, `:` and `?` consistent with
the authored grammar. Preserve native Command shortcuts and field editing where
they do not conflict with global ownership. Keep Space immediate for playback;
moving the leader to Space would introduce a playback-versus-prefix decision.

Reserve the complete current Kestrel global map, not only its movement keys.
That includes OPT+H/J/K/L, SHIFT+OPT+H/J/K/L, CTRL+OPT+H/J/K/L, Hyper bindings,
OPT+N/P, OPT+Space, Desktop bindings and global launcher combinations. The registry
also has application-specific bindings, which should conflict only in that app's
scope. Physical key identifiers, logical keys and modifier aliases must be
normalized explicitly; a string comparison of displayed chord labels is inadequate.

The current audit uses the evaluated registry in
[`kestrel-reserved.tsv`](../crates/deadpan-app/src/navigation/kestrel-reserved.tsv)
and the production routers in
[`shortcut_audit.rs`](../crates/deadpan-app/src/navigation/shortcut_audit.rs).
It does not execute or parse arbitrary Swift. A supplied live source is checked
by SHA-256; a changed registry requires deliberate re-evaluation. Unknown physical
keys or app scopes fail rather than silently dropping a reservation.

Deadpan's own command registry should keep routing, discoverable keys, enabled
state and help aligned as the vocabulary grows. A proposed registry is not proof
that those surfaces are already generated from one source. Tests should catch a
documented shortcut with no route and an enabled action whose target is unclear.

## Review a workflow, not only a screen

Use short real tasks: select a beat, seek, Split, Repeat, pause, adjust Camera,
cancel or apply, then undo. Repeat the relevant path with the pointer. Inspect
whether selection follows the committed result, focus stays where expected,
controls remain readable and the image reflects the correct revision.

The current [replay scenarios](UI_FEEDBACK.md#implemented-scenarios) cover portions
of this workflow and a separate 10,000-beat background fixture. Playback feedback
uses injected service updates and does not qualify audio output. Extend replay
coverage when adding an interaction; keep unsupported and unmeasured behavior
visible instead of inferring coverage from a similarly named scenario.

The rapid-input scenario now requires eight separate commits from one batch of
explicit wraps, their exact nested result, and individual undo to the Original.
It also holds real writer updates at the UI delivery boundary to check pending
prefixes, pointer cancellation, bounded overflow and Escape. Help replay checks both scroll state
and changed painted content. Camera cancellation checks the submitted composition
rather than only a closed mode or unchanged revision. The separate `edit-latency`
scenario checks cached Repeat and an 11-frame silent-freeze Hold through the real
commit and offscreen GPU path, then undoes each to the exact authored Original
baseline. Its 50 ms Repeat and 100 ms Hold p95 gates use 40 measured edits per
type after four warm-up cycles; matching commit intervals remain separately
inspectable. The 2026-09-26 release report preserves its two measured
edit-latency failures. Later runs retain separate identities and results in the
[layout qualification](qualification/workspace-layout-2026-09-27.md).

Record extra keystrokes, unexpected mode changes, focus repairs and required
mouse reaches as concrete friction. Treat lower counts as a useful comparison,
not a universal quality target: an explicit confirmation or visible scope can
prevent a costly mistake. Pair those observations with stage latency and pictures.
Preserve failed attempts and native limitations rather than reporting only the
successful final screenshot.

Changes to binding meanings, focus policy or preview/commit semantics must update
the implementation, current user-facing guidance and affected replay scenarios
together. Keep historical qualification notes unchanged; record new evidence in
a new dated report.
