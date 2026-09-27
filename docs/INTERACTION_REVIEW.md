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
The complete visual run passed 3,472 routing cases against 62 reservations with
no source drift. Configuration and injected events do not prove physical key
delivery. [Qualification](qualification/ui-feedback-2026-09-26.md) records the
executed builds, measurements and remaining evidence boundaries.

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

The replay has exposed concrete interaction problems:

- **The retained workspace has a shallower viewer than the design target.** A
  static comparison of the [visual06 workspace capture](../tools/ui-feedback/evidence/2026-09-26/workspace.png)
  with the [enlarged ImageGen target](design/boards/single-source-workspace-v1.png)
  shows several transport and status rows consuming vertical space beneath a
  comparatively shallow image. The coded card strip also lacks the pictured
  thumbnails. Reclaim vertical space for the viewer while keeping ordinary keys
  visible, and measure the result in the production replay at the same viewport.
  This comparison uses the retained harness build, not a new native GUI run;
  generated proportions and pictured features are targets, not measured pixels
  or proof of implementation.

- **Selection disappeared after resizing.** In the 10,000-beat scenario, selected
  beat 9994 left the visible strip when the viewport changed from 1280×820 to
  960×640. The card spacing changed, but the old absolute scroll offset was
  clamped to the new end. [`cards.rs`](../crates/deadpan-app/src/preview/cards.rs)
  now reveals the selected card when card spacing or viewport width changes.
  The focused regression and the final visual large-project replay pass for this
  transition.
- **Camera was partly below the inspector's visible area.** At the default
  1280×820 viewport, reaching the common Camera action required scrolling.
  Move common actions into an initially visible section, or provide a clear
  overflow cue where scrolling is necessary. Keep real wheel input in replay:
  clicking a clipped accessibility rectangle would hide the usability problem.
- **Rapid edit input is rejected while busy.** Seven of eight Repeat intents in
  one batch were explicitly rejected in the final visual run. Every intent was
  accounted for and the admitted edit had the correct result, but this remains
  interaction friction. Review safe bounded handling of successive edit intent
  and make any rejection immediately clear; do not insert replay waits to hide it.
- **Committed edits miss the picture feedback budgets.** In the final release
  run, cached Repeat input-to-picture p95 was 75.26 ms against 50 ms, and the
  11-frame Hold fallback was 138.65 ms against 100 ms. Their observed commit p95
  values were 3.26 ms and 69.65 ms. Use the recorded stages and matched samples
  to locate the work; preserve these misses until the actual path improves.
  A [paired analysis](qualification/composite-insertion-2026-09-26.md#retained-preview-latency-diagnostic)
  of those same 40 cycles places most post-commit time before receipt of the
  decoded picture: request-to-receipt p95 was 67.77/69.03 ms for Repeat/Hold.
  This interval includes worker work and UI delivery; it does not identify
  decoder time alone or prove a new performance result.
  A separate headless timer probe observed requested 1 ms sleeps taking 63.66 ms
  at p95 in the current sandbox. The harness now uses repaint notifications and
  distinguishes worker start, finish, publication and receipt. A fresh host
  Metal run is still required before attributing the earlier replay misses to
  application work or claiming that the budgets are met.
- **An output error is clipped on its first painted frame.** The focused playback
  follow-up found text at y=818..830 in an 820-point-high viewport. The next frame
  moved it to y=795..807. Review dynamic status sizing so a new failure is fully
  visible immediately; a later correct frame does not make the first one clear.

The [final visual run](qualification/ui-feedback-2026-09-26.md) passed all nine
scenarios and the shortcut audit, with the Camera and rapid-input warnings retained.
The release run passed navigation budgets and failed both edit-to-picture budgets.
The [retained summary](../tools/ui-feedback/evidence/2026-09-26/summary.json) and
[first error frame](../tools/ui-feedback/evidence/2026-09-26/playback-error-entry.png)
preserve the measured and visible findings; the focused painted-playback and
baseline-comparison follow-up passed without erasing them.
These results establish the tested behavior and identify work still needed; they
do not establish overall usability or native accessibility.

## Priority improvements

| Priority | Desired interaction | Current boundary and evidence needed |
| --- | --- | --- |
| First | Every advertised shortcut reaches its action in the user's environment. | `,i` replaces Cmd+Return. The evaluated Kestrel reservation audit covers globals and the known Ghostty-specific exception; source drift fails explicitly when supplied. Complete native physical-delivery evidence and maintain the audit as bindings change. |
| First | Focus, selected content, Original/Your edit context and requested/displayed positions remain distinguishable. | Replay checks selected-card visibility and exact pane focus after text cancellation. Inspect loaded, pending and error frames and confirm equivalent meaning through native accessibility. |
| First | Common inspector actions are visible and reachable without discovery by scrolling. | Run05 found the Camera button partly clipped at 1280×820. Move it into the visible common-action section or add a clear overflow cue. Preserve pointer-wheel replay and verify actual clipped bounds before clicking. |
| First | A committed edit reaches its picture feedback budget. | Final release Repeat/Hold p95 was 75.26/138.65 ms against 50/100 ms. Investigate the matched commit, decode and composition stages without weakening the targets. |
| First | Rapid commands preserve intent or make rejection clear. | Seven of eight batched Repeat intents were rejected while busy. Keep bounded work and revision-aware targets while reviewing whether successive edits can be retained safely. |
| First | New errors are readable on the first painted frame. | Playback follow-up found the first output-error line partly below the viewport; the next frame was correct. Reserve or calculate the required status height before the error first paints. |
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

The rapid-input scenario now accounts for every edit intent, including explicit
busy-service rejection, and requires a distinct revision and exact result for
every admitted edit. A warning about rejected intents remains useful product
feedback even when correctness checks pass. Help replay checks both scroll state
and changed painted content. Camera cancellation checks the submitted composition
rather than only a closed mode or unchanged revision. The separate `edit-latency`
scenario checks cached Repeat and an 11-frame silent-freeze Hold through the real
commit and offscreen GPU path, then undoes each to the exact authored Original
baseline. Its 50 ms Repeat and 100 ms Hold p95 gates use 40 measured edits per
type after four warm-up cycles; matching commit intervals remain separately
inspectable. The final visual suite passes, while the release run preserves the
two measured edit-latency failures in its report.

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
