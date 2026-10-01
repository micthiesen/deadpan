# Native marks check, 2026-10-01

The isolated release bundle is identified in `binary.json`. The fixture is a
copy of the closed final rendered project: one 110-frame fragment of the
120-frame Original, with a ten-frame prefix removed. CUA sent native key events
to `dev.thiesen.deadpan.marks-qa`; read-only SQLite backups capture each result.

- `20l m` showed a persistent pending m and Edit boundary 20. `q` saved it.
- After `20l`, `'q` returned to Edit 20. Ctrl-O returned to Edit 40; Ctrl-I
  returned to Edit 20. The native screenshot showed source slate 030 at Edit 20.
- `:source`, `5l`, `mQ` saved a distinct uppercase mark at Original ordinal 5.
  Its persisted exact PTS is 5005 at time base 1/30000.
- `'q` restored Edit 20; `'Q` restored Original 5. The retained edit remained
  110 frames and its picture/time structure stayed unchanged.
- `:marks` showed the separate q and Q rows, their Edit/Original domains and
  the captured Original boundary 5. Native Tab focused Save this position.
- Ctrl-O from the Marks modal closed it and returned to Edit 20. Ctrl-I returned
  to Original 5. Reopening the list and Escape cancelled without any database
  change. All 20 tables match the post-Original-mark backup exactly.
- `:unmark q` removed only q in one saved transaction and retained Original 5.
  `u` restored every mark field; `'q` returned to Edit 20. Other authored fields
  and all 16 unrelated tables stayed unchanged throughout.
- Cmd-Q exited normally. After restarting the same bundle/project, `'q` and
  `'Q` restored Edit 20 and Original 5. Reopen/navigation changed no table.
- The app was quit again. Both native processes exited 0. CUA's app inventory
  contained no Deadpan instance, the process scan was empty, and a nonblocking
  exclusive acquisition verified that `.writer.lock` had been released.

This is a bounded native keyboard and presentation check. It does not establish
OS IME, physical non-US layout, VoiceOver, physical listening, signing or release
packaging. Native screenshots were inspected in the tool session; the evidence
package retains the equivalent rendered UI captures and these measured snapshots.
