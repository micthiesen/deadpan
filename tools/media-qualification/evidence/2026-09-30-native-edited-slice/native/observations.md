# Isolated native edited-slice QA

Bundle `dev.thiesen.deadpan.edited-slice-qa-20260930`, release binary identity in
binary.json. The reserved `dev.thiesen.deadpan.cursor-qa` app/window was never
selected, inspected, moved, resized or relaunched. No Spaces switch occurred.
A copied retained replay project lived only under this task's scratch directory.

- Production keys `gg 10l :hold 11f` authored the fixture Hold, duration131.
- `gg 8l v 17l y` captured Edit[8,25),17frames. AX reported successful copy.
- Escape cleared the selection; `gg 60l :splice` opened insertion at Edit60.
- `i l o 2l` refined to[9,27),18frames. AX reported separate copied source
  and destination clocks. The screenshot showed endpoint slates009/015 and
  main copied endpoint015. Source pictures were decoded by the native worker.
- Escape cancelled. Reopening started with original register[8,25),17frames.
- The same refinement then Enter committed an18-frame Sequence at[60,78),
  selected Copied contents with3child beats and total edit149.
- `u` restored the131-frame fixture. A second `u` restored the120-frame baseline.
- The accepted register survived both Undos. Reopening :splice and pressing o
  showed historical copied endpoint013 for copied Edit frame25, with endpoint
  slates008/013. It did not substitute current Edit frame24.
- Cancel and quit released the writer lock. Post-quit CUA observations reopened
  the same temporary bundle with no project; a final quit without another UI
  observation exited it, confirmed by the exact bundle executable process path.

Consistent SQLite backups prove copy/cancel changed no cells in20tables,
placement created exactly1revision, placement Undo restored all fixture authored
fields except fresh revision, and the second Undo restored the entire baseline.
Sixteen unrelated tables stayed identical throughout. Counts and assertions are
retained in verification.json. Actual native input/AX checks add no listening,
IME or complete accessibility qualification. Native screenshots were inspected
in CUA but its visible-screen capture clipped the right part of the large window;
full viewport/layout evidence uses the retained Metal replay images.
