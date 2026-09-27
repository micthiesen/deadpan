# Keyboard compatibility

Deadpan reserves Kestrel's global shortcuts. The current whole-Original reuse
binding is **`,i`**, with **`:insert`** retained. `⌘Return` belongs to Kestrel's
Ghostty launcher. Reuse runs once, rejects a count, does not repeat while held,
and does not activate inside text entry or IME composition.

The comma prefix remains usable while browsing the Original so `,i` can copy it
into Your edit. Other edit operators keep the Original non-destructive. Native
text editing retains its own selection, clipboard, undo, and redo behavior.

## Automated audit

The UI feedback runner includes a shortcut audit. The standalone routing tests
also run without a native window:

```sh
cargo test --locked -p deadpan-app navigation::
```

[The audit](../crates/deadpan-app/src/navigation/shortcut_audit.rs) sends each of
the 62 exact global Kestrel bindings through the actual `Bindings::key`, Camera,
text-action, and inspector routers. Its 3,472 cases include all current pending
prefix states, counts and overflow, text/IME states, and Camera key repeat. A
reservation fails if it dispatches an action or leaves an editor prefix pending.
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
