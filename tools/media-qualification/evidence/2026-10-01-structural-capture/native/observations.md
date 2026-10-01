# Native structural copy/cut check, 2026-10-01

The isolated debug/UI-harness bundle is identified in `binary.json`. It uses the
same production native controls, store, decoder and Metal picture path as the
passing debug replays. This was functional QA, not a responsiveness measurement.
The app was opened only for this check and quit afterward.

The copied, closed `delete-range` fixture had a 20-frame prefix followed by two
nested ordinary Sequence groups containing a 100-frame fragment. The initial
Visual selection at root was cleared without editing. Two native Return keys
entered `delete-outer group / delete-inner group`. `gg v 10l` then selected
global Edit `[20,30)` in that supported scope.

- `d` saved one cut, selected its join at Edit 20, changed group length 100→90
  and total 120→110, and published the ten-frame historical register. The native
  screenshot showed Original slate 030 at that join.
- `:splice` displayed historical endpoint slates 020 and 029. Its scope path
  stayed in the nested group. Escape cancelled; all 20 database tables matched
  the post-cut backup exactly.
- `p` appended the register after the remaining fragment at Edit 110. The new
  selected group had bounds `[110,120)` and total duration returned to 120.
  Its native displayed slate was 020. One new revision/history entry was added.
- `u` removed the paste; another `u` restored the cut. Each step received a
  fresh revision. Full authored-document comparisons match the corresponding
  earlier state except that revision. All 16 unrelated tables stayed unchanged.
- `gg y` with no Visual range copied the entire restored 100-frame direct child.
  `:splice` displayed `[20,120)` and endpoint slates 020/119. Cancel and this copy
  left all 20 tables identical to the post-Undo backup.
- Cmd-Q exited the app. The first immediate inventory still reported shutdown
  in progress; the next inventory contained no Deadpan app. Process verification
  and a nonblocking writer-lock acquisition independently confirmed release.

`native-snapshot.py` uses SQLite's backup API and queries the completed backup,
so each retained database/document pair is consistent. `verify-native.py`
checks all seven snapshots. This check adds no audio device, listening, native
IME, VoiceOver or release-latency evidence. Native screenshots were inspected
through CUA but not saved. Their right side was outside the returned visible
capture area; complete minimum/default layout evidence comes from the retained
offscreen Metal replays. No native full-window layout claim is made.
