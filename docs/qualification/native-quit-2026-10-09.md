# Native application Quit

Source base `04d2180c`, Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1,
pinned winit 0.30.13 and FFmpeg 8.0.3. Core format 47 and SQLite schema 75
are unchanged. Work and review are by the sole working agent.

## Implementation

`deadpan-lifecycle` owns the native termination boundary. It adds a missing
`applicationShouldTerminate:` selector only to the checked winit delegate,
retains the original object and preserves its termination callback. A
competing handler is never overwritten. The host publishes bounded static
preview names after input and layout. A native alert can veto Quit without
borrowing the app or mutating a project. Reentrant Quit and changed captures
refuse discard. Application-menu Quit now invokes the same AppKit method as
system Quit; window close retains its existing egui prompt and async drain.

Apple documents the delegate's [termination decision](https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationshouldterminate(_:)).
`NSTerminateLater` enters a [modal run-loop mode](https://developer.apple.com/documentation/appkit/nsapplication/terminatereply/terminatelater?language=objc).
The pinned winit observer defers queued event handlers outside the default
mode, so an egui confirmation cannot simply be scheduled from that reply.
This adapter uses `NSAlert::runModal` and returns a final decision instead.

Unsafe is confined to registering the checked Objective-C ABI and managing
one alert-only local event monitor. The monitor retains its window and
buttons, borrows each live event only during the callback, returns either
the original pointer or null, and removes its token once on the main thread.
It handles Escape, Tab/Shift-Tab and focused Enter/Space while preserving
Command/Control/Option routing. No global event tap or system setting changes.

## Verification

Two pure policy tests pass: stale/reentrant confirmation, cancellation,
one-use discard, unchanged frames, bounded input and generation overflow.
The complete app/lifecycle UI-feature run passes 1,097 tests (236.355 s,
eight slow, two existing skips). Formatting and strict workspace Clippy pass.
The final default-feature app/lifecycle Clippy check and six focused policy,
menu and close-state tests pass too (1,054 unselected tests). The debug linker
retains its existing compact-unwind warning for an `__eh_frame` over 16 MB.
The first compile caught generated API
constant names and object-reference conversions; a monitor compile caught a
borrowed window and was fixed by retaining it. Logs remain in
`/tmp/deadpan-quit-*-20261009.log`.

Native tests use only Accessibility, with the developer wrapper
`/tmp/deadpan-quit-native-20261009/Deadpan.app`. The tested debug binary
is `8e77afdd0d3a7931c7eebd1d845975b3638cf0f48ce3783d04846cbc9cd4d466`.
They establish:

- Unsaved command text is named in the native Quit alert. Return and Escape
  keep it, repeated Quit works, and text focus returns without changing it.
- A changed Camera X of 51 survives cancelled Quit; its draft and viewer
  focus remain. This used the preceding monitor build with the same snapshot
  and cancellation path.
- Tab focuses Keep editing, another Tab focuses Discard, Shift-Tab returns
  to Keep editing and Space preserves the command. Two Tabs followed by
  Return explicitly discard it and the app process exits.
- Escape after dismissing the alert once again cancels ordinary command
  input, proving the alert monitor no longer consumes editor input.
- No-draft native Quit exits without confirmation.

Native testing caught two issues that code-only tests could not: setting a
default button replaced its Escape key equivalent, and default native Tab
traversal skipped buttons with this Mac's keyboard navigation setting.
The scoped monitor supplies both safe cancellation and explicit focus
traversal. Reversing the key-equivalent assignment fixed Escape but disabled
Return; that intermediate build was replaced, not treated as passing.

The computer-use tool timed out reading both `com.apple.dock` and `Dock`.
Native AppKit Quit is exercised through the application's real Cmd-Q/menu
selector. This is not a claim that the Dock UI itself was operated. Physical
logout, speech, OS IME and physical keyboard delivery remain owner checks.

The final release app is
`33c84977b65291c590259f141c7e7e3369855707b25a61d8a4ed76e0af9516e9`.
The sorted changed-Rust-file map has SHA-256
`4a05acd25bcbd6b75f404e721fde9ceb2477ddaa01dafcfc295afaa3f2a4eb54`
and is retained at `/tmp/deadpan-quit-source-20261009.json`.
Release `storage-failure` and `recovery` each pass 13 scenario checks plus
the Kestrel audit. Both retain three second-pass command-footer layout
retries; recovery also reports one run of three consecutive input frames
with distinct retry causes. Neither has a failed or ignored-retry paint
check. These offscreen replays exercise the existing window-close path;
native termination is tested separately below. Reports and the executed
binary remain under `/tmp/deadpan-quit-release-20261009`.

The same release executable in the native developer wrapper reopens the
project from the encoding trial below as Saved, without a crash offer.
Camera `,f` then `l` produces X=51. Cmd-Q names Camera in the native alert;
Return preserves X=51 and viewer focus. A repeated Cmd-Q while asking and
Escape also preserve the draft. Two Tabs focus Discard, then Return exits.
The process is gone, the authored JSON still equals `render-before.json`
and full project validation passes. Results are retained in
`release-after-discard.json` and `release-validate-after-discard.json` under
the native evidence directory. This developer wrapper does not qualify
packaging, physical logout or a clean machine.

## Quit during real encoding

The native app opened a disposable project with a 100,000-frame silent tail
added through the shared command API. A real public Render request routed
through its authenticated owner. After hardware qualification, the retained
progress and process table both showed active encoding of 100,120 frames.
Native Cmd-Q terminated the app and its separate encoder; neither remained
in the process table. A read-only database query, before reopening, found
attempt `45e462d4-b4f1-488c-a086-ae294aa59feb` in `cancelled` with
`cancellation_requested=true`, sequence 4 and no diagnostic. The authored
JSON before and after is identical at revision
`720a1b0d-baca-4857-b82e-ae657fae088f`, full validation passes, no output was
published and the native launch PID was removed from the journal.

The observing CLI exited 1 with `HostOwnerChanged` and “The project owner
is closing or unavailable” before it received a terminal status. This is
retained as an unknown-delivery outcome, not a successful CLI cancellation
reply. The durable attempt independently proves completed cancellation.
No shutdown-latency claim is made from the tool round trips.

Evidence is retained in `/tmp/deadpan-quit-native-20261009`:
`render-quit.jsonl`, `render-quit.stderr`, the before/after process tables,
`render-before.json`, `render-after.json`, `render-validate-after.json` and
`render-shutdown-verification.json`. This uses the final debug binary above;
it does not claim a model job or every helper type was active at exit.
