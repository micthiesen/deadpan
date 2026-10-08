# Adversarial suite

Gate G requires a crash/chaos/malicious-input suite and long-project stress
([specification Section 30](spec/DEADPAN_SPEC.md#gate-g--release-qualification),
Section 26.1: "Fuzz malformed schemas and command inputs without launching media
workers"). This document describes the reproducible, offline suite. Results
are recorded per run in `docs/qualification/adversarial-*.md`.

## Engines

The product workspace pins stable Rust 1.97.1. `crates/deadpan-chaos` runs
deterministic mutation regressions on that toolchain, using the existing
`serde_json` dependency:

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

The separate [`fuzz/`](../fuzz) workspace uses `libfuzzer-sys` 0.4.13 and a
nightly compiler for actual sanitizer coverage. `cargo xtask fuzz` builds and
runs its 27 parser targets without starting media workers, opening windows or
reading the user's projects. Native admission targets write only private
temporary input files. Each process has a 256 KiB input limit, a 10-second
per-input timeout and a 2 GiB RSS limit. Campaign duration is per target;
`--jobs` bounds parallel processes. These are fuzz-run limits, independent of
the production readers' own limits.

### Coverage-guided parser map

| Untrusted input | libFuzzer target | Production boundary |
| --- | --- | --- |
| Project JSON and history rows | `core-document`, `core-patch`, `core-command` | `ProjectDocument::from_json`, `DocumentPatch` including compact `command::patch_wire`, and `apply` with forward/inverse validation |
| Frozen audio contexts and timing layouts | `core-audio-context`, `core-audio-layout` | Their production `from_json` and `to_json` methods |
| Register rows, edited slices and Macro programs | `core-register-value`, `core-edit-slice` | `RegisterValue` deserialization, `SemanticProgram::validate`, captured-slice parser |
| Recipe files and saved recipe labels | `core-gag-recipe` | `GagRecipe` deserialization, label parser and expansion at three project rates |
| Personal keymap configuration | `app-keymap-config` | `Bindings::from_json_reporting`, real trie compilation and routing; the feature-gated app library uses the production modules |
| Signed update envelopes and payloads | `cli-update-manifests` | `SignedManifest::parse`/`verify`, `DownloaderManifest::parse`, `PackUpdate::parse` |
| Model-pack and conditioning manifests | `models-pack-manifest`, `models-conditioning-manifest` | `PackManifest::validate`, `BridgeContext` and `ConditioningColour::from_manifest` |
| Framed job and host worker messages | `jobs-{generation,tracking,faces,transcription}-protocol`, `cli-{render,encoded,verification,admission-probe}-protocol` | Production readers and writers in both directions, plus request-bound response classification |
| Transcript JSON and recognizer output | `analysis-transcript` | `Transcript` deserialization and `Transcript::from_segments` |
| Retained measured media indexes | `media-source-index`, `media-audio-index` | Production snapshot JSON readers and exact round trips |
| MP4, Matroska and WAVE before FFmpeg | `source-container-video`, `source-container-audio` | The closed container grammar, codec configuration admission and MP4 inspection |
| HEVC SPS and FFV1 configuration records | `source-hevc-sps`, `source-ffv1-config` | The production bounded geometry/configuration parsers |

Every target starts from committed input under `fuzz/corpus/<target>`. The
stable chaos target of the same name replays those inputs in ordinary tests.
Macro seeds also feed the register target. Framed streams use selector 0 for
requests, 1 for responses and 2 for a request followed by responses classified
against it. Stable tests and nightly targets share `protocol_stream`, including
the request-specific classifier and request round-trip checks. Stable seeds
assert that the classifier is actually reached before mutation begins.

Build/replay and an example ten-minute-per-target campaign:

```sh
cargo xtask fuzz list
cargo xtask fuzz seeds
cargo xtask fuzz replay
cargo xtask fuzz run --minutes 10 --jobs 4 --output /tmp/deadpan-fuzz-RUN
```

Install nightly and `cargo-fuzz` as development tools first. The separate
`fuzz/Cargo.lock` is committed. The product uses Cargo's `--locked` flag.
Because cargo-fuzz 0.13 does not forward that flag, its runner first resolves
the full graph with `cargo metadata --locked --offline`, builds without network
access, and rejects any change to the lock's SHA-256. Reports retain
nightly/cargo-fuzz versions, source revision and
tracked-diff hash, a hash manifest covering tracked and untracked source files,
each executed binary's SHA-256, coverage/features, execution
counts, exit status, logs and artifact paths. A nonzero or signalled exit fails
even when no artifact was written. The summary is saved before merging; a
failed run never merges. Successful merges minimize discoveries and add them
without deleting committed seeds or fixed reproducers. Nothing in the runner
asserts that a campaign has run or that edge coverage proves completeness.

Reproduce and minimize a failure from the separate workspace:

```sh
cd fuzz
cargo +nightly fuzz run TARGET /absolute/path/to/artifact -- -runs=1
cargo +nightly fuzz tmin TARGET /absolute/path/to/artifact
```

Keep the original artifact and log in the run directory. After fixing the
bug, add the minimized input to `fuzz/corpus/TARGET` and a focused regression
that fails against the old code. Never discard a timeout or OOM artifact just
because a later input does not reproduce it. Run `fuzz replay` on the fixed
source, then record the new campaign's source and binary identities. The
[release audit](RELEASE_AUDIT.md) records which runs have actually been verified.

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
| `jobs-extension-plan` | Exact extension plan and generated-only sampling map | Both directions of the K9/E8/N12 real-media conversion case | Strict round trip; bounded first/middle/last sampling stays inside generated frames; terminal output ordinal rejects |
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

## Hostile workers

These tests launch real child processes through each host's production
supervision path: the trusted runtime seam (`RenderWorkerRuntime`,
`TrackingRuntime`, `TranscriptionRuntime`, `BridgeRuntime`), the shared
`SupervisedProcess`, `deadpan_native_process::spawn` and
`terminate_owned_group`, and the contained artifact reader. Only the worker
is a stand-in: a Python fixture that reads the host's real first request and
then misbehaves. No production backdoor exists; the tests use the same
executable-selection fields a host fills from its packaged workers.

`crates/deadpan-cli/tests/hostile_workers/hostile.py` holds the shared
behaviours. A mode `hostile:<name>:<dir>` names one, and `<dir>` is a
test-owned record directory where the fixture writes its leader, group,
launch and descendant PIDs. `support.rs` reads them back. After every host
returns, it asserts that the leader, each recorded child and the whole group
are gone. It allows three seconds for launchd to reap orphans and never
waits on a live process.

| Behaviour | What the stand-in does | Bound |
| --- | --- | --- |
| `oversized`, `just_over` | Declares a 4 GiB or a 256 KiB + 1 frame, then holds its pipes | Refused from the header alone, under 10 s with a 60 s deadline |
| `slow_loris` | Declares a legal 4 KiB frame and sends one byte every 50 ms | Stopped by the 1.5 s deadline, or by cancellation, under 12 s |
| `fork_spam`, `fork_spam_exit` | Forks 24 sleeping children in its group, then hangs or exits without a terminal reply | Every child gone when the host returns |
| `fork_spam_valid` | The same, after a valid completion | Admitted only after all 24 children are stopped |
| `escape` | A child calls `setsid` and keeps the inherited stdout/stderr | Fails as "pipes stayed open". The escapee is alive afterwards; the test kills it |
| `escape_quiet` | A `setsid` child closes its pipes, waits for the host to return, then rewrites the worker's output | The host admits the clean leader. The escapee outlives it, and the admitted private snapshot is unchanged |
| `stderr_flood` | Writes 16 MiB to stderr, then exits 3 | Typed error under 4 KiB; the AI host keeps exactly the last 64 KiB and counts the rest |
| `symlink_outside`, `hardlink_outside` | Claims a valid manifest whose artifact is a symlink or hard link to a matching outside file | `Artifact` error; the outside file is unchanged |
| `symlinked_scope` | Replaces the output directory with a symlink to an outside directory holding a matching artifact | `Artifact` error |
| `fifo`, `directory` | Places a FIFO or a directory at the artifact path | `Artifact` error without blocking |
| `sparse` | A 64 GiB sparse file whose declared hash and length are the real 12 to 35 bytes | Refused from metadata under 10 s, never read |
| `absolute`, `parent` | Declares an absolute or `..` artifact reference | Protocol refusal, not a fixture crash |
| `wrong_attempt` | Answers for another attempt | Protocol refusal |
| `tamper_input` (verifier) | Rewrites and truncates its staged `input/movie.mp4` | The host's private candidate is unchanged |

Coverage by host:

| Host | Test | Behaviours |
| --- | --- | --- |
| Raw picture worker (`render_worker::prepare`) | `tests/render_worker.rs`: `hostile_artifact_claims_are_refused_without_following_them_outside`, `hostile_frames_floods_stalls_and_descendants_are_bounded_and_stopped`, `setsid_escapees_are_not_contained_but_cannot_change_admitted_bytes` | All artifact claims, frames, stalls, descendants, both escapes |
| Encoded worker (`encoded_render::encode`) | `tests/encoded_render.rs`: `hostile_movie_claims_outside_the_scope_or_not_regular_are_refused`, `hostile_frames_floods_stalls_and_descendants_are_bounded_and_stopped` | All artifact claims, frames, stalls, descendants, `escape` |
| Finished-file verifier (`verification::verify`) | `tests/encoded_verification.rs`: `hostile_verifier_processes_are_bounded_stopped_and_cannot_touch_the_candidate` | Frames, stalls, descendants, `escape`, `tamper_input`; the candidate survives every failure |
| Encoder admission probe (`admission::qualify`) | `tests/encoder_admission.rs`: `hostile_probe_processes_are_bounded_stopped_and_never_retried` | Frames, stalls, descendants, `escape`; cleanup confirmed, nothing rejected or retried |
| Tracking (`tracking::track`) | `tests/hostile_analysis_workers.rs`: `tracking_admits_only_contained_observations_and_stops_every_group` | Honest control, all artifact claims, frames, stalls, descendants, `escape` |
| Face detection (`faces::detect_faces`) | same file: `face_detection_refuses_hostile_frames_stalls_and_descendants` | Frames, stalls, descendants, `escape`, `wrong_attempt` (no artifact) |
| Transcription (`transcription::transcribe`) | same file: `transcription_admits_only_a_contained_regular_transcript`, `transcription_bounds_frames_stderr_stalls_and_descendants`, `cancelling_a_worker_that_ignores_it_mid_frame_reports_cancellation`, `transcription_escapee_keeping_pipes_fails_and_outlives_group_cleanup` | Honest control, all artifact claims, frames, stalls, cancellation, descendants, `escape` |
| AI pause worker (`generation::attempt::run_worker`) | `src/generation/attempt/tests.rs`: `hostile_ai_workers_fail_truthfully_and_leave_no_group_member`, `stalled_and_forking_ai_workers_are_stopped_by_cancellation`, `an_escaped_ai_worker_descendant_fails_the_attempt_and_is_not_contained` | Frames, `wrong_attempt`, floods, descendants and `escape` end durably `Failed`. A stall or fork ignoring cancellation ends `Cancelled` within the 5 s grace |

The generic supervisor and artifact reader keep their own lower-level process
tests in `crates/deadpan-jobs/tests/supervisor.rs` and `artifact.rs`.

These runs found one defect. When the supervisor killed a worker that ignored
cancellation while a frame was in flight, the truncated frame was reported as
a worker fault ("frame ended after N of M payload bytes"). The same ignored
cancellation then ended `Failed` or `Cancelled` depending on where the kill
landed. Read errors after the supervisor's own escalation are now ignored,
since cancellation already withholds any candidate. The regression test is
`escalated_cancellation_does_not_report_the_frame_its_kill_truncated`.

These tests do not cover:

- Real encoders, Vision, whisper.cpp or MLX under hostile media. The workers
  here are stand-ins, and each real worker's own input hardening is covered
  separately.
- Resource exhaustion inside a worker: memory, disk or file-descriptor
  pressure.
- Escapes by other means than `setsid`, such as double forks into another
  session, `launchd` jobs, or writes to paths outside the workspace. The hosts
  never read such writes, but nothing prevents them. This is process
  ownership, not an OS sandbox.
- The AI host's 30-minute deadline. Its stalls are stopped by cancellation in
  these tests.
- Linux runs of the shared tests; they were run only on macOS.

## Limits

- Stable chaos uses verdict-class novelty; the separate libFuzzer targets use
  sanitizer coverage. Neither proves complete path coverage, and a short smoke
  campaign does not replace sustained campaigns.
- Real codecs run only in the source decoder and conversion-helper targets, on
  small fixtures; the encoders, the render/tracking/transcription workers and
  model inference are exercised only at their protocol boundaries.
- `chaos --sanitize` instruments the C adapters only. The nightly libFuzzer
  build instruments Rust with sanitizer coverage/ASan; it does not rebuild the
  linked FFmpeg distribution with sanitizers or exercise full codecs.
- In-process allocation bounds charge the calling thread; work moved to other
  threads is bounded by the child-process RSS check only where a target uses
  one.
- The history chain and register bank digest are unkeyed: they detect
  corruption and partial tampering, not a deliberate consistent rewrite by
  someone with write access.
- The long-project stress uses a synthetic Original; decoding, proxying and
  encoding a real two-hour Original are not exercised.
