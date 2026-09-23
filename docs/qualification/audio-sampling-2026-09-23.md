# Audio sampling contract qualification, 2026-09-23

This increment separates structural allocation, sampled PCM coordinates and
retained envelope progress in `deadpan-plan` and the source/stage readers. It
builds on `904bd1bc435228f59c0d98476bb5fdc3cba05ae0`. Core 14/database 20 remain
unchanged. [The contract](../AUDIO_SAMPLING.md) describes the implemented values
and the remaining authored Hold work.

The exact repository gate passed on macOS arm64 with Rust 1.97.1 and the pinned
compatible FFmpeg prefix. The native smoke test initialized Apple M5 Max Metal
and completed window shutdown. All **985 tests passed**, with zero failed or
ignored tests. Tracked and untracked source/fixture hashes were unchanged across
the entire gate. [Raw evidence](../../tools/audio-qualification/evidence/2026-09-23-sampling-clocks/summary.json)
includes command results, logs and source hashes.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --workspace --locked` | 985 passed |
| `cargo build --workspace --locked` | Pass |
| `cargo run -p deadpan-cli -- doctor` | Pass; development foundation, core 14/database 20 |
| `cargo run -p deadpan-app --locked -- --smoke-test` | Pass; Metal window and shutdown |

New tests cover root round-even versus prepared point-ceil grids, signed grid
origins, query-independent sampling maps, single and repeated NTSC resume,
44.1 kHz source steps, complete filter support, suffix-first/irregular reads,
and actual canonical Preserve output sampled without changing its full input
or descriptor. The PCM fixture is generated stereo signal, not recorded speech
or a listening qualification. The full suite also reruns existing decoded-media
parity, mixed Preserve/FollowSpeed, RoomTone, tiny-envelope and Split tests.

Envelope tests cover raw and faded endpoint silence, negative progress, Hard
policies, tiny widths, exact sample exhaustion, wide signed anchors and invalid
input atomicity. A one-frame plan test retains a 12e18-sample envelope without
allocating it, proving that length is independent of current output demand.

General review and a focused arithmetic/endpoint review found one compatibility
defect in the first implementation: rejecting envelope lengths greater than
i64::MAX rejected a previously valid retained Partition whose two endpoints each
fit i64. Length now uses the full positive u64 range, progress uses checked i128
with exact decimal inspection, and the concrete plan regression passes. Both
reviews reported no remaining material finding. A synthetic affine calculation
that exceeded an intermediate rational limit had no valid old-plan witness;
its old full-node extent already overflowed, so it did not justify a broader
arithmetic rewrite.

One focused Clippy invocation overlapped the widening change and saw five
temporary test argument type mismatches. Those were fixed before this recorded
gate. The final gate ran against stable sources and passed. Document checks
verified all eight ImageGen images/prompts, five unchanged archived specification
files and changed Markdown links.

No live aesthetic or keyboard review was repeated because this increment changes
no GUI behavior. The native smoke check is not an aesthetic or interaction claim.
Authored arbitrary-boundary Hold insertion, persisted reference clocks, stable
Repeat phase retention, retained nested silence masks, new seam fades and complete
application editing/playback/export remain open. These tests qualify the derived
sampling contract only. No DP requirement or delivery gate was marked complete.
