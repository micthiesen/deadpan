# Dependency decisions

The specification selects native Rust, egui/eframe/wgpu on Metal, and a pure
editing core. These override the generic Bun/TypeScript setup defaults. The
workspace follows the maintained Rust conventions in sibling `beastie`:
edition 2024, resolver 3, Rust 1.97.1, shared dependencies, rustfmt, and Clippy.
There is no JavaScript runtime, environment configuration, or credential
requirement in this scaffold.

## Adopted for the foundation

| Component | Direct pin | Upstream license | Scope |
| --- | --- | --- | --- |
| Rust | 1.97.1 | MIT OR Apache-2.0 | Compiler, rustfmt, Clippy. |
| eframe | 0.36.2 | MIT OR Apache-2.0 | Native welcome shell, egui, wgpu, AccessKit. No media preview yet. |
| serde | 1.0.229 | MIT OR Apache-2.0 | Typed diagnostic report serialization. |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | JSON diagnostics. |
| proptest | 1.11.0 | MIT OR Apache-2.0 | Development-only exact-time property tests. |

`Cargo.toml` pins direct versions. `Cargo.lock` records every resolved transitive
version and registry checksum. The UI enables `accesskit`, `default_fonts`, and
`wgpu`, disables eframe's other default features, and checks for Metal at startup
on macOS. Application UI support is not qualification of video texture interop,
color management, accessibility, or the shared renderer.

The app's initial deployment target is macOS 15; the test host and actual
verification are recorded in [SETUP_VERIFICATION.md](SETUP_VERIFICATION.md).
The target setting does not prove compatibility with every supported OS.

Version and feature references: [eframe 0.36.2](https://docs.rs/eframe/0.36.2/eframe/),
[egui source](https://github.com/emilk/egui),
[proptest](https://github.com/proptest-rs/proptest),
[serde](https://github.com/serde-rs/serde), and
[serde_json](https://github.com/serde-rs/json).

## Qualification still required

Gate A remains open. Before adding each executable or native dependency, record
its exact revision, source and transitive licenses, build configuration,
supported OS/hardware, measured output, packaging requirements, and evidence.
Keep this log current; a selection in the spec is not a tested integration.

| Boundary | Spec candidate | Required qualification |
| --- | --- | --- |
| Media | rsmpeg + pinned FFmpeg; compare isolated Cutlass components | Decode/seek/encode, VFR and AAC sync, native ownership, color, codec/license flags. |
| GPU preview | wgpu/Metal, narrow objc2 interop | Actual decoded textures, lifetime/synchronization, color, preview/export parity. |
| Audio | CPAL + Signalsmith Stretch | Device changes, callback deadlines, DSP latency/preroll, random access, license pin. |
| Storage | rusqlite + bundled SQLite | Single writer, durable transactions, crash recovery, WAL-safe snapshots, migrations. |
| Analysis | whisper.cpp, Silero VAD, Apple Vision; ort where useful | Correctable word timing, tracking loss, privacy, actual model/runtime packaging. |
| AI baseline | Distilled LTX-Video 2B on a supported MPS path | Real hold corpus, exact seams/duration, usable-output latency and memory by hardware tier. |
| AI comparison | Pinned LTX MLX implementation and compatible weights | Same corpus, runtime/code/weight licenses, precision and endpoint support. |
| Import | yt-dlp + EJS + Deno | Permitted-source import on a clean Mac, pinning, safe updates, interrupted downloads. |
| Private runtime | python-build-standalone + pinned wheels if selected | Offline assembly, isolation, nested signing, no first-launch pip or user runtime. |
| Distribution | Apple Silicon app, signed helpers and model manifests | Notarization, online/offline clean-machine checks, SBOM and notices. |

Do not add unused dependencies or placeholder crates to imply coverage. The
remaining component map is in [ARCHITECTURE.md](ARCHITECTURE.md). Original Deadpan
code uses MIT; this does not assign MIT terms to future bundled libraries or
model weights. Record those layers separately before distribution.
