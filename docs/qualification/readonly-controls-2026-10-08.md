# Read-only controls, 2026-10-08

Commands refused by a read-only project now finish their native pending state
through the normal typed reply. Each reply retains the submitted request,
ticket, session and target identities. Repeated actions remain usable and
refusals preserve the previous register or private draft.

The service explicitly handles marks, macros, correction saves, target saves
and tracking, generation operations, Original and Edit copies/cuts, proposal
commits, cleanup, relinking, Render and saved-render history. The exhaustive
match requires new request variants to choose an admission and reply policy.
Mark jumps, face detection, Room tone preparation and private Gain, Slip and
Trim previews remain available. Place Slice preparation requires writer
admission and generation Preview persists selection, so both are refused.

## Verification

Run on this Mac, Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies
and the pinned FFmpeg 8.0.3 prefix. [Retained evidence](../../tools/media-qualification/evidence/2026-10-08-readonly-controls/)
includes exact commands, results, logs, source hashes and replay reports.

| Check | Result |
| --- | --- |
| Full app suite with `ui-harness` | 1,083 passed, two existing ignored tests, one runner leak warning |
| Final focused read-only, backup, Render/history and dialog tests | 25 passed |
| Final production keyboard/service/Metal `backups` replay | 50 checks passed |
| Strict app Clippy, all targets, `ui-harness` | Passed |
| Package contents across read-only operations and close | Byte-identical, excluding SQLite shared-memory coordination |

The final replay executable SHA-256 is
`848ba22ff78e9635fdd10c7c3cb50495e1cef50b2a89ef7ceddbc72042476d93`.
It includes the production Kestrel shortcut audit. The replay uses a real
project service and private preview workers. Each cleanup-preview P press
must start exactly one fresh worker before its schema refusal is accepted;
an old message cannot satisfy the retry check.

The full suite preceded later test/replay-only synchronization changes.
Its dialog directory-preparation test passed but nextest reported a leak.
That test passed both an isolated rerun and the final focused run without the
warning. The original cause is unproven. Logs retain the debug compact-unwind
linker warning. The final replay retains 16 second-layout-retry frames and
seven runs of consecutive retries with distinct causes, with no layout failure.
Its intermediate screenshot allowance was reached; semantic checks continued.

## Corrections during qualification

Earlier attempts exposed three fixture mistakes. The first inspected the Slip
Apply accessibility state before its first paint. The second expected an Apply
token to survive an error, although Apply consumes it and preserves the editable
draft; the corrected replay changes the retained proposal through native input
and confirms a fresh Apply becomes available. The third compared package files
while a detached backup from the previous writable session was still finishing.
The fixture now observes all owned backup threads and waits for their completion
before opening the newer-schema view. It still compares backup files.

A diagnostics-only Clippy failure used a dev dependency from `ui-harness` code;
the replay now uses its existing SHA-256 dependency. No tolerance, package-file
coverage or production backup behavior was relaxed to pass these checks.

## Limits

The newer package is simulated by raising the schema number and adding an
unknown table. This does not run a future build or qualify export from its
schema. Native picker selection is scripted. Physical display presentation,
VoiceOver speech, OS IME delivery and device audio are outside this replay.
No release performance result is claimed.
