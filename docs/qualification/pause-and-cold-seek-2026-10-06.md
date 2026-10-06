# Pause latency and cold seek, 2026-10-06

Follow-up to [command-work-2026-10-05](command-work-2026-10-05.md), which
left 10,000-beat pauses at 116.8 ms p95, and to
[seek-2026-10-05](seek-2026-10-05.md) and [proxy-2026-10-05](proxy-2026-10-05.md),
which left cold 4K seeks at 333–444 ms and the exact Original picture 390 ms
p95 after a proxied request. This record covers:

- [pause work scoped to its changes](../TIMING_STORAGE.md#pause-work-scoped-to-its-changes):
  a provisional slice instead of the complete-structure timing table, the
  inserted leaf's durations derived from the base's, and hashed or
  merge-joined identity lookups in the passes that remain;
- [compact history rows](../TIMING_STORAGE.md#compact-history-rows): one copy
  of a changed root's child list per history row instead of four;
- one verified copy of the Original per decoder instead of two: picture and
  audio sessions adopt the store's verified private snapshot;
- pipelined snapshot verification (BLAKE3 and reading overlap the copy and its
  SHA-256);
- 16 interactive codec threads above 1080p instead of 12;
- a 30 ms refinement rest for an isolated request, keeping 150 ms while the
  cursor keeps moving ([proxies](../PROXIES.md)).

These are engineering measurements on one machine, not release
qualification.

## Environment and identity

| Item | Value |
| --- | --- |
| Hardware | Apple M5 Max (Mac17,7), 18 cores, 128 GiB, AC power, no thermal or performance warning |
| Toolchain | rustc 1.97.1, release, FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix` |
| Source | HEAD `83f7153b` plus this change and another agent's concurrent DP-01 project-safety work in the shared checkout (backups, bookmarks, database schema 67; tracked diff `17bd157e…`). None of it is on the edit or seek path except the schema |
| Official run | `cargo xtask perf --stages edit,seek --fixture gen-1080p60=… --fixture gen-4k30=…`: `deadpan-cli` `9e0a1dd2499d`, `deadpan-media-worker` `eba787ef43a3`, `perf` `fb4d5560e4a6`. Every stage started at 1-minute load 2.1–3.0 (`--max-load 3`); none was flagged |
| Fixtures | The seek record's generated movies (x264 `veryfast`, keyframes 250 pictures apart), imported with each build's own `project create-original`: `gen-4k30` 3840×2160 30 fps 60 s, `gen-1080p60` 1920×1080 60 fps 120 s. `large-10000` is made by `perf make-large` |
| Before | HEAD `83f7153b` built from `git archive` in a separate target directory (`perf` `b08a0342360e`, `deadpan-cli` `b1fad328723c`; the harness differs only by the `--admissions` option added here). Its edit runs alternated with this change's at the same load (4–6, ungated); its cold seeks ran at load 1.7–4.0 |

## 10,000-beat edits (30 cycles each)

p50 / p95 ms of commit plus workspace refresh (`total_ms`), as in earlier
records.

| Run | Split | Pause | Wrap | Undo | Database |
| --- | --- | --- | --- | --- | --- |
| Official before ([command-work](command-work-2026-10-05.md)) | 28.4 / 32.6 | 80.7 / 116.8 | 31.6 / 36.8 | 25.1 / 27.9 | 102 MB |
| Before, HEAD binary, three rounds | 28.0–29.0 / 31.3–32.2 | 78.3–79.1 / 110.5–118.5 | 31.5–31.7 / 34.4–36.1 | 24.8–25.2 / 26.3–28.5 | 102 MB |
| After, same load, alternating | 19.9 / 25.8 | 42.2 / 79.0 | 19.1 / 22.2 | 17.1 / 19.5 | 50 MB |
| **After, official xtask run** | **18.8 / 24.7** | **40.9 / 81.2** | **18.0 / 23.2** | **16.0 / 18.2** | **47 MB** |

In the official run all four PASS: split, wrap and undo against 50 ms and
the pause against the 100 ms Hold target. The first pause, which binds all
10,000 Holds, takes 142 ms (221–244 ms before). A pause's commit is 35.3 ms
p50 and the refresh 4.7 ms.

The p95 sample is one specific pause: revision 65, the run's only keyframe,
serializes and writes the complete 7.8 MB document (81 ms; the other pauses
take 30–52 ms). Without that keyframe the pause p95 is the 52 ms pause that
shifts nearly every root sibling.

Generated projects stay well inside both targets: edits 3.8–4.1 ms p50
(p95 4.1–5.0 ms) and pauses 10.9–13.3 ms (p95 15.2–15.9 ms).

### Where a pause's time went

Phase timings on the release build (temporary instrumentation, removed):

| Phase | Before | After |
| --- | --- | --- |
| Capture and its validation | 12–27 ms (complete-structure layout, index and byte count) | 4.7–6.0 ms (provisional slice; validation of 10,000 owners remains) |
| Split and its validation | 7–13 ms | about 7 ms, then about 5 ms with the hashed passes |
| Resume terms | 1.3 µs per later sibling | unchanged (required by the authored semantics) |
| Leaf insertion | complete structural pass | about 3.3 ms, mostly two document clones and lineage |
| History row | about 590 KB (four child lists) | about 170 KB (one) |

A sampling profile (`sample`, 1 ms) of the remaining commit shows the four
binding validations, the Split's validation and clones, the resume
resolution and SQLite's `F_FULLFSYNC` commit as the largest shares; none is
above 10% of a pause.

## Cold seek

`cold.progressive`: a new picture session (store, plan, verified Original
snapshot, decoder) to the first visible picture at a random frame, decode plus
Metal completion. Page-cache-warm, so INFO.

| Fixture | Before, HEAD binary (n=20) | After, alternating (n=20) | **After, official (n=10)** |
| --- | --- | --- | --- |
| gen-4k30 | 344 / 444 ms (load 1.7); 391 / 3,520 ms (load 4.0, two multi-second outliers) | 272 / 374 ms (12 threads, one copy) | **193 / 275 ms** |
| gen-1080p60 | 235 / 256 ms | 178 / 196 ms | **125 / 149 ms** |

Removing the second copy saved 60–115 ms; pipelined verification and the 16
threads above 1080p the rest. Both rasters now complete a cold seek inside
the 300 ms §25 target at p95 (INFO, page cache warm). The proxy's own cold
first picture is 13 / 22 ms (reader to picture), unchanged.

## 4K warm seek and refinement

| gen-4k30 | Before ([proxy run 6](proxy-2026-10-05.md), 12 threads) | After (official, 16 threads) |
| --- | --- | --- |
| Original warm seek p50 / p95 | 103 / 208 ms (FAIL) | 88.5 / 167 ms (FAIL) |
| Proxy warm seek p50 / p95 | 7.85 / 8.24 ms (PASS) | 8.20 / 8.57 ms (PASS) |
| Exact Original seek in refinement | 143 / 220 ms | 96 / 163 ms |
| Request to exact picture p50 / p95 / max | 307 / 390 / 391 ms (150 ms rest) | **146 / 213 / 216 ms** (30 ms rest) |
| Peak footprint of the stage | 1.17 GB | 1.34 GB |

A warm 4K Original seek is decode-bound (the main thread waits on codec
threads 80% of the time; conversion is 10%, Metal 7%), so the Original alone
still misses 80 ms; the stopped viewer shows the proxy first. 1080p60 warm
seek PASSes at 30.2 / 47.7 ms with 12 threads.

The refinement samples are isolated requests: each follows the previous exact
picture by more than 150 ms, so the worker rests 30 ms (and, in the app, until
the viewer has taken the proxy reply, which the harness does at once). While the cursor keeps
moving (a request within 150 ms of the previous one) it still rests 150 ms,
so held keys never start an exact decode they would abandon; a key's first
repeat comes after the 163 ms exact seek.

## Equivalence and tests

- `FrozenAudioLayout::capture_scoped`:
  - debug builds compare every provisional table with the complete capture:
    slicing to its aliases is equal, and the node, lineage and run counts
    and the exact wire length are the complete layout's;
  - `scoped_capture_slices_like_the_complete_capture_on_random_documents`:
    400 random trees of Sequences (some empty), Holds, Repeats and
    Partitions, random alias sets and every subset; at least 100 use a slice;
  - `complete_wire_count_equals_serialization` checks the node-by-node count
    for every kind at extreme values;
  - debug builds assert that a provisional table is read only through an
    alias it projects, and `has_node` equals `nodes.contains_key`;
  - `local_command_work_equals_the_reference_on_random_edit_sequences` now
    also requires at least 20 provisional captures and one extension, with
    identical transactions, results and refusals against the reference.
- `structural_durations_after_root_leaf` is compared with the complete pass
  wherever it is shared in debug builds.
- `deadpan-audio` `timing_representation` (compact against reference
  representation, identical clocks and bit-identical PCM) passes.
- `command::patch_wire` tests: compact and complete forms decode to the same
  transaction for 3 to 501 children; a kept inverse round-trips; out-of-range,
  overlapping and non-Sequence splices are refused. Three store tests changed
  with the stored form: a tamper test edits only the forward patch (the
  inverse is implied), a literal-history test compares decoded rather than
  byte-identical transactions, and the pause history size check expects
  more than 150 KB, not 300 KB, for the first pause.
- `an_adopted_private_snapshot_serves_like_a_copied_one` (media) and
  `a_moving_cursor_waits_the_full_rest_before_refining` (app worker).
- UI replays (`cargo xtask replays`): `proxy-seek` first FAILED with the
  30 ms rest ("No jump to the end showed the proxy picture": the exact reply
  replaced the proxy reply before the replay took it). The worker now also
  waits for the proxy reply to be taken; `proxy-seek` then PASSED (14
  checks), as did `workspace`, `delayed-preview` and `rapid-input`.
- Suites: `cargo test --locked` for `deadpan-core`, `deadpan-store`,
  `deadpan-plan`, `deadpan-media`, `deadpan-cli` and `deadpan-playback`
  (all passed), `deadpan-audio --test timing_representation`, and the app's
  `worker` tests (44 passed). Strict Clippy (`--all-targets -D warnings`) for
  those crates and for `deadpan-app` with and without `ui-harness`.
- `structural_properties::generated_sequences_commit_every_command_family`
  occasionally reported `IsolateGap` never committed, in 3 of 24 runs of the
  unchanged HEAD source too (random seeds); see the review follow-up.

## Review follow-up

An independent review found no path to a committed bad document; these were
then tightened:

- Store forgery tests that edited `$.inverse…` with `json_set` had created a
  partial inverse and passed because it failed to parse. They now rewrite the
  row with a complete explicit inverse (and keep the stored document bounds
  covering it), and fail for their intended reason: four for a reused
  allocation namespace, one for measured geometry.
  `explicit_history_inverses_are_checked_against_the_forward_patch` accepts a
  complete correct explicit inverse and refuses a complete wrong one
  ("stored command, patches, and revision disagree").
- Scoped-capture parity compares results, not only successes, and
  `scoped_capture_matches_the_complete_capture_at_ten_thousand_children` checks
  counts, wire length and every slice subset against the complete capture in
  release as well as debug builds.
- The composite path extends its own provisional table, and a pause whose
  table still missed an alias would repeat its leaf with complete captures
  instead of refusing (debug builds assert it never happens).
- A child splice has one encoding (explicit `"after": null` refused); the
  writer compares the inverse field by field instead of building the reversed
  patch; the phase-only byte bound is documented and debug-asserted; the
  root-leaf derivation debug-asserts that no other node changed; the identity
  hasher starts from a per-process random offset, with its trust premise
  documented; snapshot adoption is `from_store_verified_snapshot`
  (doc-hidden, recomputes the SHA-256 in debug builds; a true cross-crate seal
  would need the store's copy loop in the media crate); a raw source preview
  serves above 1080p with 16 threads; scrubbing is classified by request
  arrival, not worker start.
- A second official run after these fixes (`deadpan-cli` `2248927eb38c`,
  `perf` `1401f3504a76`, tracked diff `79f0c404…`, every stage below load 3, none flagged) confirms the
  targets: 10,000-beat split 18.4 / 24.7 ms, wrap 18.4 / 23.3 ms, undo
  16.2 / 17.9 ms and pause 40.5 / 76.2 ms (all PASS); generated-project edits
  under 5.2 ms p95 and pauses under 16.9 ms; 1080p60 warm seek 30.0 / 47.7 ms
  (PASS); 4K proxy warm seek 8.3 / 8.6 ms (PASS); 4K refinement 148 / 215 ms;
  cold completion 130 / 149 ms at 1080p60 and 193 / 276 ms at 4K (INFO); the
  4K Original's own warm seek still FAILs at 87.1 / 166.8 ms.
- `structural_properties` runs with a fixed seed, and the command-family
  coverage test commits Repeat, play count, gap and `IsolateGap` in a directed
  prelude, so it no longer depends on the run.

## Not covered

- A full `cargo xtask gate`, the complete app test suite and the UI
  performance replays.
- Sharing one verified snapshot between the preview, playback and proxy
  builder: each still makes one store snapshot (the proxy builder still
  copies once more, in the background).
- The keyframe commit (about 40 ms on 10,000 Holds) and the remaining
  whole-document passes, which bound pause p95 on a wide root.
- Page-cache-cold seeks, native-window display latency and lower hardware
  tiers.
