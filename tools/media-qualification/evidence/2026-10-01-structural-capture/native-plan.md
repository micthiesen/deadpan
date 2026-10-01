# Native keyboard check

This is the initial plan. The retained replay package contained nested groups;
the executed scope and outcomes are recorded in `native-qa/observations.md`.

Use a private copy of the closed `delete-range` replay package and a uniquely
named local developer bundle. Record the built binary identity. No user project
or other app state is needed.

1. Open Your edit, normalize to frame 0 and take a consistent SQLite backup.
2. Use `20l v 10l d` to cut `[20,30)` through actual native keys. Check 110 frames,
   the saved cut register, cursor 20 and rendered Original frame 30. Back up.
3. Inspect the historical copy with `:splice`. Check its Original endpoint
   slates 20 and 29. Cancel and prove all database tables are unchanged.
4. Use `p` after the selected suffix to append the copied ten frames. Check
   duration 120, selected inserted range `[110,120)` and original frame 20.
   Back up, Undo paste, back up, Undo cut and back up.
5. With no Visual selection, use `y` on the restored Original beat. Inspect
   its complete endpoints with `:splice`, then cancel. Prove the copy/preview
   writes no database rows and Undo restored all authored fields except revision.
6. Quit the QA app, verify its process is absent and acquire/release its writer
   lock. Record native observations separately from rendered replay captures.

The zero-duration/equal-time workflow is covered by actual production-key Metal
replay at minimum/default sizes. This native check targets OS input and the
saved-cut/whole-child workflow; it does not imply IME, VoiceOver or listening QA.
