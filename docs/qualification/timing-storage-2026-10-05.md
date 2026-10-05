# Retained timing storage qualification, 2026-10-05

[Compact timing storage](../TIMING_STORAGE.md) removes the pauses × beats
growth recorded in [performance run 3](performance-2026-10-04.md#pause-history-grows-every-later-edit).
These are engineering measurements on one machine, not release qualification.

## Environment and identity

| Item | Value |
| --- | --- |
| Hardware | Apple M5 Max (Mac17,7), 18 cores, 128 GiB, AC power, no thermal warning |
| Toolchain | rustc 1.97.1, release, FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix` |
| Source | HEAD `d8e0d0a2` plus this change and another agent's concurrent seek, media and xtask work in the shared checkout |
| After | `cargo xtask perf --stages edit --generate --fixture interview=…`; `deadpan-cli` `857eb48c…`, `perf` `00d54dd7…`. Every stage started at 1-minute load 2.73–2.97 (`--max-load 3`), none flagged. |
| Before | The HEAD `perf` example (`d8e0d0a2`, same harness) on fresh copies, the same evening, plus [run 3](performance-2026-10-04.md) for the generated fixtures |

The final review fixes (compact size checks in compound and slice preview, the
elided-head check and refusing older packages that cannot replay) followed the
measured build. They do not run on the measured edit paths.

## Edits on real and generated projects (30 cycles each)

p50 / p95 ms of commit plus workspace refresh, as in run 3.

| Package | Split | Pause | Wrap | Undo | Database | Document |
| --- | --- | --- | --- | --- | --- | --- |
| interview, before (HEAD binary) | 28.1 / 73.9 | 32.7 / 101.4 | 18.8 / 57.7 | 19.2 / 58.6 | 194 MB | 2.24 MB |
| interview, after | 7.6 / 17.9 | 10.0 / 16.2 | 7.0 / 9.8 | 6.9 / 15.1 | 7.7 MB | 0.54 MB |
| gen-1080p60, before (run 3) | 21.1 / 71.4 | 41.4 / 107.5 | 20.9 / 54.7 | 17.1 / 57.0 | 192 MB | 2.24 MB |
| gen-1080p60, after | 7.5 / 16.1 | 16.1 / 24.9 | 7.0 / 11.0 | 6.9 / 10.1 | 9.2 MB | 0.56 MB |
| gen-4k30, before (run 3) | 20.0 / 72.2 | 37.6 / 101.3 | 20.8 / 55.4 | 17.1 / 56.3 | 193 MB | 2.26 MB |
| gen-4k30, after | 10.1 / 19.0 | 16.4 / 24.3 | 9.2 / 16.0 | 8.7 / 20.8 | 8.1 MB | 0.57 MB |

Document sizes are the pretty `to_json` length the harness reports; stored rows
are compact. All after rows PASS the 50 ms edit and 100 ms Hold targets. Cost
still rises with the edited structure (interview pause 6 → 16 ms from the first
to the last tenth), not with retained clocks.

## 10,000 beats (`large-10000`, 30 cycles)

| | Split | Pause | Wrap | Undo | Pauses refused | Database | Document |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Before (HEAD binary) | 1,346 / 1,388 | 1,000 / 1,517 (4 committed) | 1,027 / 1,038 | 999 / 1,016 | 26 of 30 | 4.82 GB | 33.2 MB |
| After | 403 / 457 | 638 / 711 | 349 / 379 | 340 / 375 | 0 | 174 MB | 16.3 MB |

Cost is now flat in commit order (pause p50 638 → 669 ms from the first to the
last tenth). The document stays at its first-pause size: the tree (1.7 MB
compact), one 10,000-node table (2.8 MB) and 10,000 owner bindings (3 MB).
Later pauses add 4–25-node tables. The edits still FAIL: each commit parses,
validates and serializes the complete document several times (about 100 ms to
parse it, 23 ms per validation). Without any pause the same fixture took
91–114 ms in run 1.

## Open with history

| Package | Before | After |
| --- | --- | --- |
| interview after 30 cycles (122 revisions) | 4.5 s | 0.64 s |
| large-10000 after 30 cycles (121 revisions) | 105 s (4.8 GB) | 28.9 s (151 MB) |

Opening still recomputes every command, which dominates the 10,000-beat time.

## Storage composition after 30 cycles on 10,000 beats

| Part | Before | After |
| --- | --- | --- |
| Revision rows | 123 complete pretty documents | 9 stored compact documents (62 MB), 114 elided |
| Navigation patches | none | 30 rows, 8.5 MB |
| History rows | 58 MB per later entry (four complete binding states) | 71 MB in total; later pauses about 0.6 MB, mostly the root Sequence's 10,000-child list in the node patch |
| Timing tables in the head | one 2.8 MB table per pause | one 2.8 MB table, then 4–25 nodes per pause |

## Correctness evidence

- `deadpan-audio/tests/timing_representation.rs`: random sequences of
  InsertTime, Split, Repeat wraps with silent and room-tone gaps, ripple and
  range deletes, MoveRange and undo, over Sources,
  room-tone and silent Holds, Preserve retimes and optional root sounds, at
  30000/1001, 24 and 25 fps. Each step runs compact and inside
  `with_reference_timing_representation`; structure, resolved root clocks and
  bit-exact authored-bus and limited-output PCM must agree, and the compact
  document is never larger. Release, 300 seeds: 1,761 commits (686
  InsertTime, 328 ripple and 159 range deletes, 130 moves, 222 splits, 236
  wraps; 89 wraps refused identically by both) and 2,179 audible bit-exact
  comparisons, all passing. The debug suite runs 4 seeds.
- Core, plan, audio, store, CLI and app suites: 3,389 passed, 0 failed, 2
  ignored before the review fixes; store, CLI and audio reran after them
  (1,296 passed, 0 failed). Strict Clippy is clean on the changed crates.
- 22 tests that assert reanchor step counts or complete tables now run inside
  the reference representation. Two subprocess tests (CLI delete, native
  delete) assert the compact result instead.
- `deadpan-store/tests/revision_storage.rs`: elision at the 16-revision
  keyframes, exact reconstruction of every revision through edits, undo, redo
  and reopen, a tampered navigation patch, bounded pause history, and an older
  package with whole-state binding history refused as `UnsupportedSchema(62)`
  with its database bytes unchanged.
- `audio_binding::wire_bound_tests`: the structural wire-size bound exceeds a
  worst-case binding's actual JSON by more than twice.
- An independent review found no correctness bug in slicing, step omission,
  granular patches or reconstruction. It raised the upgrade refusal, mixed
  size checks and an unenforced head-document invariant, all fixed, and the
  coverage gap, partly closed by the MoveRange and DeleteRange draws.

## Not covered

- Trim, Roll, Slip, edited-slice capture and paste, IsolateGap, Group,
  SetRepeatPlays and beat sounds have no compact-versus-reference property
  draw; they pass their existing suites, including their decoded-PCM tests, in
  compact mode. Generated Sources lack the picture handles Trim requires.
- No full `cargo xtask gate`, ui-harness or painted replay was run.
- Real Caminandes is no longer on disk; the other schema-17–24 review packages
  are unsupported formats.
