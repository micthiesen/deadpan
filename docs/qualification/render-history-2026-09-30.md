# Native saved-render recovery qualification, 2026-09-30

`Renders` and `:renders` expose saved edits, attempts and destinations from the
open project's authoritative store. Eight-row pages run on the project owner;
ticketed replies preserve independent edit, preview and render receipts.
Recovery captures immutable historical identities before a native picker and
uses the existing Retry/Reconcile workflow. Core schema 33 and database schema
42 are unchanged. See [the contract](../RENDER_JOBS.md#native-saved-render-browser).

This advances DP-05, DP-17, DP-18, DP-20 and DP-21. No DP requirement or delivery
gate is complete. Full editing, mastering/effects, HDR, priority scheduling,
app-connected inference and release qualification remain open.

## Production replay and real movies

The final Metal `render` replay passes 112 checks plus the Kestrel shortcut
audit. It closes and reopens a real project before browsing stored work. The
current editor revision differs from the saved render revision throughout
recovery; it remains unchanged.

- A retained-movie retry creates a fresh attempt and freshly verifies bytes from
  the original encoding checkpoint before publishing at a new destination.
- Checking a previous destination uses its captured path and leaves both the
  original movie and retry movie byte-for-byte unchanged.
- Rendering a saved edit again performs fresh automatic admission and encoding
  of that historical edit, with a different checkpoint owner.
- A service-started render is admitted while UI updates are held across a
  close/reopen. Releasing the combined update opens the new session's matching
  status and the movie is published normally.

Four MP4s and their publication reports are retained. Every publication uses the
production finished-file verifier and confirms worker cleanup. This is coverage
of native recovery routing using the existing qualified SDR fixture; it does
not expand the format, picture-effect or acoustic acceptance matrix.

The replay checks actual paint visibility at 960×640 and 1280×820, native widget
Tab/Enter navigation, blocked editor commands inside the modal, cancelled picker
without an extra attempt, and restored pane focus. IME Preedit plus Enter and
Commit plus Space on a focused recovery action cannot activate it. The final
captures show readable saved-edit/destination rows and completed movie paths.
The screenshot allowance warning is retained: intermediate captures stop at its
bound while semantic frames and reserved named captures continue.

## Regression tests and review

Four actor tests cover bounded page continuations, reopen/interruption, committed
movie knowledge after later failure, stale/zero tickets, stale sessions, invalid
ordinals, and preserved command/preview receipts. Recovery tests check historical
revisions, fresh attempt identities, original checkpoint ownership, exact
publication targeting, reopened sessions and engineering-policy refusal. Their
opaque retained-byte fixtures test coordinator admission, not valid encoded media;
the production replay above supplies actual media evidence.

Two pure UI tests reject stale history replies and retain a known movie commit
through later publication failures. Existing command parser coverage now includes
`:renders` and rejects arguments. All 344 optional UI-feature tests pass.
The complete locked workspace run passes 2,578 tests, including doctests, with
none failed or ignored. Strict workspace and UI-feature Clippy and formatting
also pass against the same final code inventory.

Independent review and failed replays led to these corrections:

- Retained workflow receipts are filtered by the current project/session before
  exposing status or active-work controls.
- Session cleanup precedes detection of a coalesced new-session workflow, so
  cleanup cannot immediately hide that workflow's status.
- Composition-owned Enter, Space and Escape are removed before egui button
  activation, while composition events remain available.
- Tab immediately scrolls a focused body action into view. Scroll height uses
  the measured modal header and remains stable across layout passes.
- Cancelling a native picker preserves the browser's entry focus. Closing the
  modal schedules focus restoration for the next outer frame, after its modal
  focus ownership ends.

The final independent read-only review found no remaining actionable issue in
that deferred focus/session handling. Initial fixture compilation errors, three
failed replay reports and the mistaken `ui-check` Cargo feature invocation remain
in the evidence. The valid feature is `ui-harness`. Debug UI-test linking emits
the existing large `__eh_frame`/compact-unwind warning; the tests pass without
suppressing it.

## Native window and destination sheet

A separate developer QA app opened the replay's retained project at its existing
window size. Native `:renders`, Tab/Shift Tab, Enter, Saved edits, Destinations
and a verified attempt worked. Focus was visibly outlined and actions remained
in view. The actual macOS Save sheet opened at Exports with a suggested MP4 name;
Cancel returned to the attempts page. Escape returned to the editor, where `l`
moved Edit 0 to 1/120 and `h` returned to 0/120. No movie was saved or authored edit
made during this native check.

After quitting only that QA app, an inventory-only check confirmed it was gone
while the user's separate Cursor QA app remained running and untouched. All 20
stored tables matched before/after SQLite backups exactly, `quick_check` passed
and the writer lock was released. The native executable was SHA-256
`8b5c50820759cf73ccadb06232a783dc2b4fdf0b4dd6eec629ca1420ff6624c8`.

The browser-role operator inspected inline native screenshots. The documented
CUA surface offers no local image-save path, so these are not retained PNG files.
Native AX exposed the Save sheet, but the custom Saved renders overlay was absent
while disabled editor controls remained listed. Screenshot and keyboard evidence
establish the visible interaction only; native modal accessibility remains an
observed gap requiring investigation. The harness's AccessKit tree does contain
the controls. Real native IME was not exercised; the synthetic replay covers its
input-ownership regression. The earlier hidden Open-sheet observation on the
user's other desktop was not reproduced or resolved by this separate-window test.

## Environment and retained evidence

Host: Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1, Metal and pinned LGPL
FFmpeg at `/tmp/deadpan-ui-ffmpeg/prefix`. Final code source inventory:
`92d7ad2cd912d5ff54a3abe73af9d93686a8ba9dadd4307837f88c9a61ac1b7d`.
The replay executable is SHA-256
`62280e5b3747f83bfa1da8629780c263516c784acaf48295dd3317f73320af74`.
The separate developer QA bundle uses the new app identity and passed actual
Metal startup and the shutdown callback. It still depends on build-host libraries
and is not a signed standalone release.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-render-history/README.md)
includes exact command journals, source inventories, failed and final compressed
replay reports, inspected captures and the four actual exported movies. Project
database retention uses SQLite backup, never a direct copy of a live main file.
Native VoiceOver, real CJK IME, non-US layouts, physical power loss and the full
application concurrency/failure matrix remain unqualified by this increment.
