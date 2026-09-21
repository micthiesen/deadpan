# Exact source picture mapping qualification, 2026-09-21

This change adds independent exact picture duration, selected-span endpoint
policy, reversible node/occurrence commands, core schema 7 and database schema
12. [The contract](../SOURCE_VIDEO_MAPPING.md) distinguishes picture sampling
from beat occupancy and inverse boundary resolution. Authored import and every
full-product requirement remain open or partial.

## Behavior exercised

- At 30000/1001 fps, a one-second Source rounded to 30 frames retains natural
  picture time. Project frame 15 requests 31031/2 source ticks, selecting the
  interval beginning at 15510. Historical fitting would request 15500 and select
  the preceding interval.
- A 3/2-frame exact mapping under a two-frame beat uses its explicit endpoint
  policy at the last frame center. Holding stays within the chosen trim. Exact
  boundaries, trims inside source intervals, negative timestamps, missing
  coverage and mismatched clocks have separate regression cases.
- Exact inverse anchors compose through nested retimes and repeated occurrences.
  Property tests compare against an independent elapsed-time formula. Holding
  never clamps out-of-host anchors or rebinds marks. Single-play edits retain the
  other plays, beat duration and audio mapping, and their inverse restores the
  complete original document.
- Missing mapping/policy, nonpositive or overflowing durations, extra/null
  fields, and still/blank targets fail. The headless CLI exercises dry runs,
  reopened exact queries, durable undo/redo and stale revision rejection for both
  independent stream mappings.

The native headless integration test uses the actual `rgb25_24.mp4` fixture.
Selecting original frames `[1,24)` occupies 28750/1001 project frames, rounded to
29. Every selected frame passes through the persistent native decoder and its
RGBA bytes are compared with the independently authored fixture formula.
Historical fitting selects different frames at project positions
`2, 7, 12, 17, 22, 27`. This demonstrates the timing difference in decoded pixels.

An initial version of that test assumed a finer source clock and attempted an
integer half-frame trim. The measured clock is 1/24, so the assertion exposed
truncation in the test setup. The final fixture uses measured integer PTS
boundaries; production mapping code did not change for that correction.

## Migration evidence

The [schema-11 fixture](../../crates/deadpan-store/tests/fixtures/v11-history.sql)
was produced by a rebuilt CLI and store harness from archived commit
`8a6c264e447615c585a2b9f849edfe44f51504fd`. That old runtime reports core schema 6
and database schema 11, and independently reopened and validated the result.
Two generation runs reproduced the SQL byte for byte. The
[fixture provenance](../../tools/media-qualification/evidence/2026-09-21-video-mapping/fixture/README.md)
retains exact sources, commands and hashes.

The fixture contains 24 revisions and 13 edits, direct and occurrence audio
mappings of 60000/1001 and 120000/1001 frames, offsets -137 and 2401, three marks,
an abandoned branch and pending redo. Tests compare every old snapshot, command,
forward/inverse transaction, history cursor and operational generation/original
row. All existing audio decisions remain intact while picture gains `FitBeat`.
New picture mapping commits, undoes, redoes and reopens after migration.

Negative cases cover newer picture fields/commands in old documents, inserts
and patches; missing/null or altered audio mappings; altered offsets; incompatible
schema tags; and corrupt or missing original records. Failures retain the
unchanged source database and consistent pre-migration backup. Media bytes are
outside this metadata fixture; history validation does not qualify media readiness.

## Verification scope

All 458 Rust tests passed, with no failures or ignored tests. Formatting, strict
workspace Clippy, the locked workspace build and headless doctor passed on the
Apple M5 Max / 128 GiB / macOS 26.5.2 host with Rust 1.97.1. Doctor reports core
schema 7 and database schema 12. The
[gate report](../../tools/media-qualification/evidence/2026-09-21-video-mapping/gate/report.json)
and compressed logs retain the exact commands and results. The
[source manifest](../../tools/media-qualification/evidence/2026-09-21-video-mapping/source-manifest.json)
binds tested source, manifests and fixture bytes to their base revision.

Independent general, timing and migration reviews found no actionable issues.
[Review dispositions](../../tools/media-qualification/evidence/2026-09-21-video-mapping/review.json)
record their checks and limits. Two reviewers lacked the native FFmpeg prefix;
the root's standalone and workspace native runs supply that execution evidence.

No native decoder implementation, application startup or GUI behavior changed.
The new integration test uses the existing qualified decoder against a committed
fixture. Sanitizer, Python, listening, device, aesthetics, keyboard, IME and
accessibility checks were not repeated for this authored-state change. Prior
[source-preview GUI evidence](source-preview-2026-09-21.md) remains limited to
its tested surface. Native video resource admission, qualified stream receipts,
common A/V origin, project-basis selection, authored import, playback and export
remain required.
