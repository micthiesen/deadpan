# Catalog sound recovery

Source base `fec738e4`, Apple M5 Max, macOS 26.5.2, Rust 1.97.1 and the
qualified FFmpeg 8.0.3 prefix. Implementation, review and verification were
performed by the sole working agent. No document or database schema changes.

## Change

The native `:relink` flow could restore a missing sound, but its fallback for
a file still present on disk always selected the Original. A damaged catalog
sound therefore had no explicit native repair target.

`:sound-relink` captures the focused catalog sound's asset, original content
identity, location version and project session when command entry opens. It
also captures absence. The command opens a sound-specific picker and uses the
existing verified managed restore or linked relink worker. Different bytes
refuse; stale sessions, removed targets and changed locations cannot retarget
the command. It creates no authored revision and changes no sound placement.

Recovery now distinguishes audio-only catalog media from the Original and
compatibility-view videos. A sound-only missing-media report says **Sound
missing**, names that sound and focuses **Locate sound…**. Restoring the
Original schedules its picture even if an unrelated sound remains missing.
Import guidance also lists the already-qualified WAV, MP3, AAC and Opus paths.

## Verification

The [retained evidence](../../tools/media-qualification/evidence/2026-10-11-sound-relink/README.md)
contains commands, failures, final source and fixture hashes, executed replay
binary identities and selected final captures.

- The final app run passes 1,103 tests. Three unchanged, expensive keyboard
  audit tests passed in the preceding run and were excluded from repetition:
  **1,106 distinct passing app tests across the two runs**. Two normally skipped
  tests remain skipped. Strict app Clippy passes with and without `ui-harness`;
  formatting passes.
- The new service test imports a linked 44.1 kHz mono WAV into a profiled
  project, verifies its bookmark and linked ownership, renames it,
  verifies automatic relinking, rejects a stale location version, and commits
  an explicit identical-file relink. Both operations preserve the complete
  authored document and Original record; the closed package validates fully.
- The `sound-relink` Metal replay imports and places a real stereo WAV,
  damages its retained bytes without changing length, refuses a different
  replacement and repairs from the identical file. It compares complete bytes,
  both cursors, the selected Original and sound, and the unchanged revision.
  It then removes both media files, restores the Original while the sound
  remains unavailable, restores the sound through its recovery report, and
  rejects stale and absent command-entry targets after asynchronous Open.
  The final run passes **22 checks** (21 workflow/layout checks plus the
  keyboard audit); its app SHA-256 is
  `6fad6d4e80e50b740ba87ca61cba6c2c76c1b03e5c2fc7f5cedcd9e555487b2a`.
- The existing Original relink replay passes with the final production code.
  Subsequent changes affect only the sound replay fixture. The recovery
  replay passed on the preceding build. Every replay audits the production
  keyboard router against the pinned Kestrel reservations. No local
  `Shortcuts.swift` was available for a current-source comparison.

Each keyboard audit passes **21,884,016 routing cases** against 62 reservations
with zero conflicts. The final sound replay records 11 frames needing a second
layout retry and three runs of consecutive retrying frames with distinct input
or state changes. Every painted frame settles; no retry repeats for three
frames without new input. These warnings were reviewed, not suppressed.

The [repaired sound](../../tools/media-qualification/evidence/2026-10-11-sound-relink/replays-final-3/sound-relink/sound-relink-043.png)
and [missing sound report](../../tools/media-qualification/evidence/2026-10-11-sound-relink/replays-final-3/sound-relink/sound-relink-057.png)
were inspected directly: the retained picture is visible, the report accurately
names the sound, its text fits and the locate action has a visible focus cue.

The first compile caught two harness integration mistakes. The command-reference
rewrite deliberately failed its test to require diff review, then passed in the
final run. The first two sound replays tried to click a catalog row while the
rail was still scrolling after placement. The first run timed out after the
command correctly refused placed-sound focus; an explicit button/focus check
in the second run exposed the moving hitbox. The corrected replay focuses the
stable pane heading, selects through production `j` routing and asserts the
catalog identity before repair. The next run passed all repair checks but its
stale-session fixture attempted to reopen an already-open project, correctly
receiving a no-op; the fixture now closes and reopens before submitting the
captured command. All initial results remain retained. Debug linking reports
the existing large unwind-table
warning; strict Clippy has no warnings.

These runs use real files, SQLite, verified media workers and offscreen Metal.
Picker answers are scripted. No live GUI session, physical keyboard, OS input
method, VoiceOver speech or listening was exercised. No encoder, native adapter,
dependency or packaging change required a new media corpus, sanitizer run or
bundle qualification. DP-15 remains partial pending its consolidated acceptance
audit; this record does not close the product goal.
