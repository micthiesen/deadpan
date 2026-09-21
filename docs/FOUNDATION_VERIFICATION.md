# Editing foundation verification

Verified on 20 September 2026 on Apple M5 Max, 128 GiB unified memory,
arm64 macOS 26.5.2 (25F84), Rust/Cargo 1.97.1, and bundled SQLite 3.53.2.
This report accompanies the first structural editing and SQLite implementation,
following setup commit `dc48e3b1f25f69dd8d1fa50b8c8d79ad76a54215`.
The repository commit containing this report identifies the tested source and
locked dependencies. This is partial evidence for Gates A and B, not a release.

## Implemented and checked

| Boundary | Evidence |
| --- | --- |
| Pure editing | [`deadpan-core`](../crates/deadpan-core/) validates flat Source/Sequence/Hold/Repeat/Retime documents, identities, tree ownership, exact durations, asset/source bounds, and strict JSON. Structural commands produce forward/inverse patches without mutating the input. |
| Serialization | Deterministic dumps and transaction round trips; duplicate IDs, unsupported schemas, cycles/orphans, excessive depth, invalid timing, oversized output, and malformed JSON rejection. Generic document deserialization cannot bypass bounded `from_json`. |
| Persistence | [`deadpan-store`](../crates/deadpan-store/) stores immutable revisions, command history, cursor, and redo state atomically. Undo/redo survive reopen and use new revision IDs; edits after undo retain abandoned history. Semantic replay verifies every retained revision and the current history state. |
| Failure handling | Actual SQLite page exhaustion returns `DiskFull` and retains the last committed state. A failed history insert rolls back the preceding revision insert. A child process exits inside an uncommitted transaction; reopening recovers the last committed document. Live backups include committed WAL pages. |
| Writer ownership | Independent process writer rejection; readers and dry runs coexist. A regression test covers a duplicated lock descriptor so an inherited handle cannot extend ownership after the store is dropped. |
| Host API | [`deadpan-cli`](../crates/deadpan-cli/) creates/validates/dumps projects, previews/commits commands, navigates history, and checkpoints. Both executables share the API. Process tests verify stale/reused revisions, protocol rejection, actionable error codes, undo/redo dry runs, and unchanged state after failures. |

## Validation results

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed. |
| `cargo test --workspace --locked` | **50 passed, none ignored**: 25 core, 16 store, 7 CLI, 2 native headless integration tests. |
| `cargo build --workspace --locked` | All four crates built. |
| `cargo run --locked -p deadpan-cli -- doctor` | Valid structured JSON; actual SQLite version and exact timing probe, with unavailable capabilities identified. |
| `cargo run --locked -p deadpan-app -- --smoke-test` | Actual Apple M5 Max Metal device initialized; native frames rendered, window closed, shutdown callback completed. |

The native smoke check ran after the shared headless entrypoint was added. The
subsequent changes forwarded specific domain error codes through the headless
protocol and bounded database identity metadata. Their regression tests and the
full noninteractive gate passed. No GUI behavior changed in those final fixes.

Independent core, persistence, and media-harness reviews were performed. Review
fixes include bounded document ingress/output, specific domain errors through
the CLI, dry-run/commit parity, semantic history validation, stored JSON bounds
before text extraction, bounded database identities/kinds, and undo/redo previews. The media review additionally
required authored frame identity and stronger upstream build reproducibility;
its final evidence is maintained in the separate report below.

## Media qualification and remaining work

[Native media qualification](qualification/media-2026-09-20.md) preserves actual
codec/seek/PTS/audio assertions, sanitizer results, library hashes/licenses,
upstream comparison builds, and failures. Its nonzero exits are intentional
qualification failures, not ignored tests. No application media adapter or
shipping FFmpeg build has been qualified.

The editing model still needs stable anchors, nested occurrences and overrides,
semantic selectors, an indexed render plan, and effects. Core edits currently
clone and validate the document; storage open/validation replays retained
history. Large-document/history latency and memory use are unqualified.
Persistence still needs migrations, restore/recovery UX, managed media and
relinking, host socket routing, and full hostile-project/failure coverage.

The UI remains the welcome shell. GUI aesthetics, natural keyboard editing,
focus/IME, accessibility, and complete keyboard-only workflows remain acceptance
work as the editor is built. No interactive GUI check was repeated for these
domain/storage changes. The earlier desktop-inspection limitation is recorded
in [setup verification](SETUP_VERIFICATION.md).

Next implementation priorities are the pure anchor/occurrence and render-plan
contracts, alongside a compatible pinned media build and adapter. GPU preview,
audio/DSP, model/runtime qualification, actual local AI holds, import/export,
distribution, and full release acceptance remain in the
[requirement tracker](REQUIREMENTS.md). No DP requirement or delivery gate is
marked complete.
