# Native Slip observations, 2026-10-01

The root agent launched the isolated developer bundle with `Slip QA.deadpan`
and drove native input through `cua_repl`. The bundle matches the release replay
binary exactly. The app's content was 1280×820 points at 2× scale; the native
capture including its title area was 2560×1704 pixels. The assigned desktop and
window were used without changing desktop settings. Compact 960×640 resizing
was exercised by the separate production replay.

## Native sequence

1. `:source`, then `7l`, placed the independent Original cursor at ordinal 7.
   `:sequence`, Enter and `3l` entered the retained group and selected its first
   five-frame Partition at Edit 3.
2. `:slip +2f` displayed source slate 015 at Edit picture 4 with Original cursor
   8 shown independently. Both root and Source camera paths remained visible.
   The native AX tree exposed the heading, amount, comparison, inspection,
   Apply and Cancel controls with readable names and focus.
3. `b` displayed the saved source slate 013 at the same Edit picture and disabled
   Apply. Another `b` restored Proposed. Tab focused the amount field; Cmd-A,
   `-100f` and Return retained the preview without saving. Its exact report
   clamped the request to -10f. One `l` moved immediately to -9f. Right inspected
   Edit picture 5 while the real Edit cursor stayed 3 and Original stayed 7.
4. Escape restored the selected fragment, group, Edit cursor and Beats focus.
   A consistent SQLite backup matched all 20 pre-preview tables exactly.
5. Reopening `:slip +2f`, then nine native Tab presses, focused Apply. Return
   saved one revision/history entry and preserved both cursors, group and wrapper.
   Native `u` and Ctrl-R each produced one fresh revision. Consistent backups
   verify exact candidate content, inverse restoration and Redo.
6. Cmd-Q exited the first process with status 0. After a fresh launch, Enter and
   `3l` showed source slate 015 at Edit picture 4 again. The saved document and
   every database table matched the preceding Redo snapshot. Cmd-Q exited the
   second process with status 0. No Deadpan app remained in the native inventory
   or process scan, and its writer lock was available.

The first rendered replay exposed missing-glyph boxes for two arrow labels.
The final build spells those key names Left and Right; both native controls were
readable. Full native IME, non-US layouts and VoiceOver were not exercised.
Synthetic composition and focus checks belong to the separate replay evidence.
No audio-device audition, export or full Trim claim is made by this session.
