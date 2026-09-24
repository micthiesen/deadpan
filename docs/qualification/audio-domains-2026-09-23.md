# Physical audio-domain qualification, 2026-09-23

This increment builds on `edc55fc2f31424953234c667a175d78c7e48d3f4`, whose
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35943945160) passed.
The reference plan now distinguishes raw processing domains from flattened
policy intervals and supplies borrowed unit-rate root resume maps.
[The contract](../AUDIO_REFERENCE.md#physical-processing-domains-and-root-maps)
records the API and its limits. Core 14/database 20 remain unchanged.

## Verification and review

The full repository gate passed with **1,026 tests**, zero failed and zero
ignored. All 335 source and fixture hashes stayed unchanged through the run.
[Raw evidence](../../tools/audio-qualification/evidence/2026-09-23-domains/summary.json)
retains command results, compressed logs, source hashes and the gate script.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --workspace --locked` | 1,026 passed |
| `cargo build --workspace --locked` | Pass |
| `cargo run -p deadpan-cli -- doctor` | Pass; development foundation |
| `cargo run -p deadpan-app --locked -- --smoke-test` | Pass; Metal and shutdown |

Independent general and clock reviews found no issues. The focused plan suite
passed 11 tests and the signal-transfer suite passed seven. The eight saved
ImageGen boards/prompts, five unchanged imported spec files and changed Markdown
links also verified. Tests ran on macOS 26.5.2, Apple M5 Max, Rust 1.97.1 with the
pinned FFmpeg 8.0.3 compatible developer prefix. These results do not complete
any product requirement or delivery gate.

## Behavior exercised

The plan tests retain an opaque Preserve domain across its inner Source, silent
Hold and RoomTone intervals, while the ordinary query still reports their
policies. A three-unit work allowance visits just the root and opaque stage.
Partition crops retain full meaningful context; ordinary Edit crops constrain
it. Source placement gaps remain distinct from explicit silence. Billion-play
Repeat queries resolve stable play and gap identities without expansion, and
sparse overrides retain the correct effective occurrence.

At NTSC, inserting 1f inside a 2f A followed by a 2f B maps new sample 3203 to
old 1602 for A, and B's new start 4805 to old 3203. Continuing A's map into B
would instead choose old 3204. A second insertion inside a longer A composes
the current map to old 3204, rather than recomputing old 3203 from picture time.
Tests also cover negative and wider-than-i64 exhausted positions, fractional
root evaluation, query limits and foreign plan/preparation-clock identity.

An audio integration test uses the new domain map to resume actual canonical
Preserve output twice, transfer the correctly masked old-root signal onto an
explicit fractional point grid, and run a second canonical Preserve. It compares
PCM and masks against an independent explicit anchor and a fully materialized
masked carrier, with suffix-first irregular reads. A deliberately wrong anchor
produces different final PCM. The synthetic signal does not qualify recorded
speech, listening quality or application playback.

## Remaining work

These handles identify physical frozen contexts, not logical lineage across
Split copies. The tests deliberately keep copied aliases distinct even when
their timing agrees. Authored Hold insertion still needs persisted shared audio
lineage, live-to-frozen bindings, compact lifecycle transforms, policy replacement,
seam envelopes and strict history migration. Caller-supplied current starts are
not automatically resolved from a live document. No media access is authorized
by a timing handle, and no playback/export or whole-product acceptance is claimed.

No GUI behavior changed. This increment does not repeat visual, focus/IME or
keyboard QA; the saved single-Original ImageGen boards and earlier native
reviews remain the design evidence.
