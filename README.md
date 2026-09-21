# Deadpan

**A keyboard-native editor for making a moment last considerably too long.**

Deadpan is a Rust-native macOS video editor designed around editable beats: pauses, repeats, reactions, selective audio emphasis, reframing, and local AI-generated holds. Timing and attention are the organizing ideas. The full design targets Apple Silicon and an eventual self-contained application.

This repository currently contains a **setup foundation**, not a working video editor:

- `deadpan-core`: exact frame/sample/rate/range and repeat-duration arithmetic.
- `deadpan-app`: an `egui`/`eframe` native development welcome shell using `wgpu` on Metal.
- `deadpan-cli`: a headless `doctor` diagnostic entry point.

Media import, editing commands, project storage, playback, AI generation, and export are not implemented. All 24 full-product requirements remain open or partial in the [requirement tracker](docs/REQUIREMENTS.md).

## Run the foundation

Development uses Rust 1.97.1, Cargo, and the macOS native build tools. The pinned toolchain is selected by `rust-toolchain.toml`.

```sh
cargo run -p deadpan-cli -- doctor
cargo run -p deadpan-app
```

The application command opens the development shell on macOS. `doctor` reports the current foundation; it does not qualify media, models, or packaged runtimes. No secrets or model downloads are required for this scaffold. These developer prerequisites are separate from the finished product's zero-manual-setup installation requirement.

For checks and native smoke testing, see [Development](docs/DEVELOPMENT.md) and the [setup verification report](docs/SETUP_VERIFICATION.md). The initial product deployment target is Apple Silicon macOS 15, subject to dependency qualification; that target is not a tested release support claim.

## Specification and project map

The complete original design package is preserved in [`docs/spec`](docs/spec/README.md), with [source checksums](docs/SPEC_PROVENANCE.md):

- [Full product specification](docs/spec/DEADPAN_SPEC.md), the normative 33-section source.
- [PDF reading edition](docs/spec/DEADPAN_SPEC.pdf).
- [Implementation-agent handoff](docs/spec/AGENT_HANDOFF.md).
- [Keyboard reference](docs/spec/KEYBOARD_REFERENCE.md), specifying the planned editing language.

[Agent instructions](AGENTS.md) include the design philosophy and exact validation gate. [Architecture](docs/ARCHITECTURE.md) maps the current crates to the complete component design. [Requirements and delivery gates](docs/REQUIREMENTS.md) distinguish implemented foundations from the evidence still needed for release.

Work begins with dependency qualification and the pure editing foundation. Completing an early gate does not reduce the full specification or establish release readiness.
