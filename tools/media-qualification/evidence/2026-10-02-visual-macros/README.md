# Visual macro evidence

Recorded checks for Visual selection, range copy/cut/replacement and counted
semantic macros. See the [qualification report](../../../../docs/qualification/visual-macros-2026-10-02.md).

- `checks/`: command metadata, complete compressed logs and source inventories.
- `replays/`: complete rendered macro/Kestrel report with assertions and state.
- `screenshots/`: inspected 960 by 640 Visual copy, replacement and selection states.
- `review-notes.json`: cross-layer review and corrected failures.
- `scripts/`: exact check and collection scripts as inert text.
- `metadata.json` and `cleanup.json`: host/build identity and process cleanup.

The failed compile and CLI fixture run are retained. `SHA256SUMS` covers every
retained file except itself. Scratch projects and live discovery credentials
are excluded. No physical-input, acoustic, performance or release qualification
is claimed.
