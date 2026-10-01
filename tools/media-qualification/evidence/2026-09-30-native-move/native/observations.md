# Isolated native Move check

The unique bundle `dev.thiesen.deadpan.native-move-qa-20260930` ran the exact
release binary from performance-final; binary.json records the matching hash.
The private package was copied from the closed visual-third replay fixture.
The reserved `dev.thiesen.deadpan.cursor-qa` app/window was never selected,
inspected, moved, resized or relaunched. No Spaces switch occurred.

- Cmd+O opened the private package through the native file picker.
- `gg 20l v 10l y` captured Edit [20,30), without writing a revision.
- Escape cleared selection; `gg 60l :splice` opened Copy at Edit 60.
- `m s` selected Move and the removal site. The proposed main picture showed
  Original slate 030 at Edit boundary 20. `b` showed saved slate 020 there.
- `f` selected the saved insertion site, slate 060 at Edit boundary 60.
  `b` showed proposed slate 020 at Edit boundary 50. Captions and both timeline
  intervals matched each site. Native screenshots were inspected at each step.
- `i l` refined the draft to [21,30), with result [51,60). Escape cancelled
  and restored Edit cursor 60. No cells changed in any of 20 database tables.
- Reopening `:splice m` retained the original register [20,30) and result [50,60).
  Enter committed exactly one revision/history entry. The native inspector
  showed a 10-frame Fragment from original [20,30), and the finished selection
  was Edit [50,60), total 120, cursor 50 and Beats focus.
- `u` restored all baseline authored fields with a new revision.
- `:splice m` after Undo rejected the historical register with an explicit
  older-revision explanation. Enter could not commit. `m` restored a usable
  historical Copy proposal; Escape closed it without changing any database cell.
- Cmd+Q exited the exact temporary executable and released its writer lock.

SQLite snapshots use the backup API. verification.json checks every table for
copy/cancel and historical rejection, all authored fields for Undo, exact
revision/history counts, and 16 unrelated tables across every snapshot.

Native CUA screenshots show the correct picture slates but clip the right side
of the large window in the visible-screen capture. Full layout evidence uses
the separately inspected Metal replay captures. No physical listening, native
IME or complete accessibility claim is added. This test did not play device audio.
