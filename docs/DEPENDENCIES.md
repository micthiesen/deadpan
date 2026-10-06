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
| egui_kittest | 0.36.2 | MIT OR Apache-2.0 | Optional developer `ui-harness` feature only. Replays the production eframe app using its shared Metal renderer and AccessKit geometry. Native input and physical display qualification remain separate. |
| image | 0.25.10 | MIT OR Apache-2.0 | Already locked; optional direct PNG capture and bounded checkpoint comparison for `ui-harness`, and (PNG feature only) the CLI's contain-resize and PNG encoding of AI hold conditioning frames. No image-diff runtime in the shipped app. |
| rfd | 0.17.2 | MIT | macOS-only asynchronous native file/save/folder panels. Default features disabled; no shell or external dialog executable. |
| objc2-foundation | 0.3.2 | MIT | Existing locked native dependency, now direct with narrowly selected features for safe system Documents-directory discovery through NSFileManager. No new runtime or unsafe application code. Also direct in the `deadpan-track` worker for the request `NSArray`, Vision `NSError` descriptions and the empty face-detection handler options `NSDictionary`. |
| objc2 | 0.6.4 | MIT | Existing locked dependency, now direct for a safe autorelease pool around Documents discovery on the service thread. Only an owned Rust path leaves the pool. Also direct in the `deadpan-track` worker for Vision object ownership and a per-picture autorelease pool. |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | Existing locked dependency (through muda and winit), now direct in `deadpan-app` with only `NSAccessibility` and `NSWorkspace` features, to read the macOS Reduce motion and Increase contrast settings through safe bindings. The app keeps `unsafe_code = "forbid"`. |
| muda | 0.21.0 | Apache-2.0 OR MIT | macOS-only native menu bar (default GTK features disabled). Reuses the locked objc2 0.6.4 / objc2-app-kit 0.3.2 stack; adds crossbeam-channel 0.5.17 and keyboard-types 0.8.3. Its Objective-C code stays inside the crate; the app keeps `unsafe_code = "forbid"`. |
| ureq | 3.4.2 | MIT OR Apache-2.0 | HTTPS model-pack downloads in `deadpan-models`, whose transport the CLI also uses for pinned [downloader helper](YOUTUBE_IMPORT.md#helper-bundle) installs: rustls 0.23 with ring, rustls-platform-verifier (system trust store), HTTPS-only redirects. No other network client is linked. |
| ring | 0.17.14 | Apache-2.0 AND ISC | Already locked through rustls; now direct in `deadpan-models` for Ed25519 verification (and release-time key generation and signing) of [signed update manifests](UPDATES.md). Adds no crates to the lock. |
| rustls (dev) | 0.23.45 | Apache-2.0 OR ISC OR MIT | Test-only dev-dependency of `deadpan-models`: the local HTTPS server in `src/packs/interrupted_download_tests.rs`. Same locked version and ring provider as ureq's client; default features off (`ring`, `std`). Adds no crates to the lock and is not linked into any shipped binary. |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | Already locked through `png`/`ureq`; direct in `deadpan-cli` only to inflate the single entry of the hash-verified Deno release ZIP. The extracted executable is verified against its own pinned SHA-256. |
| whisper-rs | 0.16.0 | Unlicense | macOS-only, in the `deadpan-transcribe` worker only, with Metal. Its safe abort wrapper is not used (see [transcription](TRANSCRIPTION.md)). Build uses bindgen 0.72.1 and cmake 0.1.58 with the host CMake and Clang. |
| whisper-rs-sys / whisper.cpp | 0.15.0 / 1.8.3 | Unlicense / MIT | Vendored whisper.cpp and ggml compiled statically into the worker executable; not linked into the app or CLI. |
| objc2-vision | 0.3.2 | Zlib OR Apache-2.0 OR MIT | macOS-only, in the `deadpan-track` worker only: bindings to the system Vision framework (`VNTrackObjectRequest`, `VNSequenceRequestHandler`, `VNDetectedObjectObservation`, and for face proposals `VNDetectFaceRectanglesRequest`, `VNImageRequestHandler`, `VNFaceObservation`) with default features disabled. Vision itself ships with macOS; nothing is bundled. Its `unsafe` calls stay in the worker's documented adapter (see [tracking](TRACKING.md)). |
| objc2-core-video | 0.3.2 | Zlib OR Apache-2.0 OR MIT | macOS-only, in `deadpan-track` only: owned BGRA `CVPixelBuffer` creation and locking for Vision input. |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | Existing locked dependency, now direct in `deadpan-track` only for `CGRect` and `CFRetained` ownership of pixel buffers. |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | Already locked through eframe; direct Metal/WGSL dependency for the shared picture baseline. |
| skrifa | 0.44.0 | MIT OR Apache-2.0 | Already locked through epaint; direct in `deadpan-render` (default features disabled, `std`) to read caption glyph outlines and metrics from the embedded Inter variable font (SIL OFL 1.1, `assets/brand/source`). Rasterization and compositing are Deadpan's own ([captions](CAPTIONS.md)). |
| pollster | 1.0.1 | Apache-2.0 OR MIT | Already locked; development-only offscreen GPU qualification. |
| serde | 1.0.229 | MIT OR Apache-2.0 | Validated domain, transaction, and protocol serialization. |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | Bounded project/command JSON and diagnostics. Core enables `raw_value` for borrowed retained-audio layout/input preflight before typed materialization. |
| rusqlite | 0.40.2 | MIT | Authoritative SQLite package/history, backup API, and SQLite limits. |
| SQLite via libsqlite3-sys | 3.53.2 via 0.38.2 | Public domain; binding MIT | Bundled with rusqlite; WAL, FULL synchronization, foreign keys, immutable revision/history writes. |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Atomic checkpoint files and isolated integration fixtures. |
| signal-hook | 0.3.18 | MIT OR Apache-2.0 | Safe SIGINT/SIGTERM cancellation flags for the headless Render owner. Signal callbacks only set an atomic flag; the owner pumps cancellation, journals and checked worker cleanup before releasing its writer. |
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

The development-only `deadpan-chaos` crate (the [adversarial suite](ADVERSARIAL.md)
engine) adds no third-party dependency: it uses the existing `serde_json`, and
replaces cargo-fuzz/libFuzzer, which need a nightly toolchain, with a stable
in-repository mutation runner. It is a dev-dependency only and never ships.
Its only `unsafe` is a documented forwarding `GlobalAlloc` wrapper over the
system allocator used to bound per-case allocation in test binaries.

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

## Downloader helpers

[YouTube import](YOUTUBE_IMPORT.md) runs external helper executables; none is
linked into Deadpan. This build accepts only these exact upstream release files.
`cargo xtask bundle` ships them as a read-only baseline in
`Deadpan.app/Contents/Resources/helpers`, which the running bundle prefers.
Deno keeps its upstream Developer ID signature and pinned bytes. yt-dlp is
re-signed with the hardened runtime, with no entitlements (measured). Its
compiled pin adds a signature-independent content hash, so the shipped file is
anchored to the pinned release and not to the bundle manifest
([packaging](PACKAGING.md#downloader-baseline)).
`deadpan-cli downloader install` downloads and verifies the same files into
`~/Library/Application Support/Deadpan/helpers`. That managed root serves
development builds and is the future update location. Signed update manifests
and rollback (§15.2) remain open.

| Helper | Pinned release file | Size and SHA-256 | License | Notes |
| --- | --- | --- | --- | --- |
| yt-dlp | 2026.08.19 `yt-dlp_macos` (universal2 PyInstaller one-file) | 37,146,048 bytes, `0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202` (release `SHA2-256SUMS`) | Unlicense | Official executable; bundles Python 3.14.6 and its optional libraries, including `yt_dlp_ejs`. |
| yt-dlp-ejs | 0.8.0, embedded in the yt-dlp executable | Covered by the yt-dlp hash; `downloader status --probe` reads the loaded version | Unlicense (scripts); bundled third-party JavaScript under its own notices | YouTube JavaScript challenge solver. `--no-remote-components` forbids fetching other versions. |
| Deno | 2.9.7 `deno-aarch64-apple-darwin.zip` | 38,469,316 bytes, `5cd46d6268f6f78f5d88bdc7159d20bd44cdaa4b3303474839f87ec6fe7ae25c` (release `.sha256sum`); extracted `deno` 80,982,000 bytes, `b73737579d5a84c160e3316487594783fa5c15f4e13252a6a07050b755317f1a` | MIT | The only enabled yt-dlp JavaScript runtime, by absolute path. |

Merging separately delivered picture and sound uses Deadpan's own isolated media
worker and pinned LGPL FFmpeg 8.0.3 libraries (stream copy, no codec), not an
`ffmpeg` executable; the developer prefix intentionally builds no `ffmpeg` program.
The bundle carries the yt-dlp, PyInstaller/Python (`THIRD_PARTY_LICENSES.txt`),
yt-dlp-ejs and Deno license notices, vendored byte-for-byte under
[packaging/notices](../packaging/notices). Deno publishes no aggregated notice
for its embedded V8 and crates; that gap remains open.

## Packaged FFmpeg

`cargo xtask bundle` copies the transitive libraries the executables load from
the pinned prefix (`libavcodec.62`, `libavformat.62`, `libavutil.60`,
`libswresample.6`, `libswscale.9`) into `Contents/Frameworks`. It rewrites them
to `@rpath` and records their shipped hashes, the configure line and the exact
source archive in the notices and SBOM. The build refuses a prefix that is not
8.0.3 or that enables GPL, nonfree or version-3 components. An FFmpeg
source-hosting or written-offer decision is still required before public
distribution.

## Private AI runtime

`cargo xtask bundle` ships the AI pause runtime inside the application
(specification §14.1–§14.2) from pins in
[`tools/ai-runtime/pins.json`](../tools/ai-runtime/pins.json), assembled by
[`tools/ai-runtime/build.py`](../tools/ai-runtime/build.py) on the build Mac.
Python is a build-time tool; nothing is installed or compiled on a user's Mac,
and the runtime never uses the user's Python, packages or environment.

| Component | Pin | License | Role |
| --- | --- | --- | --- |
| CPython 3.12.13 (python-build-standalone 20260325, `aarch64-apple-darwin-install_only_stripped`) | archive SHA-256 `c33a3485…f1286` | PSF-2.0 (plus the bundled libraries CPython's `LICENSE.txt` lists) | Interpreter; `pip`, `ensurepip`, Tcl/Tk, IDLE, tests, headers and manual pages removed after install |
| ltx-2-mlx (`ltx-core-mlx`, `ltx-pipelines-mlx` 0.15.8) | commit `3392d75934120b7e69eefbe55893f7ef82be92a4`, 139 files checked against `ltx-source-manifest.json`, `LICENSE` SHA-256 pinned | MIT | Model code, on the import path through a relative `.pth` |
| 36 wheels below | per-wheel SHA-256 from that checkout's `uv.lock` (SHA-256 `ae86edd…72823`), selected for cp312 arm64 at a macOS 26.0 floor | per row | Installed offline with `pip --no-index --no-deps --only-binary=:all:` |
| x264 r3222 | commit `b35605ace3ddf7c1a5d67a2eb553f034aef41d55`, tree `0700538866963f968154ea768289bf100350e84e` | GPL-2.0-or-later | Static library in the runtime's `ffmpeg` |
| FFmpeg 8.0.3 programs | the existing archive pin, configured `--enable-gpl --enable-libx264 --disable-autodetect --enable-zlib --disable-network` | GPL-2.0-or-later | `ffmpeg`/`ffprobe` the worker runs for the model's CRF-33 H.264 conditioning and lossless `libx264rgb` intermediates |

The wheel set is exactly the 38-package environment the qualified checkout's
`uv sync --frozen` produced (`runtime-inventory.json`), minus the two editable
LTX packages, and every binary wheel has the same platform tag uv chose on
macOS 26 (`build.py pins --lock <uv.lock>` regenerates it). `mlx` and
`mlx-metal` publish separate `macosx_14_0`, `macosx_15_0` and `macosx_26_0`
builds. Measured on 2026-10-05 (M5 Max, macOS 26.5.2, same project, Hold,
seed and otherwise identical bundled runtime), the `macosx_15_0` build took
254.6 s in the worker against 113.2 s for `macosx_26_0` (a first bundled run
with the 15.0 build took 178.5 s; the development environment with the 26.0
build took 106.1 s). The runtime therefore pins the 26.0 builds and declares
`minimum_macos: 26.0` in `runtime.json`; on an older macOS the app reports
"AI pauses need macOS 26.0 or later" instead of launching the worker. The rest
of the app keeps its macOS 15 floor. Upstream wheels are unmodified except that
absolute `LC_RPATH` entries left by their CI builds (Pillow's
`/Users/runner/...`) are deleted so nothing can load from outside the bundle;
every Mach-O is re-signed.

| Wheel | Version | License | File |
| --- | --- | --- | --- |
| annotated-doc | 0.0.4 | MIT | `annotated_doc-0.0.4-py3-none-any.whl` |
| anyio | 4.12.1 | MIT | `anyio-4.12.1-py3-none-any.whl` |
| certifi | 2026.2.25 | MPL-2.0 | `certifi-2026.2.25-py3-none-any.whl` |
| click | 8.3.1 | BSD-3-Clause | `click-8.3.1-py3-none-any.whl` |
| filelock | 3.25.2 | MIT | `filelock-3.25.2-py3-none-any.whl` |
| fsspec | 2026.2.0 | BSD-3-Clause | `fsspec-2026.2.0-py3-none-any.whl` |
| h11 | 0.16.0 | MIT | `h11-0.16.0-py3-none-any.whl` |
| hf-xet | 1.4.2 | Apache-2.0 | `hf_xet-1.4.2-cp37-abi3-macosx_11_0_arm64.whl` |
| httpcore | 1.0.9 | BSD-3-Clause | `httpcore-1.0.9-py3-none-any.whl` |
| httpx | 0.28.1 | BSD-3-Clause | `httpx-0.28.1-py3-none-any.whl` |
| huggingface-hub | 1.7.1 | Apache-2.0 | `huggingface_hub-1.7.1-py3-none-any.whl` |
| idna | 3.11 | BSD-3-Clause | `idna-3.11-py3-none-any.whl` |
| jinja2 | 3.1.6 | BSD-3-Clause | `jinja2-3.1.6-py3-none-any.whl` |
| markdown-it-py | 4.0.0 | MIT | `markdown_it_py-4.0.0-py3-none-any.whl` |
| markupsafe | 3.0.3 | BSD-3-Clause | `markupsafe-3.0.3-cp312-cp312-macosx_11_0_arm64.whl` |
| mdurl | 0.1.2 | MIT | `mdurl-0.1.2-py3-none-any.whl` |
| mlx | 0.32.2 | MIT | `mlx-0.32.2-cp312-cp312-macosx_26_0_arm64.whl` |
| mlx-arsenal | 0.2.4 | Apache-2.0 | `mlx_arsenal-0.2.4-py3-none-any.whl` |
| mlx-lm | 0.31.1 | MIT | `mlx_lm-0.31.1-py3-none-any.whl` |
| mlx-metal | 0.32.2 | MIT | `mlx_metal-0.32.2-py3-none-macosx_26_0_arm64.whl` |
| numpy | 2.4.3 | BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0 | `numpy-2.4.3-cp312-cp312-macosx_14_0_arm64.whl` |
| packaging | 26.0 | Apache-2.0 OR BSD-2-Clause | `packaging-26.0-py3-none-any.whl` |
| pillow | 12.1.1 | MIT-CMU | `pillow-12.1.1-cp312-cp312-macosx_11_0_arm64.whl` |
| protobuf | 6.33.6 | BSD-3-Clause | `protobuf-6.33.6-cp39-abi3-macosx_10_9_universal2.whl` |
| pygments | 2.19.2 | BSD-2-Clause | `pygments-2.19.2-py3-none-any.whl` |
| pyyaml | 6.0.3 | MIT | `pyyaml-6.0.3-cp312-cp312-macosx_11_0_arm64.whl` |
| regex | 2026.2.28 | Apache-2.0 AND CNRI-Python | `regex-2026.2.28-cp312-cp312-macosx_11_0_arm64.whl` |
| rich | 14.3.3 | MIT | `rich-14.3.3-py3-none-any.whl` |
| safetensors | 0.7.0 | Apache-2.0 | `safetensors-0.7.0-cp38-abi3-macosx_11_0_arm64.whl` |
| sentencepiece | 0.2.1 | Apache-2.0 | `sentencepiece-0.2.1-cp312-cp312-macosx_11_0_arm64.whl` |
| shellingham | 1.5.4 | ISC | `shellingham-1.5.4-py2.py3-none-any.whl` |
| tokenizers | 0.22.2 | Apache-2.0 | `tokenizers-0.22.2-cp39-abi3-macosx_11_0_arm64.whl` |
| tqdm | 4.67.3 | MPL-2.0 AND MIT | `tqdm-4.67.3-py3-none-any.whl` |
| transformers | 5.3.0 | Apache-2.0 | `transformers-5.3.0-py3-none-any.whl` |
| typer | 0.24.1 | MIT | `typer-0.24.1-py3-none-any.whl` |
| typing-extensions | 4.15.0 | PSF-2.0 | `typing_extensions-4.15.0-py3-none-any.whl` |

**GPL programs.** x264 has no LGPL-compatible substitute for the model's
upstream CRF-33 conditioning round trip, which the adapter must not silently
bypass, nor for the lossless RGB H.264 intermediate the host converter admits.
The runtime therefore carries a separate GPL build of the `ffmpeg` and
`ffprobe` programs. They run as separate processes and are distinct from the
LGPL libraries the application links. Specification §27.2 requires the owner
to decide this before distribution: `--identity` builds refuse to include the
runtime without `--allow-gpl-ai-codec`, and `build-provenance.json` records the
decision. Replacing them would need a re-qualified conditioning route (for
example VideoToolbox H.264) and an FFV1 intermediate admitted by the converter.

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
| Private runtime | python-build-standalone + pinned wheels (selected; [above](#private-ai-runtime)) | Offline assembly, isolation, nested signing and a scrubbed-environment generation pass on the build Mac; Developer ID library validation, notarization and a clean Mac remain. |
| Distribution | Apple Silicon app, signed helpers and model manifests | Relocatable hardened bundle, notices and CycloneDX SBOM are built by `cargo xtask bundle` ([packaging](PACKAGING.md)); Developer ID signing, notarization, online/offline clean-machine checks and model manifests remain. |

Do not add unused dependencies or placeholder crates to imply coverage. The
remaining component map is in [ARCHITECTURE.md](ARCHITECTURE.md). Original Deadpan
code uses MIT; this does not assign MIT terms to future bundled libraries or
model weights. Record those layers separately before distribution.
