# Dependency decisions

The specification selects native Rust, egui/eframe/wgpu on Metal, and a pure
editing core. These override the generic Bun/TypeScript setup defaults. The
workspace follows the maintained Rust conventions in sibling `beastie`:
edition 2024, resolver 3, Rust 1.97.1, shared dependencies, rustfmt, and Clippy.
There is no JavaScript runtime or credential requirement. The complete workspace
requires an explicit pinned FFmpeg developer prefix; see [Development](DEVELOPMENT.md).

## Adopted for the foundation

| Component | Direct pin | Upstream license | Scope |
| --- | --- | --- | --- |
| Rust | 1.97.1 | MIT OR Apache-2.0 | Compiler, rustfmt, Clippy. |
| eframe | 0.36.2 | MIT OR Apache-2.0 | Native project workspace, egui, wgpu and AccessKit. |
| rfd | 0.17.2 | MIT | macOS-only asynchronous native file/save/folder panels. Default features disabled; no shell or external dialog executable. |
| objc2-foundation | 0.3.2 | MIT | Existing locked native dependency, now direct with narrowly selected features for safe system Documents-directory discovery through NSFileManager. No new runtime or unsafe application code. |
| objc2 | 0.6.4 | MIT | Existing locked dependency, now direct for a safe autorelease pool around Documents discovery on the service thread. Only an owned Rust path leaves the pool. |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | Already locked through eframe; direct Metal/WGSL dependency for the shared picture baseline. |
| pollster | 1.0.1 | Apache-2.0 OR MIT | Already locked; development-only offscreen GPU qualification. |
| serde | 1.0.229 | MIT OR Apache-2.0 | Validated domain, transaction, and protocol serialization. |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | Bounded project/command JSON and diagnostics. Core enables `raw_value` for borrowed retained-audio layout/input preflight before typed materialization. |
| rusqlite | 0.40.2 | MIT | Authoritative SQLite package/history, backup API, and SQLite limits. |
| SQLite via libsqlite3-sys | 3.53.2 via 0.38.2 | Public domain; binding MIT | Bundled with rusqlite; WAL, FULL synchronization, foreign keys, immutable revision/history writes. |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Atomic checkpoint files and isolated integration fixtures. |
| thiserror | 2.0.20 | MIT OR Apache-2.0 | Typed storage and CLI errors. |
| uuid | 1.26.1 | MIT OR Apache-2.0 | Host-generated v4 project/node/revision identities; no randomness in core. |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Safe process-group signalling, unreaped exit observation, and nonblocking worker pipes on macOS/Linux. Already locked transitively; now pinned directly with `process` and `fs`. |
| libc | 0.2.189 | MIT OR Apache-2.0 | Existing macOS bindings inside `native/deadpan-process` and `native/deadpan-fileclone` for process-group membership and descriptor cloning. Uses system APIs, with no bundled native library. |
| sha2 | 0.11.0 | MIT OR Apache-2.0 | Streaming SHA-256 for worker snapshots and complete original identity; default `alloc`/`oid` features disabled. |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | Streaming internal content addresses for project-managed generated and original objects. |
| cc | 1.4.7 | MIT OR Apache-2.0 | Already locked transitively; direct build dependency for the isolated C codec adapter. |
| FFmpeg | 8.0.3, commit `8ae0b34901ba60a802f183ee75a250a9fc3e09a5` | LGPL 2.1 or later for the selected build | Generated-video helper and persistent source adapter, dynamically linked against the explicit prefix. GPL, nonfree, version-3, autodetected external libraries and networking are disabled. App bundling remains open. |
| proptest | 1.11.0 | MIT OR Apache-2.0 | Development-only exact-time, document, and transaction property tests. |

The native workspace uses [rfd 0.17.2](https://docs.rs/rfd/0.17.2/rfd/) with
main-thread panel construction and nonblocking future polling. Cargo reuses the
locked Objective-C dependencies. Native create/import/open panels and cancellation
need app interaction evidence; headless dialog tests cover polling and result
ownership. This does not establish bookmark access or signed bundle behavior.

`Cargo.toml` pins direct versions. `Cargo.lock` records every resolved transitive
version and registry checksum. The UI enables `accesskit`, `default_fonts`, and
`wgpu`, disables eframe's other default features, and checks for Metal at startup
on macOS. Application UI support is not qualification of video texture interop,
color management, accessibility, or the shared renderer.

`deadpan-models` adds host bundle qualification using the existing core, jobs,
media, serialization and BLAKE3 dependencies. It adds no registry dependency or
model runtime. Media now also links the persistent source adapter. Generation
integration tests continue to exercise the isolated native helper and SQLite
store; no model download is required by the repository gate.

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

[Worker verification](WORKER_VERIFICATION.md) records the process tests and
Darwin adapter qualification. The adapter checks the installed Apple SDK ABI and
[XNU's libproc wrapper](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/libsyscall/wrappers/libproc/libproc.c)
and [process enumeration](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/proc_info.c).
It links the system API through the pinned libc declarations. Tests qualify the
observed host behavior, not all OS versions or sandbox entitlements.

The [RustCrypto SHA-2 implementation](https://docs.rs/sha2/0.11.0/sha2/)
adds six locked transitive packages: block-buffer 0.12.1, cpufeatures 0.3.1,
crypto-common 0.2.2, digest 0.11.3, hybrid-array 0.4.15, and typenum 1.20.1.
Their published Cargo manifests all declare `MIT OR Apache-2.0`; registry
checksums are retained in `Cargo.lock`. The default implementation detects
available ARM SHA instructions and otherwise uses its portable software path.
No external executable or native library is added for hashing.

The FFV1 admission parser uses the default range probability table and bitstream
rules from [RFC 9043](https://www.rfc-editor.org/rfc/rfc9043.html). Its Code
Components carry the RFC's Simplified BSD notice in
[`video_codec.rs`](../native/deadpan-source/src/video_codec.rs). Preserve that
notice in future binary-distribution materials. The surrounding original parser
is MIT; it does not copy FFmpeg's LGPL implementation. This adds no dependency or
runtime download. The release notice/SBOM assembly remains open.

The [official BLAKE3 implementation](https://docs.rs/blake3/1.8.7/blake3/)
adds `constant_time_eq` 0.4.2 (`CC0-1.0 OR MIT-0 OR Apache-2.0`); its other
dependencies were already locked. The default `std` feature is enabled;
optional mmap and parallel hashing are disabled. BLAKE3 supports the specification's
internal content addressing and does not replace SHA-256 worker declarations.
The [generated-object store](GENERATED_MEDIA.md) uses the existing safe rustix
filesystem bindings for descriptor-relative paths, exclusive rename, and macOS
`F_FULLFSYNC`. No external runtime is added for this storage boundary.

The storage foundation tests writer ownership, read-only coexistence, durable
undo/redo, retained branches, actual SQLite disk-full rollback, interrupted
transactions, and live backup consistency. Schemas 1 through 9 migrate through a
validated consistent copy into schema 10. [Original ownership](ORIGINAL_MEDIA.md)
shares the generated-object engine and uses `deadpan-fileclone` with the existing
libc pin. Real APFS clone independence and forced verified-copy fallback are
tested. No registry package or end-user runtime was added. This is partial
storage qualification; authored import, reference collection, portable-copy
workflow and restore/recovery UI remain open.

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
The reports retain failures and complete notices.

The [canonical worker prototype](qualification/audio-canonical-2026-09-20.md)
uses a fixed 256-sample schedule, exact rational input boundaries, padded/cropped
context, replay, and prepared PCM. A 120 ms window with 15 ms analysis steps
passes 3,447 checks in each normal/sanitized run, with 558 matching PCM hashes.
The 120/30 and 60/15 alternatives retain their transient/pitch failures. The
prototype's worst measured consumption call exceeds a device deadline; DSP and
file reads must stay off the callback. App binding, plan integration, cache/job
lifecycle, output devices, listening, and the remaining audio operations are
still unqualified.

The [production DSP boundary](AUDIO_DSP.md) now vendors those exact MIT headers
under `native/deadpan-dsp/vendor`, including upstream forwarding headers, complete
notices, pins and per-file SHA-256 sums. It uses the existing pinned `cc` build
dependency. Rust owns bounded planar input for the lifetime of the C++ renderer;
the adapter admits one constant-rate/integer-pitch recipe, bounded reads and
cooperative replay. The qualification harness includes the same canonical
header. This is a worker adapter, not a native output backend or a complete
audio renderer. No new registry dependency or build-time download is introduced.

## First real local model probe

The [LTX MLX smoke](qualification/model-smoke-2026-09-20.md) pins runtime commit
`3392d75934120b7e69eefbe55893f7ef82be92a4`, the LTX-2.3 q4 pack, and the Gemma
4-bit text encoder. Verified selected files total 36,152,862,913 bytes. On the
reference M5 Max, one cold-process generation produced 25 silent 768×320 frames
at 24 fps in 90.968 seconds inside the generation call. It does not establish
warm latency, an authored interior Hold, corpus acceptability, or a selected
shipping backend. Missing color tags and developer GPL FFmpeg use remain
unqualified. Runtime, LTX, and Gemma license layers are recorded separately;
neither runtime nor weights are approved for redistribution by this probe.

The [supervised development adapter](qualification/model-worker-2026-09-21.md)
connects this same candidate to the Rust process boundary and exact bridge plan.
Its tagged, lossless RGB serialization still uses developer GPL FFmpeg. This
qualifies a development boundary, not an app codec/runtime redistribution choice.

The [FFV1 master probe](qualification/ffv1-2026-09-21.md) converts the actual
native sequence and sampled candidate through the isolated LGPL FFmpeg 8.0.3
libraries. Normal and instrumented adapters preserve every RGB8 pixel, frame
ordinal, and color tag in FFV1 v3/Matroska, with slice CRC enabled. The container
clock rounds to milliseconds, so native rational timing and sampling remain
separate metadata. Tail truncation can preserve all decodable pictures; artifact
hash and length checks remain required. The subsequent
[host converter](MEDIA_CONVERSION.md) integrates this route through a narrow C
adapter in an isolated helper, with safe Rust supervision and descriptor-only I/O.
The native build checks exact headers, and the worker checks loaded library
versions, configurations, and LGPL licenses. This choice reuses the qualified C
conversion loop; it does not select a general Rust media binding for import or
playback. It does not replace the model worker's development serialization
dependency or qualify app bundling, signing, or redistribution of model runtimes.

## Source resampling boundary

`deadpan-audio` adds no third-party package or native library. It reuses the
locked core/media/serialization dependencies and implements a bounded,
versioned finite FIR in safe Rust. Exact fractional start phase is part of the
source mapping contract. The pinned FFmpeg SWR API's integer drop/insertion and
phase-table resolution do not supply an arbitrary initial fractional position;
Rubato's current asynchronous API also keeps its running phase private. Neither
was added as an unused dependency or treated as an exact-phase solution.

[Source preparation](AUDIO_PREPARATION.md) defines the kernel and speaker matrix.
Its [measurements](qualification/audio-preparation-2026-09-21.md) retain actual
signal errors and block costs. This is a worker sampler, not a replacement for
the qualified stretch adapter or an audio-device selection. A full listening
corpus, format/layout choices, voice integration and callback scheduling remain
required.

## Qualification still required

The [prepared output boundary](AUDIO_OUTPUT.md) pins CPAL 0.18.2 and rtrb 0.4.0
for a narrow macOS device probe. [Source audit](qualification/audio-output-dependency-audit-2026-09-21.md)
and [hardware results](qualification/audio-output-2026-09-21.md) keep normal
callback evidence separate from exceptional OS-clock allocation, device changes,
physical-format side effects, acoustics and full transport qualification.
[Notices](../native/deadpan-output/THIRD_PARTY.md) retain exact dependency/license
provenance; these do not establish a signed application distribution.

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
