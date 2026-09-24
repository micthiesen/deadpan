# Retained audio context qualification, 2026-09-23

This increment builds on `6a1ab4fa4ac6235a404760a4c98cb3b4d7aec1aa`, whose
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35947858805) passed.
It retains complete raw audio processing inputs alongside frozen timing and
reopens them against immutable project history. [The contract](../AUDIO_CONTEXT.md)
defines the standalone schema 1 and explicit media-admission boundary.
Core 15/database 21 remain unchanged.

## Verification

The full repository gate passed with **1,062 tests**, zero failed and zero
ignored. All 347 source and fixture hashes stayed unchanged through the run.
[Raw evidence](../../tools/audio-qualification/evidence/2026-09-23-context/summary.json)
retains command results, compressed logs, source hashes and the gate script.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --workspace --locked` | 1,062 passed |
| `cargo build --workspace --locked` | Pass |
| `cargo run -p deadpan-cli -- doctor` | Pass; development foundation |
| `cargo run -p deadpan-app --locked -- --smoke-test` | Pass; Metal and shutdown |

The 14 focused context tests passed within the workspace gate. The eight saved
ImageGen boards/prompts, five unchanged imported spec files and changed Markdown
links verified. Tests ran on macOS 26.5.2, Apple M5 Max, Rust 1.97.1 with the pinned
FFmpeg 8.0.3 compatible developer prefix. The native smoke initialized the Metal
GPU and completed the window shutdown callback; it is not an aesthetic or
keyboard acceptance test.

## Review

Independent general, media-admission and PCM/clock reviews found no issues.
The review covered the complete dirty and untracked increment against
`origin/main` at the base revision above. Reviewers made no source changes.
The main pass added a cache-admission regression for prepared RoomTone and
nested Preserve; it was included in the PCM review.

## Behavior exercised

Core tests round-trip exact positive/negative sample offsets, fractional Source
placement, RoomTone and Tail inputs, full referenced asset records and Source
absence. Picture-only and unused assets are excluded. A billion-play Repeat keeps
its compact order, sparse override and copy lineage. Later document changes do
not alter a captured context. A supported Source mapping plus mix offset whose
effective start exceeds Placement's i64 range remains representable. Unknown,
duplicate, oversized and inconsistent inputs, invalid asset contracts and
out-of-stream selections fail admission.

Plan tests compare complete normal and restored-context audio queries, source
offsets, edge policies, RoomTone and nested Preserve clocks. A final-play query
through a billion-play Repeat stays bounded and retains its sparse override.
Audio-only plans reject picture evaluation.

Audio tests decode qualified PCM and run canonical DSP. Restored contexts match
ordinary plans through structural Split, later deletion, RoomTone, nested
Preserve, signed placement, raw and faded reads, and irregular suffix-first
access. Picture-only Source absence remains distinct from explicit silent Hold
suppression. Providers with only ordinary revision lookup are rejected before
that lookup runs. Prepared RoomTone and Preserve caches recheck context admission
before returning PCM, including after the host contract changes or the provider
is replaced. Unsupported Tail processing still fails explicitly.

Host tests create qualified managed and linked project media. After undo and
asset-alias reuse, a serialized historical context still produces the original
decoded PCM, including after writer reopen; the current session produces the
replacement. Read-only access leaves the revision/history counts and cursor
unchanged. Validly encoded changes to the asset contract or mapping/offset are
rejected against retained history, even when effective placement is unchanged.
Wrong projects, wrong or absent revisions and modified linked bytes fail.

## Remaining work

The context supplies the old signal body. Live occurrence bindings, exact phase
composition, compact Repeat edit/birth lifecycle, policy replacement and genuine
seam envelopes remain required before atomic inserted-time editing and app
playback can use it. The host requires retained project history and independently
verified originals; a standalone JSON context is not a portable media package.
This increment does not complete a product requirement or delivery gate.

No GUI behavior changed. Visual, focus/IME and keyboard QA are not repeated for
this backend increment; the saved single-Original ImageGen boards and earlier
native reviews remain the design evidence.
