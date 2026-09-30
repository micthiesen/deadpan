# Returning to the retained Original, 2026-09-30

Selecting the already-selected registered video through Browse, its card or
legacy Sources navigation now preserves its Original cursor. It also keeps the
accepted picture and geometry while the refreshed picture prepares. Sound
playback and resume are revoked, and the Edit cursor and selected beat stay
unchanged. Selecting a different video or leaving a raw preview retains the
existing reset to zero. No project format or authored transaction changes.

This fixes the cursor reset observed during
[owner-preparation qualification](owner-preparation-2026-09-30.md). DP-20 and the
complete product requirements remain partial.

## Regression and visual review

The production sound replay now starts at Original 5 and Edit 2. Against the
old code, returning through Browse failed: Original became 0, Edit stayed 2,
and the accepted picture was cleared. The failure is retained.

After the fix, all 39 sound-workflow checks pass, along with the shortcut audit.
The replay uses real registration, widgets, project services and Metal. It
checks both nonzero positions, stopped sound/resume, a held decoder reply with
the original displayed identity and geometry intact, the refreshed frame, and
return through Browse from Your edit. Audio delivery is explicitly simulated;
this does not qualify device timing or listening.

The minimum 960×640 sound state and the default 1280×820 return states were
inspected against the sound-audition design board. The retained synthetic
picture, caption and frame boundary agree. No layout changes were made; the
previously documented small Original-view picture at minimum size remains.

Independent review found no introduced issue in selection, playback revocation,
picture admission or the regression. The separate `:source` view command already
preserves the cursor but still clears the picture during decode when leaving
Your edit. Its transient presentation behavior was outside this selection fix.

## Evidence and limits

The locked workspace suite passed 2,572 tests, including doctests; all 338
optional UI-feature tests passed. Both had zero failures or ignored tests.
Strict workspace/UI Clippy and formatting passed.

In the native single-Original window, Original stayed at 11/120 after sound
selection and Browse, with picture frame 12; Edit remained at 7/120 and the same
120-frame beat. `:source` also preserved that Original position. The separate
legacy keyboard round trip was not completed: an Open-panel attempt coincided
with the user moving the window to its own desktop. Later inspection showed the
original project with disabled controls and no visible Open sheet; Escape and
`l` did not recover input. This is an unresolved native-dialog observation,
not a qualified legacy round trip or evidence of a cursor-fix failure.
Both fixture databases retained every row in all 20 tables.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-source-return/README.md)
records the failing and passing replays, code identities and native observations.
The code inventory is
`d3702bc0e4ab81ead329349308573a68aa0dc028943fd59feee20e80852b3cc1`.
Host: Apple M5 Max, macOS 26.5.2, Rust 1.97.1 and the pinned LGPL FFmpeg
development prefix. Native and replay executables are developer builds.

No performance, acoustic, VoiceOver, IME, non-US layout or release-packaging
qualification is claimed. No shortcuts changed. Native persisted Render recovery
remains the next implementation boundary.
