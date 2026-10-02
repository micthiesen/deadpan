# Native Trim focus and cancellation

2026-10-01, Apple M5 Max / macOS 26.5.2. Default-feature debug binary SHA-256
`af4c3cf7414db129f03409b824b654f2822c5695fa320d5931e5255633eae6fe`.
Source manifest `9291e1ead7429b00123743fbfc57a26891ac6f468fe066d5a4d63dd94b24fc24`.
The recorder retains the command, source inventory and clean exit in
`checks/native-trim-native-before-focus.json`. The app ran while workspace
tests compiled, so this is functional observation only, without latency or
audio-device qualification. No audio transport was started.

The existing assigned desktop and window bounds were preserved. The inspected
native screenshot was 2560 by 1704 pixels including chrome. CUA observations
remain in the conversation; no native screenshot file was saved.

1. Entered Original, moved to ordinal 7, returned to Edit, entered the ordinary
   group and moved to Edit frame 3. `,v` opened the zero In/Ripple draft.
2. `l Tab l Tab l Tab l r` retained In/Out/Slip/Roll = 1/1/1/1 under Overwrite.
   Proposed outgoing Edit 16 / Original 27 and incoming Edit 17 / Original 3
   matched fixture slates 026 and 002. Project delta was zero with one silent
   filler. Both captured editor cursors stayed fixed.
3. `b` displayed Before outgoing Edit 14 / Original 24 and incoming Edit 15 /
   Original 1. Apply was disabled. Toggling back restored Proposed.
4. Native amount entry `e`, Cmd+A, `-100f`, Enter clamped Roll to zero while
   retaining I/O/S = 1. The field acknowledged +0f. Feedback reported requested
   -100f, executable 0..94f, and the right-beat picture-start handle. Proposed
   outgoing Edit 15 / Original 26 and incoming Edit 16 / Original 2 matched
   fixture slates 025 and 001. No authoring occurred.
5. Native Tab reached Loop, Apply, Cancel and the feedback viewport. Immediate
   Tab then Up retained feedback focus in this OS run. This did not reproduce
   the egui no-idle first-focus issue identified in review; the production replay
   separately tests that exact event sequence.
6. End revealed the final handle and Project opened lines, with a visible focus
   ring and complete bottom content. Home then Escape restored the ordinary
   selected Source, group and Edit frame 3. No warning or modal remained.
7. SQLite backup API snapshots compare all 20 tables exactly equal to the entry
   snapshot, retaining 8 revisions and 5 history entries. Cmd+Q exited with code
   zero; a process scan found no Deadpan executable and the writer lock was
   available. See `cancel-verification.json`.

This check does not establish save/Undo/Redo/reopen, listening, VoiceOver,
physical IME, non-US layouts, or the later focus correction.
