# Whole-Original semantic macros

Source base `6dda3c47318e204093be08ef066fa6628591aea1`, Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1.

`InsertOriginal { asset }` records whole-Original reuse as a semantic operation.
The project host checks the initialized workflow Original, its qualification,
the staged asset record and the retained receipt before deriving full A/V timing
at the project rate. It does not reinterpret the Original as a copied moment:
audio lead/tail, outward enclosure and terminal picture holds remain intact.
Each invocation allocates fresh identities, inserts after the selected direct
child or at the current Sequence's end, and never replaces a Visual selection.
Counted execution remains one reversible transaction and leaves registers intact.

Native `,i` and `:insert` use that path in one-Original projects. Explicit legacy
projects retain their existing insertion path, which recording refuses before
submission. Original-pane insertion captures and rechecks its view, pane, focus,
cursor, session and revision. Like other compound semantic edits, it requires
the register bank to be loaded so the host can validate that bank remains unchanged.

Independent review checked qualification, timing, allocation, atomicity and
native integration. It prompted explicit capture/revalidation of the Original
view and sound/event focus. Regression coverage exercises those captures and
refuses later view, pane, cursor and sound-focus changes.

The release visual and isolated performance `macros` replays each pass 401
scenario checks plus the Kestrel audit. They record one whole-Original operation,
run it twice as a 360-frame edit, undo once to baseline, and insert from the
Original pane without losing its cursor. Reports remain in
`/tmp/deadpan-dp18-macros-visual` and
`/tmp/deadpan-dp18-macros-performance`. The performance binary is SHA-256
`4e77a7cfbb85aba7acc3d5b6c0d1f6a57d1be635d0d8cc29753d6044679e375a`.
The visual binary precedes the Jobs-only wording/harness fixes and is SHA-256
`bc7425a815a4339825ebea7f849f9d701a7f5a8949446a55ae13d989c461839f`.

Small-fixture UI frame CPU p95 is 0.908 ms over 2,803 frames; input-to-commit
p95 is 37.220 ms over 32 samples. These measurements do not qualify full-size
media or physical keyboard/display latency. Reports retain footer layout retry
warnings; no assertion failed. Core fresh-ID/placement/inverse tests and the
service full-A/V/unchanged-bank test pass in the final 330-test targeted gate,
as do normal and UI-harness strict Clippy. Clippy prompted boxing the full
source allocation payload; two old test helpers were updated to inspect an
owned request before consuming it. These subsequent storage/ownership changes
do not alter the UI behavior exercised by the retained release replays.
This closes this specific macro surface gap, not all of DP-06.
