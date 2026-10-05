# Adversarial suite qualification, 2026-10-05

First Gate G crash, chaos and malicious-input run and long-project stress,
using the suite described in [Adversarial suite](../ADVERSARIAL.md).
Compact evidence (per-target seeds, executions, bounds, binary digests and the
stress report) is in [adversarial-2026-10-05.json](adversarial-2026-10-05.json).

Host: Apple M5 Max, macOS 26.5.2, Rust 1.97.1, the pinned LGPL FFmpeg 8.0.3
developer prefix. Source: commit `61e834a7` plus the uncommitted suite changes
(and another agent's concurrent accessibility work in `deadpan-app`, which no
adversarial target links). The machine was concurrently running that agent's
UI replays, so throughput figures are lower bounds.

## What ran

| Run | Command | Wall time | Executions | Result |
| --- | --- | --- | --- | --- |
| Regression | `cargo test ... adversarial` (all adversarial tests, debug) | 28 s including build check; 25 s of tests | about 35,000 | Pass |
| Campaign | `cargo xtask chaos --minutes 10` (debug) | 605 s | 52,705,716 over 33 targets | One target failure (below), fixed and pinned |
| Store recheck | `cargo xtask chaos --only store-tamper --minutes 6` after the fix | 360 s | 92,674 | Pass |
| Sanitized | `cargo xtask chaos --sanitize --minutes 3` | 180 s | 37,970,238 over 6 `deadpan-source` targets, 1,593 isolated decodes | Pass; both binaries link `libclang_rt.asan_osx_dynamic.dylib` |
| Stress | `DEADPAN_STRESS=full cargo test --release -p deadpan-store --test long_project_stress` (what `cargo xtask chaos --stress` runs) | 29.6 s | 10,000 beats, 300 edits | Pass |

Campaign highlights (all zero failures except `store-row-tamper`):

| Target | Executions | Distinct verdict classes | Max case | Max thread allocation |
| --- | ---: | ---: | ---: | ---: |
| `source-container-video` / `-audio` | 218,777 / 262,444 | 151 / 116 | 35 ms | 2.8 KB |
| `source-codec-config` (avcC/hvcC in place) | 253,238 | 32 | 35 ms | 118 KB |
| `source-hevc-sps` / `source-ffv1-config` | 16.1 M / 12.7 M | 8 / 4 | 0.2 ms | 140 B |
| `source-native-decode` (child per case) | 6,158 | 12 | 27 ms | child RSS bound 2 GiB |
| `media-worker-conversion` (real helper) | 7,958 | 4 | 68 ms | separate process |
| `core-command` (forward/inverse exactness) | 460,677 (922 accepted edits) | 35,706 | 32 ms | 17 MB |
| `core-patch` | 401,057 (535 applied) | 25,858 | 15 ms | 8.9 MB |
| `core-document` / `-audio-context` / `-edit-slice` | 201,880 / 494,345 / 291,801 | 15,050 / 39,511 / 17,889 | 80 ms | 1.6 MB |
| `store-database-bytes` / `store-row-tamper` | 13,650 / 4,246 | 159 / 138 | 120 ms | 450 KB |
| Worker protocols (8 targets) | 6.7 M in total | 7 to 23 each | 34 ms | 14 MB |
| `cli-live-endpoint` (real socket) | 333,230 (2,110 dispatched) | 2,221 | 7.4 ms | 67 MB |
| `models-pack-archive` | 14,685 (227 imports) | 15 | 145 ms | |
| JSON evidence (indexes, transcripts, tracks, manifests, yt-dlp) | 13.2 M in total | | 142 ms | 8.6 MB |

The verdict-class counts for JSON targets are inflated by identifiers in error
messages; they show breadth of rejection paths, not coverage.

Earlier development campaigns on the same targets added 12.6 M core
executions (150 s per target) and 43,209 store cases with no failure other
than the register findings below.

## Findings

No panic, abort, signal, sanitizer report, hang, time-bound or allocation-bound
violation occurred in any target. Every rejection was a typed, nonempty error.
In particular, accepted command and patch mutations always produced valid
documents whose inverse restored the exact original; the live endpoint never
dispatched without the owner secret or echoed it; pack import never wrote
outside its store or staged a link; and byte- or row-level package damage never
validated as a different head or historical revision.

The only invariant failures were in register-bank tampering:

1. `register_state.version` changed (`03 01 03 7b 7d`) and the package still
   validated with a different bank version.
2. A `registers` slot row deleted (`a4 87 5d fe`) or renamed from `a` to `x`
   (`2c 5a df ff`, the 10-minute campaign's failure) validated as a bank
   without, or with a moved, named slot.

Cause: register contents were digest-addressed and validated (no case changed
a stored value undetected), but the slot table and bank version sat outside
the history hash chain, so a corrupted package could silently lose, rename or
retarget a named register.

Fix: database schema 65 adds `register_state.bank_digest`, a SHA-256 over the
bank version and every `name:content-id` slot, written with every bank update
and recomputed on every bank read, so opening, both validation modes and
checkpoints refuse a mismatch (see
[the register bank digest](../TIMING_STORAGE.md#register-bank-digest-database-schema-65)).
Schema 64 packages are refused without writes under the development-format
authorization. The three minimized inputs are pinned in
`adversarial_register_slot_tampers_are_detected`, which requires a
`Registers` error for each, and the fuzz invariant is back to "every revision,
the head and the complete register bank are unchanged".

Recheck: `cargo xtask chaos --only store-tamper --minutes 6` afterwards ran
73,494 `store-database-bytes` and 19,180 `store-row-tamper` cases with no
failure; 147 row tampers were refused specifically by the new digest check
(evidence: `target/chaos/store-tamper-2026-10-05`, seeds `0xf8194831861db3a6`
and `0xb8c46d03daf019cf`). The store (408), CLI (498) and app (878, base
features) test suites pass on the fixed source.

## Long-project stress

Full scale, release: a synthetic two-hour Original (169,734 project frames at
24 fps) as 10,000 beats (Source fragments with a Hold every fiftieth),
10,089 nodes after 300 edits, 229 authored revisions plus undo/redo.

| Stage | Result | Budget |
| --- | ---: | ---: |
| Create (asset, insert, ungroup) | 0.31 s | 60 s |
| Commit p50 / p95 / max | 37 / 110 / 301 ms | p95 2 s |
| Reopen (writer) | 0.39 s | 30 s |
| Validate by receipt | 0.38 s | 30 s |
| Full replay validation | 14.0 s | 600 s |
| Four historical snapshots | 0.36 s | 60 s |
| Render plan compile | 18 ms | 30 s |
| 2,000 random picture lookups | 0.9 ms | 10 s |
| 40 ten-second audio range queries (607 spans) | 0.8 ms | 30 s |
| Peak test-thread allocation | 214 MB | 8 GiB |
| Package database | 146 MB | |

No edit was refused. The commit p95 (110 ms) exceeds the interactive edit
target that `cargo xtask perf` measures (see DP-24); this stress bounds growth
and does not qualify that target. The debug default (2,000 beats, 80 edits)
runs in the normal suite in about 13 s.

## Coverage notes and limits

- The suite has no edge coverage; mutation is guided by verdict-class novelty
  only. Low class counts for the source parsers (4 to 32) reflect early
  closed-grammar rejection of most mutations; the in-place codec-record and
  payload targets exist to get past it.
- Real codec decoding is exercised on small fixtures (up to 128 KiB) in the
  source decoder and the conversion helper. Encoders, the render, tracking,
  transcription and model workers are exercised only at their protocol
  boundaries; model inference is not exercised.
- ASan/UBSan instrument the `deadpan-source` C adapters only, not Rust, FFmpeg
  or the other native crates in this run.
- In-process allocation bounds charge the calling thread only. Store work
  runs on that thread; the live endpoint's socket work is in-process too.
- The stress Original is synthetic: no media is decoded, proxied or encoded,
  and render-plan range queries stand in for export preview. Preview/export
  comparison on real long media remains a separate Gate G item.
- Campaign throughput was measured on a shared, busy machine.
