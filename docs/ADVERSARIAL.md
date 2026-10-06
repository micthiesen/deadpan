# Adversarial suite

Gate G requires a crash/chaos/malicious-input suite and long-project stress
([specification Section 30](spec/DEADPAN_SPEC.md#gate-g--release-qualification),
Section 26.1: "Fuzz malformed schemas and command inputs without launching media
workers"). This document describes the reproducible, offline suite. Results
are recorded per run in `docs/qualification/adversarial-*.md`.

## Engine

The workspace pins stable Rust 1.97.1, so cargo-fuzz/libFuzzer (nightly
sanitizer coverage) is unavailable. `crates/deadpan-chaos` is a small
development-only replacement with no new third-party dependency (it uses the
existing `serde_json`):

- Deterministic SplitMix64 seeds; stacked byte mutations (bit flips, boundary
  integers in both byte orders, truncation, insertion, deletion, block
  duplication and swaps, cross-seed splicing), JSON node mutations (type
  confusion, extreme numbers, hostile strings, deep nesting, huge arrays,
  removed/renamed/unknown keys, donor subtree splicing) and frame-stream
  mutations that rewrite one length-prefixed JSON frame and recompute its
  length so semantic validators are reached.
- Coverage proxy: an input joins the corpus when it produces a verdict class
  (typed error code and message, digits normalized) not seen before. This is
  coarser than edge coverage; it keeps mutation productive across rejection
  paths but cannot see code reached without a distinct verdict.
- Failure capture: panics are caught per case (a scoped panic hook keeps other
  tests' output), invariant violations reported by the target, per-case wall
  time (confirmed by one re-run) and per-thread peak allocation through a
  forwarding counting allocator. Failures are minimized by chunk deletion and
  saved with their class under `$DEADPAN_CHAOS_OUT/crashes/<target>/`.
- Child isolation: `ChildRunner` re-executes the current test binary for one
  case under `/usr/bin/time -l`. Signals, aborts, sanitizer reports, hangs
  (killed after a timeout) and peak resident memory above the bound are
  failures observed by the parent.

Every target asserts that each input ends in admission or a typed,
nonempty error, never a panic or abort, within its time and memory bounds.
Many also assert semantic invariants on accepted inputs: round trips, exact
inverse patches, validated results, identity binding and containment.

## Modes

| Mode | How | Use |
| --- | --- | --- |
| Regression | `cargo test` (default) | Seeded, deterministic, a fixed iteration count per target; about 30 s of debug wall time across the suite. Runs in the normal workspace tests and CI. |
| Campaign | `cargo xtask chaos [--minutes 10] [--seed HEX] [--only a,b]` | Builds the adversarial test binaries once, records their SHA-256 digests and runs each target for its share of the time from a fresh or given seed. Writes `report.jsonl`, `summary.json`, logs and crash artifacts below `target/chaos/<stamp>` (or `--output`). Fails if any target found a failure. |
| Sanitized | `cargo xtask chaos --sanitize` | Builds `deadpan-source` with the documented ASan/UBSan C flags and runtime link arguments for `aarch64-apple-darwin` in its own target directory and runs the container and out-of-process decoder targets there. Rust and the FFmpeg libraries are not instrumented. |
| Replay | `DEADPAN_CHAOS_REPLAY=<file> cargo test -p <crate> <test>` | Runs exactly one saved input. |
| Stress | `cargo xtask chaos --stress` | Full-scale long-project stress in release (below). |

Environment: `DEADPAN_CHAOS_SECONDS`, `DEADPAN_CHAOS_SEED` (hexadecimal; mixed
with the target name), `DEADPAN_CHAOS_ITERATIONS`, `DEADPAN_CHAOS_OUT`.
`cargo xtask chaos --list` prints the entries.

## Targets

| Target | Boundary | Seeds | Invariants beyond typed errors |
| --- | --- | --- | --- |
| `source-container-video`, `source-container-audio` | `deadpan-source` closed MP4/Matroska/WAVE grammar before FFmpeg (`input::validate`), plus `inspect_mp4` | All committed source fixtures under 128 KiB | An admitted MP4 must also inspect |
| `source-codec-config` | `avcC`/`hvcC` records mutated in place inside real MP4 hosts | Every fixture's record | |
| `source-hevc-sps` | `hevc_sps_geometry` | SPS units from every `hvcC` | Cropped size never exceeds coded size |
| `source-ffv1-config` | FFV1 v3 configuration parser | Matroska CodecPrivate payloads | |
| `source-native-decode` | Full FFmpeg video and audio decoding, one child process per case | Fixture payloads mutated in place (containers stay admissible) | Decoded RGBA size matches geometry; no signal, sanitizer report, hang or RSS above 2 GiB |
| `media-worker-conversion` | The real isolated FFV1 conversion helper (`canonicalize`) | `rgb*` helper fixtures | A `Protocol` error (crash, abort or malformed reply) is a failure; accepted output satisfies its canonical contract |
| `media-helper-protocol` | Helper argv request and stderr reply | Conversion and bridge requests, success and failure replies | Replies validate against the request they answer |
| `media-source-index`, `media-audio-index` | Stored measured index JSON | Unit-test indexes | Exact round trip |
| `core-document` | `ProjectDocument::from_json` | Store current-schema documents | Validates and round-trips exactly |
| `core-audio-context`, `core-audio-layout` | Frozen audio contexts and layouts | Captured from those documents | Round trip |
| `core-edit-slice`, `core-register-value` | Edited slices and register values | Whole-child captures and the frozen range fixture | Round trip |
| `core-command` | `CommandRequest` JSON applied by the core | Rename/Delete/DeleteRipple/WrapRepeat/Split/SetHoldDuration/Insert for every node | Forward patch applies, result validates and the inverse restores the exact document |
| `core-patch` | `DocumentPatch` JSON | Real transactions | Any applied result validates |
| (deterministic) | Deep chains (100,000 levels), cycles, shared and self parents, dangling children, 200,000 siblings | Generated | Refused or valid, within 20 s, no stack exhaustion |
| `store-database-bytes` | Arbitrary byte damage to a package's SQLite file | A package with inserts, a Repeat, a deletion, undo/redo and a register | Open plus full validation fails cleanly or every revision and register is unchanged |
| `store-row-tamper` | One cell rewritten, nulled, retyped, byte-flipped or deleted in any nonempty table | Scripted cell edits | As above, including the register slot table and bank version |
| `jobs-generation-protocol`, `jobs-tracking-protocol`, `jobs-faces-protocol`, `jobs-transcription-protocol` | Framed worker protocols, both directions | Valid requests, cancellations and responses | Responses classified against the attempt they claim |
| `cli-render-protocol`, `cli-encoded-protocol`, `cli-verification-protocol`, `cli-admission-probe-protocol` | Render, encoded render, verification and encoder-probe worker protocols | As above | As above |
| `cli-live-endpoint` | Bytes sent to a real bound live-project socket endpoint | Authenticated requests with representative operations | Dispatch only with the owner secret; responses never contain it; dispatched payloads pass the semantic decoder |
| `cli-ytdlp-metadata` | `--dump-single-json` output and format selection | Representative metadata | Accepted metadata names the requested video |
| `models-pack-archive` | Offline pack tar import | Exported and system-tar archives plus traversal, absolute, link, pax and GNU long-name members | Imports exactly the manifest bytes; nothing written outside the store; no link staged |
| `models-pack-manifest` | Pack manifest JSON | Approved manifests | |
| `analysis-transcript`, `analysis-tracker-artifact` | Recognizer segments, stored transcripts and tracker artifacts | Unit-test outputs | Transcripts revalidate and round-trip |

## Long-project stress

`crates/deadpan-store/tests/long_project_stress.rs` creates a generic project
with one synthetic two-hour Original (an unqualified legacy asset record; no
media bytes) cut into Source beats with a Hold every fiftieth beat, edits it
through the real store (renames, Hold durations, Repeat wraps, Splits, ripple
deletions, undo/redo), then reopens, validates by receipt and by full replay,
reads historical revisions, compiles the render plan and times picture and
ten-second audio range queries. Each stage has a wall-time budget; the test
thread has an allocation budget. The normal suite runs 2,000 beats and 80
edits; `cargo xtask chaos --stress` runs 10,000 beats and 300 edits in release.
Budgets bound pathological growth; the Section 25 interactive targets remain
measured by `cargo xtask perf`.

## Process kills

`crates/deadpan-store/tests/chaos_kills.rs` is the crash part of the suite. A
child process (the test binary re-executed) keeps a writer busy with edit
commits, AI generation attempt state changes, verified backups with rotation
and checkpoints; another child restores backups back and forth; a third runs a
synthetic release migration; the proxy cache test kills a child publishing
proxies. The parent SIGKILLs each after a seeded random
delay, repeatedly on the same package, then reopens and checks that history
validates, every reported commit survives, no attempt is left running, every
published backup verifies and every checkpoint is intact, and that a killed
restore or migration leaves exactly one whole state. The normal suite runs 8
kills per test from a fixed seed; `DEADPAN_CHAOS_SEED` (hex) and
`DEADPAN_CHAOS_ITERATIONS` widen it. Details and the hot-journal defect it
found: [backups](BACKUPS.md#process-kills). A process kill is not a power
loss.

## Limits

- No edge coverage: the verdict-class proxy cannot guide mutation through code
  that produces no new verdict, and regression iteration counts are modest.
- Real codecs run only in the source decoder and conversion-helper targets, on
  small fixtures; the encoders, the render/tracking/transcription workers and
  model inference are exercised only at their protocol boundaries.
- Sanitizers instrument the C adapters only, not Rust or FFmpeg.
- In-process allocation bounds charge the calling thread; work moved to other
  threads is bounded by the child-process RSS check only where a target uses
  one.
- The history chain and register bank digest are unkeyed: they detect
  corruption and partial tampering, not a deliberate consistent rewrite by
  someone with write access.
- The long-project stress uses a synthetic Original; decoding, proxying and
  encoding a real two-hour Original are not exercised.
