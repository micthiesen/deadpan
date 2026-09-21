# Development

Deadpan uses a Rust workspace, pinned to Rust 1.97.1. Its native application targets Apple Silicon macOS; the specification proposes macOS 15 as the initial deployment baseline, pending qualification. Cargo installs/builds the locked Rust dependencies. Native development requires the macOS build tools.

The foundation needs no credentials, media tools, model weights, Python environment, or external services. Later media and model dependencies must pass Gate A and be bundled for end users. Development instructions must never become a requirement for using the distributed application.

## Validation gate

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo run -p deadpan-cli -- doctor
```

Use `cargo fmt --all` to apply formatting. Keep `Cargo.lock` committed; dependency updates are explicit reviewed changes. Tests should establish meaningful behavior and failure modes rather than mirror implementation.

Prefer meaningful unit tests, integration tests, and deterministic headless harnesses for most verification. Keep keyboard grammar, focus routing decisions, command transactions, geometry, and worker state transitions testable without live UI automation. Review GUI aesthetics and natural keyboard navigation explicitly. Use computer interaction when visual quality, native focus/IME, accessibility, or end-to-end ergonomics need observation.

The gate verifies only the implemented foundation. It does not establish media accuracy, AI quality, accessibility conformance, signed distribution, or performance budgets. Those require the evidence in the [requirement tracker](REQUIREMENTS.md) and specification.

## Native application smoke test

On a supported Apple Silicon Mac:

```sh
cargo run -p deadpan-app -- --smoke-test
cargo run -p deadpan-app
```

`--smoke-test` opens the native window, closes it after frames have rendered, and checks the shutdown callback. Run it for native startup or lifecycle changes. Where an interactive check adds evidence, confirm the affected layout, focus, keyboard navigation, and close behavior. Do not repeat GUI testing for unrelated pure-core changes. Confirm no media/import/render controls imply unavailable functionality. For lifecycle changes, verify the intended quit/SIGTERM behavior and exit status. Record the actual OS/hardware and what was observed; a successful compile is not a UI smoke test.

The shell does not test frame decoding, a media texture path, realtime audio, or preview/export equivalence. Qualify those using isolated technical harnesses before integrating an editing workspace.

## Implementation sequence

1. Read [the specification](spec/DEADPAN_SPEC.md), [handoff](spec/AGENT_HANDOFF.md), and [AGENTS.md](../AGENTS.md). Sections 1–8 define semantics, 12–14 define AI/runtime contracts, 17–24 define architecture, and 26–31 define verification and command details.
2. Consult [Architecture](ARCHITECTURE.md) for current responsibilities and planned boundaries. Keep the pure core independent; add a crate when working code benefits from isolation.
3. Work on Gate A qualification alongside the pure Gate B domain foundation. Record decisions with exact dependency revisions, licenses, failures, fixture inputs, and reproducible measurements.
4. Add meaningful tests at the relevant boundary, run the validation gate, and exercise the affected native behavior. Preserve concurrent changes and review the complete scoped diff.
5. Update the requirement tracker with implementation links, tests, measured acceptance evidence, and outstanding work. Commit and push authorized scoped work to `main`.

## Evidence discipline

For deterministic core behavior, cover fractional rates, origin-based frame/sample mapping, invalid inputs, overflow, half-open ranges, and repeat gaps. As structures arrive, add generated nested documents, inverse transaction properties, identity/anchor invariants, and serialization/render-plan equivalence.

For media work, generate frame-number/impulse/color fixtures and verify the actual encoded file, including VFR, negative/non-zero PTS, delay, rotation, color, and channel layouts. Preview and export must agree within documented stage-specific tolerances. Listen to audio boundary cases as well as measuring PCM.

For AI work, keep lifecycle test doubles separate from actual generation acceptance. Qualify a rights-cleared real-video corpus, exact seams/duration, stale-result handling, usable-output latency, memory pressure, failures, and accepted-project offline playback. A downloaded model or returned file does not satisfy DP-12.

For release, run the full keyboard-only workflow and clean-machine online/offline installer checks without developer tools or preexisting model caches. Publish reproducible performance results, fixture reports, dependency notices/SBOM, and migration policy. No gate passes through unrun or ignored checks.

## Documentation ownership

`docs/spec/` preserves the supplied complete design package. The Markdown specification is normative and the PDF is its reading edition. Evolving implementation decisions, qualification reports, and deviations belong in project documentation with links back to the relevant requirement and section. Keep [AGENTS.md](../AGENTS.md) concise and update durable conventions as they emerge.
