# Native combined Trim release verification

Source inventory: `a51de6546df98588d032ba45789341d74f4b47e12bb99fee6932dca07d449f71`.
Normal optimized executable SHA-256:
`9bcbcdede5a68b09cabd97c4305e4a071354637b6b2ecee6c0d8bfaff247f71e`.
Bundle ID `dev.thiesen.deadpan.cursor-qa`; developer wrapper retains external
host-library dependencies. Window bounds and desktop assignment were not changed.
All input used native CUA; no shell-generated keyboard or mouse events.

## Actual observations

- Set Original cursor to zero-based 7 (visible frame 8), entered the retained
  ordinary Sequence and set Edit cursor to 3.
- `:trim edge=slip delta=+2f mode=ripple` produced In/Out/Roll zero, Slip +2,
  explicit exterior outgoing slot and incoming Edit 1 / Original 13 (slate 012).
  The complete waveform covered Edit samples [0,36000).
- Shift-Space started real native output. A recorded 12-second observation
  interval after startup elapsed with no input or compilation. A subsequent
  focus change still showed Pause, and Space then changed it to Audition.
  The stopped waveform showed a nonzero heard-position marker. The pair, retained
  Original frame 8 and entry Edit cursor 3 stayed fixed, with no failure notice.
- Space resumed and the native Pause button stopped it, independently verified
  by the Audition caption. This supports the unoptimized-preparation explanation
  for the debug Starved observations. It does not isolate the precise expensive
  stage, measure acoustic output, or qualify general playback throughput.
- One Return applied the displayed proposal. The native view returned to the
  captured group/Source and Edit cursor 3. Its actual screenshot showed slate
  015, consistent with the +2 source slip at that retained Edit position.
- `u` then Ctrl-R showed Undo saved and Redo saved. SQLite backup snapshots were
  taken after Apply, Undo and Redo; no open main database file was copied.
- Cmd-Q closed the app. Reopening the same package succeeded; navigation into
  the group retained its 14-frame Source and 120-frame right neighbor.
- A second Cmd-Q closed the reopened instance. Both recorder commands exited 0.
  No Deadpan process remained and the writer lock was independently acquired.

`history-verification.json` compares every table at reopen and exact authored
state at Undo/Redo. Apply changes history from 5 to 6 entries and revisions from
8 to 9; Undo/Redo create fresh revisions 10 and 11. The saved complete intent is
Slip +2, all other values zero, Ripple, with the exact captured group and right
sibling. Both audio/video source mappings move from -10 to -12. Duration, edit
window, right Source and parent group are unchanged. Operational tables remain
unchanged. Reopening changes no table.

Native captures were inspected in the conversation; no native PNG is asserted
in this evidence directory. Retained offscreen captures separately qualify
picture, control, waveform and feedback paint. The later feedback-only wheel
reveal guard is verified in its own actual egui replay, not by this binary.
