# Keyboard compatibility

Deadpan reserves Kestrel's global shortcuts. The current whole-Original reuse
binding is **`,i`**, with **`:insert`** retained. `⌘Return` belongs to Kestrel's
Ghostty launcher. Reuse runs once, rejects a count, does not repeat while held,
and does not activate inside text entry or IME composition.

The comma prefix remains usable while browsing the Original so `,i` can copy it
into Your edit. Other edit operators keep the Original non-destructive. Native
text editing retains its own selection, clipboard, undo, and redo behavior.

Catalog placement uses `,s` with no count or key-repeat activation. In Placed
sounds and its inspector, `h/l`, `j/k`, Enter, `+/-` and `dd` act on the event.
Logical Plus accepts no modifier or Shift; Minus accepts no modifier so the
physical fallback for an underscore cannot change gain. Command, Control and
Option variants remain reserved. The `sound-placement` production replay covers
pane entry, native command text and captured targets alongside this routing audit.

The room-tone sheet adds no modified global shortcuts. Its production router
uses plain Enter, Escape and Space, plus Shift+Space. Text fields keep Space;
focused buttons keep native Enter/Space activation. Tab and Shift+Tab stay
inside the modal. Composition and held activation keys cannot apply a draft.
The same router is included in the reservation audit and `room-tone` replay.

Beat gain uses the same logical `+` / `-` keys for counted 3 dB steps, with
Placed sounds taking precedence. Camera keeps its scale keys; Original and
catalog Sound focus cannot edit a retained beat. `:gain VALUE`, `:gain-mute`
and bare `:gain` use the existing native command field and its captured target.
The Gain draft adds no modified shortcut. Plain Enter/Space on its heading
apply/audition; native text fields and focused buttons retain their own input.
Escape cancels outside active composition or popup ownership. Tab and Shift+Tab
use native traversal inside the draft and reveal each newly focused control.
At the boundary, Cancel wraps forward to the heading and the heading wraps
backward to Cancel; surrounding workspace controls remain disabled. The focused
`gain` production replay passes complete populated circuits in both directions
at 960×640 and 1280×820, including actual paint and hit clipping. These injected
events do not qualify physical keyboard layouts or OS IME delivery.

Normal workspace Tab continues to cycle panes. The saved Hold inspector's gain
controls are separately checked through AccessKit Focus events with strict
paint and complete hit-target clips. Native CUA also verified gain boundary
wrapping, keyboard envelope/key editing, field reveal, literal shortcut text and
Escape cancellation without changing the saved project.

## Render

**`⌘E`** and **`:render`** open the native Render flow for the full committed
edit. The shortcut uses exactly Command+E, clears pending prefixes, and does
not repeat while held. Command+Shift+E, Command+Option+E, Control+Command+E and
plain E do not dispatch Render. Native text fields and IME composition retain
their input. The Render button remains available when a preview owns a field;
the final field text is consumed before the preview proposal is captured.

An unsaved Camera, Gain or Room tone preview opens a decision with **Commit
preview and render**, **Discard preview and render**, and **Keep editing**.
Escape chooses Keep editing outside active composition. Tab navigation and
native button activation remain available. Cancelling the destination picker
preserves the preview and creates no render job. Choosing Commit for a changed
preview saves one typed edit, then starts Render from that exact commit receipt. A later render
admission failure still reports the saved revision. A changed project session
or revision rejects the captured choice instead of retargeting it.

The production route lives in `Bindings::key` and the native command parser;
the Camera, Gain and Room tone routers preserve their text and composition
rules around it. The reservation audit therefore exercises the same route.
The local Kestrel `Shortcuts.swift` was inspected for this change: its SHA-256
is `368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`, matching
the checked fixture. It reserves Option+E for Ungroup and has no Command+E
reservation. This source comparison is not a new routing-test or physical
keyboard qualification result. The `render` production replay covers the
shortcut, command entry, preview decisions, stale captures and export flow;
its current run results are recorded separately.

Automatic Render currently uses the SDR picture and audio path and rejects
unsupported content. Closed-project headless Render supports retained-checkpoint
retry, fresh encoding and publication reconciliation. Routing a headless request
to an already-open native project, a native recovery browser, full mastering and
HDR output remain open. See [headless Render](HEADLESS.md#automatic-render).

## Automated audit

The UI feedback runner includes a shortcut audit. The standalone routing tests
also run without a native window:

```sh
cargo test --locked -p deadpan-app navigation::
```

[The audit](../crates/deadpan-app/src/navigation/shortcut_audit.rs) sends each of
the 62 exact global Kestrel bindings through the actual `Bindings::key`, Camera,
text-action, inspector, room-tone and Gain routers. The current source expects
88 routing cases per reservation, or 5,456 total, including pending prefix
states, counts and overflow, text/IME states, and repeat/focus combinations.
The corrected workspace, room-tone, gain and retime runs each pass all 5,456
cases with no conflicts or live-source drift; all 298 app/harness tests also
pass. The [native-gain qualification](qualification/native-gain-2026-09-28.md) retains
source identities, native CUA evidence and remaining physical-input limits.
A reservation fails if it dispatches an action or leaves an editor prefix pending.
This catches shortcuts that seem harmless because they only start a count.

The checked [reservation fixture](../crates/deadpan-app/src/navigation/kestrel-reserved.tsv)
comes from evaluating Kestrel's `ShortcutRegistry.definitions`, including loop
expansion, exact modifier masks, and app scopes. There is no second application
keymap to keep synchronized: the audit calls the same routers used by the UI.
Positive-control tests demonstrate failures for the former `⌘Return` insertion
and Option-number count behavior.

The runner can compare a supplied local `Shortcuts.swift` with the fixture's
SHA-256. A changed source fails the check as **registry drift**, with both hashes
in the report. It does not infer that new Swift definitions are safe. Without a
live source, the report covers the checked registry snapshot only. See
[the UI feedback workflow](UI_FEEDBACK.md) for runner arguments and report files.

## Reserved global keys

Option is `⌥`, Command is `⌘`, and Hyper is Control+Option+Shift+Command. Modifier
combinations are exact; for example Kestrel's Option+H does not reserve plain H.

| Keys | Kestrel use |
|---|---|
| `⌥H/J/K/L` | Window focus |
| `⇧⌥H/J/K/L` | Swap tiles |
| `⌃⌥H/J/K/L` | Move tiles/groups |
| `Hyper H/J/K/L` | Resize split |
| `⌥N/P`, `⌥Space` | Cycle windows or tiled/floating focus |
| `⌥1–5`, `⇧⌥1–5` | Switch Desktop; send and follow |
| `Hyper 1–5`, `Hyper 0` | Set layout share; balance |
| `⌘⌥H/L`, `⇧⌘⌥H/L` | Relative Desktop; send and follow |
| `⌥\`, `⌥=`, `⌥A/D/T/S/E` | Layout structure |
| `Hyper Backspace/T/F/R/D/A` | Layout undo and window actions |
| `⇧⌥Space` | Toggle floating |
| `⌘Return` | Raise Ghostty |
| `⌃⌥Return`, `⌃⌘Return`, `⌥⌘Return`, `⇧⌥Return` | Communications, Mail, Codex, Calendar |
| `Hyper I/N/U`, `⌃⌘W` | Kestrel help, Notes, update, display sleep |

Kestrel's `⌘N` entry is scoped to Ghostty, so Deadpan retains its native New
shortcut. Unknown app scopes or physical keycodes in a refreshed fixture fail
validation until reviewed. Option-number inputs cannot become Deadpan counts;
Shift-only logical digits still work on keyboard layouts that require Shift.
Logical punctuation such as `,`, `:`, and `?` retains layout-aware routing.

Kestrel intercepts physical key positions, while egui routes logical keys. This
audit uses the registry's ANSI positions and verifies Deadpan's input semantics;
it does not prove OS event delivery for every input source. Physical non-US
layout delivery, real IME events, and global event-tap behavior still require
the small native acceptance check. Changing to physical shortcut routing inside
Deadpan would break its existing logical-symbol contract.

## Refresh after a Kestrel change

Use the explicit path to a trusted local Kestrel `Sources/Kestrel/Shortcuts.swift`:

```sh
python3 crates/deadpan-app/src/navigation/export-kestrel-reservations.py \
  /path/to/kestrel/Sources/Kestrel/Shortcuts.swift \
  > /tmp/deadpan-kestrel-reserved-new.tsv
diff -u crates/deadpan-app/src/navigation/kestrel-reserved.tsv \
  /tmp/deadpan-kestrel-reserved-new.tsv
```

The exporter evaluates that Swift source with data-only dependency shims, using
the macOS developer Swift toolchain. Run it only on trusted registry source. It
does not start Kestrel, install bindings, or modify dotfiles. The Rust audit
itself only hashes the supplied source and never executes it.

Review the changed definitions, replace the fixture, update the expected binding
count if needed, and rerun the audit. Unknown source grammar is not parsed or
silently skipped; a registry API change makes the exporter fail until its shims
are deliberately updated. Exporter tooling is for development and is not an
end-user application requirement.

The 2026-09-27 refresh reviewed Kestrel commit
`dc9245c09db3c101a2419c364174449b6e4415ae`. It changed only the Ghostty New-window
help description. Re-evaluating the live registry produced identical reservation
rows; only the source digest changed. The failed pre-refresh audit remains in the
[moment-paste qualification](qualification/moment-paste-2026-09-27.md).

## Interaction direction

Keep plain Vim motions and the comma leader for editor actions. Put common keys
beside their controls and show valid next keys while a prefix is pending. Use
native Command shortcuts for lifecycle and history where Kestrel leaves them
available. Preserve a clear focused-pane cue separately from selected content.

Original Visual selection and range reuse now have the `original-moment` replay;
its current native/visual evidence limits remain explicit in qualification.
General Visual replacement, targets, and Trim still need their own replay
scenarios, visible selection scope, reversible previews, and measured feedback.
Their specification bindings do not establish implemented capabilities. Any new
modifier binding must pass this audit and its real interaction scenario before
being added to help or keycaps.
