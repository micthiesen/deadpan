# Native accessibility audit, 2026-10-05

DP-20 requires native accessibility: meaningful labels and values, standard
focus indicators, keyboard-only operation and respect for UI scaling, Reduce
motion and Increase contrast ([specification §19–§20](../spec/DEADPAN_SPEC.md)).
This record audits the running app through the macOS Accessibility API, lists
the defects found, the changes made, and what remains unverified.

## Method

Hardware: Apple M5 Max, macOS 26.5 (Darwin 25.5.0). The debug app ran on a
scratch project created headlessly from
`native/deadpan-source/tests/fixtures/cfr-bframes.mp4`
(`project create-original`, then `deadpan-app --project`), never on the
Documents library. No screenshot API was used. Evidence came from:

- a small Swift AX inspector (`AXUIElementCopyAttributeValue` over the
  application element) that printed role, subrole, title, description, value,
  focus, enabled state, actions and bounds for every node, and the focused
  element;
- a Swift AX observer (`AXObserverCreateWithInfoCallback`) recording
  `AXAnnouncementRequested`, focus, value and title notifications with their
  announcement text and priority; this is what VoiceOver receives, without
  running VoiceOver;
- keyboard input through System Events key codes (`Tab`, `Shift-Tab`, arrows,
  letters, Return, Escape, ⌘ shortcuts), sent only to the app's own process;
- offscreen replay through the production router (`--ui-check`), whose
  AccessKit tree is the same tree the macOS adapter receives, and its PNG
  captures for visual review.

VoiceOver itself was not started: driving it would require granting
AppleScript control of VoiceOver, a separate privacy approval, so spoken output
and VoiceOver navigation commands remain unverified. System Settings were not
changed; Reduce motion and Increase contrast were exercised through explicit
preferences in replay and unit tests, while the native app reads the live
`NSWorkspace` values.

## Audit findings before changes

| State | Observation |
| --- | --- |
| Start screen | **Crash.** The second `Tab` aborted the app whenever an accessibility client was attached: `accesskit_consumer` panicked with "Focused ID … is not in the node list". The pane cycle moved focus to Placed sounds, which the start screen never draws, and egui reported that missing control as the tree's focus. Any VoiceOver or Voice Control user would lose the app on its first screen. |
| Start screen | The focused viewer was an image labelled only "No picture displayed"; the start actions (⌘N, ⌘⇧N, ⌘O) were separate static texts. |
| Workspace | Panes are painted; the focus targets were buttons named "Picture viewer pane"/"Current group beat outline pane" etc. with no value. Moving through beats with `j`/`k` or frames with `h`/`l` changed only painted content, so nothing was spoken. Focus order: Viewer → Inspector → Beats → Placed sounds → Original → Viewer (Shift-Tab reverses). |
| Viewer | Image labelled "Showing sequence frame 1" with no domain, time, beat or mode. Camera mode did not expose that the picture was an unsaved draft. |
| Beat cards | Toggle buttons labelled "Beat 1: cfr-bframes, Original, 120 frames, half-open frame boundaries 0 to 120" with selected state; acceptable, but not focusable themselves (the Beats pane owns keyboard focus). |
| Status | "Project opened", "Inserted a 15 frame silent pause … and saved", "Undo saved" and "Could not complete action: …" were plain static text: nothing announced saves or refusals. A Camera refusal while Placed sounds had focus was only visible. |
| Inspector | Labels/values as separate static texts in reading order; truncated values (`.truncate()`) exposed the elided "…" text. |
| Sound effects | The search field had no accessible name (placeholder only). |
| Monitor | Slider value spoken as the raw gain `0.125`. |
| Help | Opening Help left focus on the pane button and announced nothing; the Help window itself is a labelled group. |
| Models | Opening `:models` left no focused element; three packs repeat "Install from folder…" / "Install from archive…" without their pack. Tab order inside the panel is correct and Escape restores pane focus. |
| Modal sheets | Recovery, Models, Marks, Room tone, YouTube and Render preview sheets were unlabelled groups; focus landed on a button ("Reopen a11y  Enter") without the sheet's purpose. |
| YouTube | The URL field is labelled and focused by ⌘⇧N; its validation message ("Only HTTPS YouTube URLs are supported.") was not announced. |
| Trim | Good: a labelled keyboard-controls button, In/Out/Slip/Roll toggles with values, amount field, labelled junction images and waveform. The four toggles are exposed as toggle buttons rather than a radio group. |
| Render | ⌘E opens the native save sheet (fully accessible, background disabled). |
| Recovery | After a forced kill, the launch offer appeared with focus on Reopen; the Project recovery report appeared on reopen with focus on Continue. |
| Scaling | egui's ⌘+ / ⌘− / ⌘0 zoom works (text and controls scale, picture area shrinks), but Help did not mention it. |
| Motion/contrast | The app ignored Reduce motion (animated reveals, blinking cursor, rotating spinners) and Increase contrast (1-point borders at 1.6:1 against the canvas). |

The footer's key hints are pairs of static texts ("h l", "frame"). VoiceOver
reads them in order; they were left unchanged.

## Changes

| Area | Change |
| --- | --- |
| Focus validity | `Tab`/`Shift-Tab` skip panes the latest layout that drew any pane did not draw (`Pane::cycle_available`); a pass that draws no pane never re-enables them. Omission is deliberate: an open Gain draft replaces the Beats outline, and the start screen has no Inspector or Placed sounds. After drawing, focus left on an undrawn pane moves to the current pane or the nearest drawn one (announced as a focus change), and `accessibility::guard_focus` remains a last resort that gives any other stale target a node, so focus can never abort the macOS adapter. |
| Pane summaries | Every drawn pane target carries a value: the viewer gives domain, frame, exact time at the project rate, selected beat and Camera draft state ("Camera draft, unsaved. Enter applies, Escape cancels"); Beats gives "Beat 2 of 3: Pause, Hold, 15 frames (0.500 s), boundaries 10 to 25"; Inspector, Original and Placed sounds summarize their content. The current pane is a polite live region unless playback is running, a native control has focus or a key is held, so `j`/`k`, `h`/`l` and Camera entry are announced, and a held motion key is announced once at its final position on release. Picture loading is not part of the summary, so each step is announced once. The notice and the header's save state use fixed IDs, so a spinner appearing beside them cannot turn them into new, re-announced nodes. |
| Start screen | The empty viewer is "Picture viewer: no project open" with the ⌘N/⌘⇧N/⌘O instructions as its value. |
| Status | The notice line is a live region: polite for saves and finished work, assertive for "Could not complete action". The header's "Not saved" storage refusal and YouTube failures are assertive; YouTube URL validation is polite. |
| Sheets | Models, Marks, Room tone, YouTube, Render preview and the three recovery sheets are AccessKit dialogs (macOS `AXDialog`) labelled with their purpose and bounded by their measured area. Opening `:models` focuses Close inside the dialog (the list stays at its top); the AI PICTURES offer still focuses that pack's first license, and falls back to Close when the pack draws no primary control; each pack is a labelled group, so repeated button names are heard with their pack. Opening Help announces its scroll and Escape keys. |
| Names and values | Truncated labels expose their full text. The sound and source search fields are named. The monitor slider speaks "12.5 percent monitor volume". |
| Reduce motion | Followed from `NSWorkspace.accessibilityDisplayShouldReduceMotion`, re-read when the window regains focus and at most once per second on frames the app paints anyway, with no wakeups of its own and no restyle unless a value changed: no window/collapse animation, immediate scroll reveals, no cursor blink, and a static "…" (named "Busy") instead of every rotating spinner. |
| Increase contrast | Followed from `accessibilityDisplayShouldIncreaseContrast`: 1.5-point borders in `#8a93a6` (≥3:1 against canvas and panel), secondary text (all former hard-coded `MUTED` uses, now `RichText::weak()` or `style::muted`) at full text colour, a 2-point hover stroke and a 3-point yellow keyboard-focus/active stroke. Painted cards, keycaps, the viewer frame and sheets use the same contrast-aware border. |
| Scaling | Help lists ⌘+ / ⌘− / ⌘0 for interface scale and notes the followed display settings. |

Replays and tests construct the app with fixed display preferences, so a
developer's own Mac settings never change a replay result.

## Evidence

Live app after the changes (AX inspector and observer, same scratch project):

- Start screen: six `Tab` presses cycle Viewer → Beats → Original with no
  abort; before the change the second `Tab` aborted the process.
- The observer recorded `AXAnnouncementRequested` for "Inserted a 15 frame
  silent pause at boundary 10 and saved", "Beat 1 of 3: cfr-bframes,
  Fragment, 10 frames (0.333 s), boundaries 0 to 10" on `k`, "Beat 2 of 3:
  Pause, Hold, …" on `j`, "Undo saved", "Camera draft, unsaved. Enter
  applies, Escape cancels. Your edit, frame 6 of 120, …" on `,f`, the Help
  keys on `?`, and "Could not complete action: Unknown command: notacommand…"
  at high priority (90, others 50).
- The Project recovery report and launch offer are exposed as `AXDialog`
  windows titled with their purpose, focus on Continue / Reopen.
- `:models` focuses Close; Escape returns focus to the viewer image.
- ⌘+ enlarged text (the "Saved" label grew from 37×15 to 41×17 points) and ⌘0
  restored it.
- `deadpan-app --smoke-test` passed on Apple M5 Max (Metal).

Automated:

- 8 new unit tests in `preview::accessibility` (focus guard, live regions and
  pane values, full truncated text, drawn-pane tracking, Reduce motion and
  Increase contrast style and restore, static busy indicator, WCAG contrast of
  the new border, exact spoken time; drawn-pane tracking also covers a Gain
  draft omitting Beats and a pass that draws no pane) and
  `tab_skips_panes_the_layout_does_not_draw`.
  `cargo test -p deadpan-app --locked`: 874 passed, 1 ignored.
- New `accessibility` replay, 40 checks, passing: valid AccessKit focus on four
  start-screen Tabs and repair of a focus left on the undrawn Placed sounds
  pane, the viewer's spoken position and beat with a polite live setting, a
  changed value after `l`, a held `l` changing the value silently and speaking
  once on release, the painted lavender viewer focus ring and visible focus cue,
  only the focused Beats pane speaking, polite save and assertive refusal
  notices, Help's announcement, the Models dialog/group/focus and focus return,
  and with explicit Increase contrast + Reduce motion a 3-point focus ring,
  raised borders, zero animation time and, at the 960×640 minimum window, a
  footer that meets the notice panel on its first frame with footer keys and
  pane titles fully painted. Its captures (start screen, Beats focus, Models
  focus, high-contrast Models, workspace and minimum window) were inspected.
- After review fixes, `cargo xtask replays` (jobs 3) passed 21 scenarios with
  3,244 checks: accessibility, workspace, camera, menus, keymap, transcript,
  original-layout, place-slice, marks, groups, sound-placement, room-tone,
  gain, ai-pause, model-packs, slip, trim, render, recovery, storage-failure
  and youtube. An earlier model-packs failure (focusing the first install
  control scrolled the list past its heading) was fixed by focusing Close.
- Strict Clippy (`-D warnings`, all targets) for `deadpan-app` with and
  without `ui-harness`, and rustfmt, pass. The full workspace gate was not run.

## Remaining and unverified

- VoiceOver speech, rotor navigation and VO-key commands were not exercised;
  announcement requests and attributes are verified, not what VoiceOver says.
- Beat cards are not individually focusable; the Beats pane announces the
  selected beat instead. The footer's key/description pairs are read as two
  texts. Trim's In/Out/Slip/Roll are toggle buttons, not a radio group.
- Focus arriving on a pane can be spoken twice (focus change and the live
  value), depending on VoiceOver's handling of button values.
- Reduce motion and Increase contrast were verified through explicit
  preferences, not by toggling System Settings on this Mac. The grey key text
  inside command buttons (`action_text`, 7.9:1 against the canvas) is not raised
  under Increase contrast.
- No Linux build was checked: the aarch64 Linux cross-check stops in `ring`'s
  C build without a cross toolchain. The macOS-only code was reviewed by hand
  (`objc2-app-kit` and `system_preferences` are `cfg(target_os = "macos")`,
  `follow_system_display` is ungated).
- Interface scale is not persisted across launches, and large scales were not
  checked against the minimum window size.
- The YouTube details/confirm sheet, render progress and render history were
  not driven live (they need a network or a long render); their sheets gained
  dialog labels only.
- Voice Control, Full Keyboard Access, Switch Control, CJK IME and non-US
  layouts remain unqualified.
- An app instance stopped responding to Quit after `cargo build` replaced its
  running binary; later instances launched from a copied binary quit normally.
  This matches the replay runner's reason for running private copies and is not
  attributed to the app.
