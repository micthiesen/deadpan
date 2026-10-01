# Native Source Slip evidence, 2026-10-01

This checkpoint verifies the native stopped-picture `:slip` workflow on backend
commit `8c282ac8a3147e784b212852540b5e44e28382b1`. See the
[qualification](../../../../docs/qualification/native-slip-2026-10-01.md) for
behavior, results and remaining scope. Full Trim and release gates remain open.

## Results

- Focused Slip tests: 17 passed.
- App with `ui-harness`: 464 app tests and 3 headless integration tests passed.
- Default app: 428 app tests and the same 3 headless integration tests passed.
- Final formatting and strict workspace/all-target Clippy: passed.
- Final visual replay: 67 Slip checks plus the Kestrel audit passed.
- Full release replay: 3,655 checks passed across 22 executed app scenarios and
  Kestrel. Generated-picture qualification explicitly needs a separate fixture.
- Kestrel: 17,360 routing cases, 62 reservations, no conflicts or live-source drift.
- Native cancel, Apply, Undo/Redo and reopen: passed. Both processes exited 0;
  the final app/process inventories were empty and the writer lock was released.

Overlapping suites are reported separately. Unit/default manifest:
`9d93fbf93dc03951747ea1b2c300f06af54f5cfb470f60bca987259f78bf78b4`.
Final build/lint/replay manifest:
`44ed42b615d9adebf9da17a5bc7419d688ec2df0c9fe9ca73ed9917ef7ddff36`.
Only two arrow-label strings changed between them. The first replay's missing
glyphs, source reconstruction and final readable controls remain in this record.

## Inventory

| Path | Evidence |
| --- | --- |
| `checks/` | Every command journal, complete compressed log and source manifest; parsed suite counts in `summary.json`. |
| `replays/` | Full first/final visual and full release JSON/HTML reports, compressed. |
| `images/` | Inspected Proposed, Before, compact clamp, committed and nested captures; first missing-glyph example; semantic index. |
| `native/` | Six consistent SQLite backups, exact documents, table/history summaries, fixture identity, verifier result, observations and cleanup. |
| `sources/`, `source-proof.json` | Exact two-label reconstruction and unchanged backend/build proof. |
| `environment.json` | Host/runtime/release identities, all attempts and native outcomes. |
| `performance-summary.json.gz`, `replay-summary.json.gz` | Counts, findings, explicit skips and timing summaries. |
| `scripts/` | Compressed collection, launch, snapshot and verification scripts. |
| `*-review.md` | Independent service, UI and harness review conclusions. |
| `SHA256SUMS.json` | SHA-256 of every retained file except the seal itself. |

The first visual run passed assertions; its two arrow glyphs were corrected after
inspection. No command/test attempt failed in this increment. The debug link's
existing `__eh_frame` warning and the passing Gain replay's AAC `get_buffer()`
diagnostic are retained in full logs; their presence is not hidden by summaries.

Native testing used the same release binary as the performance run, SHA-256
`c1d93f04b775841e092ead66b6a520918f52de5f9fb1e1ce66d9cae87f399eb6`.
The developer bundle and live scratch project are excluded from this archive.
The bundle retains host dependencies and does not establish release packaging.
Physical IME/non-US layouts, VoiceOver, audio audition and full Trim remain open.
