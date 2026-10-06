# Command work, diagnostics and idle, 2026-10-05

[Command work reuse](../TIMING_STORAGE.md#command-work-reuse) removes repeated
whole-document passes from Split, Repeat wrap and pause commits, and from the
store's commit check. Process-local [diagnostic counters](../PERFORMANCE.md#diagnostics)
and a native `:diagnostics` panel are new. This record also measures the
release app idle with a project open. These are engineering measurements on
one machine, not release qualification.

## Environment and identity

| Item | Value |
| --- | --- |
| Hardware | Apple M5 Max (Mac17,7), 18 cores, 128 GiB, AC power, no thermal or performance warning |
| Toolchain | rustc 1.97.1, release, FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix` |
| Source | HEAD `04d38fd3` plus this change and other agents' concurrent work in the shared checkout: transcript, shot and rail analysis, and a register-bank digest (database schema 65). None of it is on the edit path except the schema |
| Official run | `cargo xtask perf --output /private/tmp/deadpan-perf-dp24 --stages edit --generate`: `deadpan-cli` `2775e1f2…`, `perf` `bfd73fb0…`, `deadpan-media-worker` `9b5c1b96…`. All three edit stages started below load 3.0 (`--max-load 3`); none was flagged |
| Before/after | `perf` built from a `git archive` of HEAD (`4c87a297…`) and from the working tree (`85c576c4…`) in separate target directories. Three alternating rounds, each on a fresh 10,000-beat package and each started at 1-minute load 2.66–3.00 |

## 10,000-beat edits (30 cycles each)

p50 / p95 ms of commit plus workspace refresh (`total_ms`), as in earlier
records.

| Run | Split | Pause | Wrap | Undo |
| --- | --- | --- | --- | --- |
| Before, round 1 | 50.3 / 55.6 | 144.6 / 177.9 | 49.8 / 53.4 | 26.5 / 28.6 |
| Before, round 2 | 50.4 / 53.3 | 141.2 / 172.7 | 50.2 / 53.8 | 26.4 / 27.7 |
| Before, round 3 | 47.8 / 50.1 | 136.1 / 168.1 | 47.8 / 50.5 | 25.2 / 27.0 |
| After, round 1 | 27.0 / 29.0 | 74.9 / 109.2 | 30.4 / 32.6 | 24.0 / 25.1 |
| After, round 2 | 31.1 / 34.7 | 81.1 / 125.3 | 33.3 / 39.0 | 26.2 / 29.8 |
| After, round 3 (load rose to 3.2) | 31.7 / 52.0 | 86.3 / 132.8 | 32.7 / 43.9 | 26.1 / 36.2 |
| After, official xtask run | 28.4 / 32.6 | 80.7 / 116.8 | 31.6 / 36.8 | 25.1 / 27.9 |

The store commit alone went from 39–41 to 19–23 ms p50 for split and wrap, and
from 127–135 to 67–77 ms for a pause. In the official run split, wrap and undo
PASS the 50 ms target. Pause FAILs the 100 ms Hold target at p95 116.8 ms
(p50 80.7 ms). Its first pause takes 221 ms because it binds all 10,000 Holds.
Later pauses rise from about 64 to 100 ms as the suffix of shifted siblings
and their retained clocks grow.

The generated projects stay well inside both targets: gen-1080p60 at
3.8–8.3 ms p95 for edits and 15.2 ms for a pause, gen-4k30 at 4.9–6.1 ms and
16.8 ms. Reopening the 123-revision 10,000-beat package writable takes 289 ms.

### Profile

A sampling profile (`sample`, 1 ms interval) ran on a release build with line
tables (`perf edit --kinds split,pause,wrap`, a new profiling mode that
measures only the named kinds). Before the change, a pause spent:

- about 20 ms serializing the complete binding state to check its byte limit;
- about 11 ms validating the captured copy from scratch inside Split;
- 11 ms validating the Split result;
- two structural walks for lineage and marks;
- a full validation of the result.

Split also built a complete anchor index and validated its result twice. Lineage
reconciliation looked every node up four times. The store copied the result to
check that the inverse restores the head.

What remains for a pause:

| Step | Share | Notes |
| --- | --- | --- |
| Capture of the complete-structure layout (node map, index and byte count) | about 14 ms | It is sliced at the end of the commit, but intermediate validation and resume terms read it |
| Split validation | about 7 ms | Binding owners reuse the head's proof |
| One resolve and resume term per later root sibling | about 5 ms | Required by authored semantics |
| Four binding validations | about 4 ms each | Capture, Split, resume terms and the result; each still builds a whole-document parent map |
| One structural walk | about 4 ms | — |
| SQLite commit with `F_FULLFSYNC` | about 10 ms | The patch carries the root Sequence's 10,000-entry children list before and after, in both directions |

## Equivalence evidence

- `crates/deadpan-core/tests/command_work.rs` runs 48 seeded random
  sequences of 40 edits over Hold-only, linked-Source and placed-sound
  documents. They cover pauses at seams and interiors, Split anywhere and of
  occurrence-mark hosts, Repeat wraps with and without gaps, ripple delete,
  Hold duration, Local, Occurrence and Sequence marks, play counts, Group,
  Ungroup, Retime wraps, root and beat sounds and Hold allowances.
  - Every command kind commits at least five times; refusals include at least
    three each of InvalidCommand, SelectionUnavailable, WrongNodeKind and
    SourceRangeInvalid.
  - More than 100 results each carry lineage, marks, retained clocks, Sources
    and sounds or allowances (the unoptimized capture branch), and at least
    five Splits relocate occurrence marks through the anchor index.
  - Each step must produce the same transaction, result, durations and error
    with and without `with_reference_command_work`, through `apply_validated`
    and `apply_with_result`.
  - Every result passes `check_against_complete_validation`.
  - `DocumentPatch::restores` must equal applying and comparing for matching,
    reversed, conflicting and unequal patch/document pairs.
  - A merged-diff test compares the diff with key-union lookup on 200 random
    maps.
  - The capture byte check equals `to_json` below and above the structural
    bound and for a state with sound clocks, and scoped binding reuse
    rechecks owners of a replaced timing table (unit test).
- Debug builds assert at every reuse that:
  - shared structural durations equal a fresh structural pass;
  - proved durations equal complete validation;
  - an adopted Split validation equals complete validation;
  - the binding byte bound is at least the exact length.
- `cargo test --locked -p deadpan-core -p deadpan-store`: 1,368 passed. This
  includes the store's random commit, rebuild and receipt suites, which assert
  on every commit that retained validation equals complete validation and
  that the adopted result equals the stored forward patch applied to the head.
- `cargo clippy --locked -p deadpan-core -p deadpan-store --all-targets -- -D warnings`
  is clean.

## Diagnostics

The counters, their recording points and their limits are in
[Performance measurement](../PERFORMANCE.md#diagnostics).

- `deadpan-cli doctor --project` reports the CLI process's own counters.
- The native `:diagnostics` panel samples them twice a second.
- The `diagnostics` replay passes 8 checks and the Kestrel audit:
  - a labelled dialog with 8 accessible rows;
  - GPU 7 submitted and 7 completed, p50 2.4 ms and p95 5.6 ms;
  - the frame row updates without input;
  - edit and transport keys are inert while the panel is open;
  - Escape returns focus to the pane;
  - sampling stops once the panel closes.

  The `workspace`, `accessibility` and `keymap` replays still pass.
- The panel was not observed in a native window, with a real model worker or
  during device playback.

## Idle and memory

| Package | Release app idle, 60 s | Idle wakeups | Footprint (peak) |
| --- | --- | --- | --- |
| gen-1080p60 | 0.40 s CPU (0.67 %) | about 0.7/s | 491 MB (805 MB) |
| large-10000 | 0.39 s CPU (0.65 %) | about 0.4/s | 289 MB (607 MB) |

- Method:
  1. Open the release `deadpan-app` (`1937fe08…`) with `--project` on a copy of
     the package.
  2. Wait 30 s for opening, index measurement and proxy preparation.
  3. Take the CPU-time difference from `ps` over 60 s, and the wakeups and
     footprint from `top` and `footprint`.
- The window was frontmost and unoccluded. This measures the process, not
  display power.
- Memory pressure was not measured. `memory_pressure -S` needs
  `kern.memorypressure_manual_trigger`, which is not permitted without root,
  and real allocation pressure on a 128 GiB machine was not attempted. The app
  stayed alive with unchanged footprint through the attempted triggers.

## Not covered

- The interview package and lower hardware tiers.
- Native-window refresh and physical display latency.
- Cold seek, playback and inference stages, which were not rerun.
- A full `cargo xtask gate`.
