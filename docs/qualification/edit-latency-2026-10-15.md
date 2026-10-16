# Edit latency on 10,000 beats, 2026-10-15

Follow-up to [store-and-seek-2026-10-15](store-and-seek-2026-10-15.md), which
left the 10,000-beat pause at 65.6 / 75.5 ms (p50 / p95) against the 50 ms
ordinary-edit target of Section 25, and split, wrap and Undo about twice as
slow as on 2026-10-06. These are engineering measurements on one machine, not
release qualification.

## Environment and identity

| Item | Value |
| --- | --- |
| Hardware | Apple M5 Max (Mac17,7), 18 cores, 128 GiB, internal APFS SSD, AC power, no thermal or performance warning recorded; macOS 26.5.2 (25F84) |
| Toolchain | rustc 1.97.1, release, FFmpeg prefix `ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3` |
| Before | `git archive` of `7b646993` (no uncommitted work), built in its own target directory: `perf` `542c3440d4de`, `deadpan-cli` `8f7aa9fcc701`, `deadpan-media-worker` `c57f8b91d11b` |
| After | the same archive with only this change's files laid over it (committed as `d454c7b8`), built in a second target directory: `perf` `2054477b85be`, `deadpan-cli` `70bf8141afcb`, `deadpan-media-worker` `7aa20211b68b` |
| Fixture | `large-10000`, made by each run's own `perf make-large` (10,000 root Background/Silence Holds) |
| Load | Other agents ran builds, tests and GUI apps all afternoon (1-minute load 3 to 19). Every row below states the load at the stage's start; the suite's `--max-load 3` gate passed for all four final runs (none flagged) |

## What dominated

Method: `sample` at 1 ms over `perf edit` (line-table release builds of the
same sources), 60 to 80 cycles on a fresh package, attributing the pause
call site of `perf::edit::run`. The before profile, per pause commit
(1,814 samples in a 12 s window):

| Part | Share | Notes |
| --- | --- | --- |
| Pause command (`apply_validated`) | 64% | of which about a fifth copying and freeing document maps |
| Rehashing the whole history | 19% | `generation_preparations::verify` called `validate_history(Receipt)` on every edit and Undo/Redo: it read and hashed every stored row (including the 16 MB keyframe) and parsed the initial document. Added on 2026-10-07 with AI replacement preparations; it grows with the history, and it is what doubled split, wrap and Undo since 2026-10-06 |
| WAL commit sync | 9% | `F_FULLFSYNC`, durability; kept |
| Command decode, cache eviction | 7% | the evicted revision's document freed on the commit thread |

With the history rehash removed, `_platform_memcmp` was about a quarter of
the remaining pause samples: identity keys in this fixture share long
prefixes (`perf-PID-N`, `large-hold-N`), and every ordered-map lookup made
several library `memcmp` calls. Copying and freeing maps was about 7 ms of a
pause (document copies for the capture, the Split and the insertion, the
store cache, the workspace copy and the plan's binding state).

## Changes

All in `d454c7b8`; see [timing storage](../TIMING_STORAGE.md#shared-collections-and-identities)
and [receipts](../TIMING_STORAGE.md#verified-history-receipts).

1. **Writer verification proportional to the edit.** Commits and Undo/Redo
   use `verify_for_writer`: when the history receipt ends at the head, covers
   every revision and its validator, profile, generation-scope (including
   preparation constraints) and qualification dependencies hold, the rows are
   the ones the writer proved at opening or wrote after their replay checks,
   and only the preparation table is checked against the head. Otherwise the
   complete receipt validation runs as before.
   `compacted_terminal_preparation_still_proves_birth_and_supplies_redo_controls`
   (constraints changed under an open writer, next edit refused, head
   unchanged) passes through the fallback.
2. **Copy-on-write document maps.** `nodes`, `audio_lineage` and the binding
   state's `bindings` and `gap_bindings` are `SharedMap`s: cloning or dropping
   one is a reference count, the first mutable access copies it once.
   Equality is content-only (no identity shortcut), and order and wire form
   are the inner `BTreeMap`'s.
3. **Shared identifiers.** The `identifier!` types hold `Arc<str>`.
4. **Inline identifier ordering.** `compare_identity` compares eight bytes at
   a time; it is exactly the byte order of the text, so every map keeps its
   order.
5. **Binding pruning** merges owners against the sorted nodes and collects
   retained timing identities by reference, skipping the table rebuild when
   every table is retained.

No validation, replay check, receipt extension, history row or authored value
changed; the commit still validates every new state completely, with the
same reuse rules as before.

## Results

`cargo xtask perf --stages edit` (30 cycles each, fixed seed), alternating
before and after, total of commit plus workspace refresh in ms, p50 / p95:

| Run | Load at start | Split | Pause | Wrap | Undo |
| --- | --- | --- | --- | --- | --- |
| Before, round 1 | 2.99 | 34.1 / 45.4 | 61.8 / 71.4 | 33.0 / 43.3 | 32.7 / 41.4 |
| **After, round 1** | **2.86** | **16.6 / 22.2** | **33.0 / 40.3** | **16.1 / 19.1** | **15.8 / 17.6** |
| Before, round 2 | 2.87 | 36.1 / 48.2 | 64.3 / 74.3 | 38.8 / 45.1 | 36.4 / 46.8 |
| **After, round 2** | **2.82** | **18.5 / 25.4** | **33.8 / 44.7** | **16.3 / 21.4** | **16.3 / 20.1** |

All 16 target rows PASS (the suite gates pause on its 100 ms Hold-insertion
row; against the 50 ms ordinary-edit target the after pause p95 is 40.3 and
44.7 ms, the before 71.4 and 74.3 ms). Parts of the after pause (p50 / p95):
store commit 27.2 / 36.2 and 28.5 / 39.9 ms, workspace refresh 4.2 / 4.8 and
4.3 / 4.7 ms (before 50.6 / 62.1 and 52.3 / 64.7, refresh 8.1 / 8.9 and
8.8 / 9.6). The first pause, which binds all 10,000 Holds, takes 123 and
157 ms after and 148 and 165 ms before; it is the maximum, not the p95 (the
29th of 30 samples).

Unchanged by the change: the deferred keyframe (43 to 52 ms, written after
the edit), the final package (43.5 MB, 16.3 MB of document JSON at revision
123) and reopening, which proves all 123 revisions through the receipt
(`replayed` 0; writable 179 ms after, 194 and 214 ms before).

Earlier runs of this afternoon, from a working-tree copy that also held
other agents' uncommitted work and lacked change 4, agree: gated (load 3.0)
pause 37.2 / 48.6 ms against 64.1 / 76.7 ms before, and under load 7 to 10
pause p95 46 to 69 ms after against 73 to 86 ms before.

## Tests

On `7b646993` plus this change (an archive, so no uncommitted work):

- `cargo clippy --locked -p deadpan-core -p deadpan-store --all-targets -- -D warnings`;
  `cargo check --locked --workspace --all-targets`.
- `cargo test -p deadpan-core --locked` (55 test binaries and doctest
  groups), including the `command_work` oracle's random equivalence suite,
  `structural_properties`, the new `identity_order_is_the_text_order`
  (600 random identities plus shared prefixes of 1 to 33 bytes, all pairs
  against byte and text order) and `shared_map` tests (sharing until written,
  content equality including an irreflexive value, wire form).
- `cargo test -p deadpan-store --locked` (42), including revision storage,
  failpoints, chaos kills and generation bundle preparations.
- `cargo test -p deadpan-audio --locked --test timing_representation` and
  `cargo test -p deadpan-plan --locked` (28).

Debug builds also assert the merge-join prune against `contains_key`.

## Not covered and remaining

- The app was not measured interactively; `perf edit` measures the store
  commit and workspace refresh as the native service performs them.
- A history row changed underneath an open writer by another program is no
  longer noticed by that writer's next edit, only by the next open or
  `validate` (the documented trust model for every other read of an open
  store). A changed receipt dependency is still refused at the next edit.
- The remaining pause cost, from a profile of the after sources under load:
  the whole-document validation passes (capture, Split intermediate, resumed
  bindings, final result) about 22%, the capture walk and its scoped table
  11%, one resume clock per later sibling 6%, the WAL sync 11% (3 to 5 ms),
  plan compilation in the refresh 12%; see
  [remaining costs](../TIMING_STORAGE.md#remaining-costs).
- `current_generation_intents`, request allocation and intent cancellation
  still rehash the history through `verify`; they are operational calls,
  not edits, and were not measured.
