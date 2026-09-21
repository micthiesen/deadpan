# Setup verification

Verified on 20 September 2026 using the initial source scaffold and its committed
`Cargo.lock`: Apple M5 Max, 128 GB memory, Apple Silicon macOS 26.5.2 (25F84),
Rust/Cargo 1.97.1. These results cover setup only. They do not qualify Gates A
through G or establish performance budgets.

| Check | Observed result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed after applying rustfmt. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed. |
| `cargo test --workspace --locked` | 12 tests passed: 10 core tests including 3 property tests, and 2 CLI integration tests. None ignored. |
| `cargo build --workspace --locked` | All three crates built successfully. |
| `cargo run --locked -p deadpan-cli -- doctor` | Valid JSON; the actual core probe maps frame 1 at 30000/1001 fps to sample 1602 and reports missing product capabilities. |
| `cargo run --locked -p deadpan-app -- --smoke-test` | Initialized `Apple M5 Max (Metal)`, ran the native UI, requested window closure, reached the shutdown callback, and exited successfully. |
| Normal app launch and SIGTERM | Process launched; SIGTERM produced the standard signal exit status 143. This is not the graceful window-close path and does not invoke a save/flush contract. The scaffold has no project or background work to save. |
| Imported design integrity | All five files compared byte-for-byte with the source ZIP. SHA-256 hashes are in [SPEC_PROVENANCE.md](SPEC_PROVENANCE.md). |
| Requirement coverage | Exactly DP-01 through DP-24 are present in the tracker, with no requirement marked complete. |

The 10,000-frame fractional-rate test totals exactly 16,016,000 samples at
30000/1001 fps by subtracting common-origin boundaries. Other tests cover
positive/negative ties-to-even, invalid rational rates, normalized ratios,
range/duration overflow, and repeat gaps only between total plays. These are
arithmetic results, not measured encoded-media synchronization.

Interactive resize, focus, keyboard, and visual inspection remain unverified.
The desktop inspection tool could not select the bare Cargo executable. A
temporary `.app` wrapper was recognized as running, but selection repeatedly
failed with `cgWindowNotFound`. The test processes were stopped. The wrapper is
outside the repository and is not an application distribution. Native startup
and orderly window closure were checked separately by the passing smoke test.

Media decode/seek/encode, realtime audio, video texture/color paths, actual AI,
project persistence/recovery, accessibility acceptance, macOS 15 runtime
compatibility, lower-memory hardware, signing/notarization, and clean-machine
online/offline installation have not been tested or implemented. Follow
[REQUIREMENTS.md](REQUIREMENTS.md) for their remaining acceptance work.

The macOS GitHub Actions workflow repeats formatting, lint, tests, build, and
headless diagnostics. It deliberately does not treat a hosted runner as a
substitute for interactive native or clean-machine release qualification.
