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
| serde | 1.0.229 | MIT OR Apache-2.0 | Validated domain, transaction, and protocol serialization. |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | Bounded project/command JSON and diagnostics. |
| rusqlite | 0.40.2 | MIT | Authoritative SQLite package/history, backup API, and SQLite limits. |
| SQLite via libsqlite3-sys | 3.53.2 via 0.38.2 | Public domain; binding MIT | Bundled with rusqlite; WAL, FULL synchronization, foreign keys, immutable revision/history writes. |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Atomic checkpoint files and isolated integration fixtures. |
| thiserror | 2.0.20 | MIT OR Apache-2.0 | Typed storage and CLI errors. |
| uuid | 1.26.1 | MIT OR Apache-2.0 | Host-generated v4 project/node/revision identities; no randomness in core. |
| proptest | 1.11.0 | MIT OR Apache-2.0 | Development-only exact-time, document, and transaction property tests. |

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
[serde_json](https://github.com/serde-rs/json),
[rusqlite 0.40.2](https://docs.rs/rusqlite/0.40.2/rusqlite/),
[SQLite license](https://www.sqlite.org/copyright.html),
[tempfile](https://github.com/Stebalien/tempfile),
[thiserror](https://github.com/dtolnay/thiserror), and
[uuid](https://github.com/uuid-rs/uuid).

The storage foundation tests writer ownership, read-only coexistence, durable
undo/redo, retained branches, actual SQLite disk-full rollback, interrupted
transactions, and live backup consistency. This is partial storage
qualification. Schema-1-to-2 migration now replays a backed-up copy and atomically
promotes it through SQLite. Restore/recovery policy and portable managed-media
ownership remain open.

## Measured media candidates

[The 2026-09-20 native report](qualification/media-2026-09-20.md) records exact
libraries, source commits, licenses, hashes, and failed samples. The developer
Homebrew FFmpeg 9.0.1 build is GPL version 3 or later and is not bundled or
selected for distribution. Hardware VideoToolbox with B-frames failed direct
muxing; explicit hardware with B-frames disabled and explicit OS software
encoding passed the scoped tiny-fixture checks. VFR requires an indexed PTS
contract rather than trusting raw duration metadata.

Pinned rsmpeg `b21fcfde8bb1ffdc179504e370e330385baa9819` does not compile against
that FFmpeg 9 build. Pinned Cutlass
`22437e2837340c7c57d62e438117f9a0fb4096d2` passes its scoped upstream tests but
fails Deadpan's nonzero-start timestamp fixture. Neither combination is adopted
unchanged. The [compatible qualification](qualification/media-compatible-2026-09-20.md)
now establishes the same rsmpeg pin with signed FFmpeg 8.0.3, built separately
under LGPL 2.1-or-later flags with no GPL/nonfree codecs. Normal and sanitizer
harness runs pass 252 positive assertions each, including original picture
identity and retained-frame ownership. Hardware/VFR B-frame mux failures and VFR
terminal-duration loss remain explicit negatives. An explicit 240000 Hz MP4 movie
timescale preserves the measured AAC offset. App integration, no-edit-list export,
format/color coverage, relocation/signing, and a shipping bundle remain open.

## Measured audio candidate

[Signalsmith qualification](qualification/audio-2026-09-20.md) pins Stretch
1.3.2 at `57b93f4e9206a089a45387eaa39bdc9f310d3308` and Linear 0.3.1 at
`5668673560146a9cfe38c25315071e3fd68c8317`, both MIT. The isolated C++17
harness uses the portable FFT, 48 kHz stereo, five speeds, and three pitch
settings. Normal and ASan/UBSan runs each pass 523 of 605 declared targets;
82 fail. Pitch, dynamics, channel levels, duration, reset, and latency alignment
pass. Block partition and local-seek equivalence, short clips, and realtime
deadlines are not qualified. All 130 PCM hashes match across the two builds.
The reports retain failures and complete notices. No app binding, DSP adapter,
or device output has been added. Preview/export must share a measured processing
schedule and state strategy before integration can qualify.

## Qualification still required

Gate A remains open. Before adding each executable or native dependency, record
its exact revision, source and transitive licenses, build configuration,
supported OS/hardware, measured output, packaging requirements, and evidence.
Keep this log current; a selection in the spec is not a tested integration.

| Boundary | Spec candidate | Required qualification |
| --- | --- | --- |
| Media | rsmpeg + pinned FFmpeg; compare isolated Cutlass components | Decode/seek/encode, VFR and AAC sync, native ownership, color, codec/license flags. |
| GPU preview | wgpu/Metal, narrow objc2 interop | Actual decoded textures, lifetime/synchronization, color, preview/export parity. |
| Audio | CPAL + pinned Signalsmith Stretch/Linear | Shared processing schedule, state-aware seeks, short clips, app binding, device changes, callback deadlines, speech/music listening review. |
| Storage | Adopted rusqlite + bundled SQLite | Complete semantic integrity/recovery, migrations, managed media, bounded history, and lifecycle failure qualification. |
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
