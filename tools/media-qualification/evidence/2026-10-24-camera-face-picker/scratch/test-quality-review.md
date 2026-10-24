# Camera face picker test-quality review

Lens: meaningful missing failure coverage, false-positive/circular oracles, race coverage, brittle assertions.

## Finding

1. **Minor** — `crates/deadpan-app/src/preview/harness/faces/camera_picker.rs:152-170` does not verify that already-saved target entries retain their indices when face proposals append. This replay starts that append case with zero saved targets and snapshots only the five fixed points. The unit test at `crates/deadpan-app/src/preview/camera/face_picker.rs:439-489` uses generic entries whose `saved` field is always `None`, so it likewise cannot detect a regression that changes the saved-target prefix/numbering. Add an integration case with a saved target covering the displayed picture, then assert its ID, label, and digit are unchanged after proposals arrive and that a proposal remains reachable on its page.

## Review summary

The new replay covers a real detector empty result, cancellation during delayed detection, multi-page keyboard access, target-only save and undo, separate framing and undo, and switching an animated Follow target. No circular-oracle or brittle-assertion issue found. The page-numbering unit test independently confirms all entries are reachable, though it does not distinguish saved entries from fixed entries.
