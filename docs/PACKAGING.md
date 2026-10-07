# Packaging

`cargo xtask bundle` builds a self-contained, relocatable `Deadpan.app` for
Apple Silicon macOS 15 or later. It is groundwork for DP-22 and Gate F: the
bundle runs from any location without Cargo, the FFmpeg developer prefix,
Homebrew or a separately installed downloader. Deadpan is a personal app with
no Apple developer account (owner decision 2026-10-05): the bundle is ad hoc
signed, or signed with the dotfiles local signing identity when a stable
signature is needed, and is never notarized or distributed.

## Build

```sh
export DEADPAN_FFMPEG_PREFIX="$HOME/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix"   # build time only
cargo xtask bundle --output /tmp/deadpan-bundle
cargo xtask bundle-verify /tmp/deadpan-bundle/Deadpan.app
```

`bundle` options:

| Option | Meaning |
| --- | --- |
| `--output DIR` | Required. Writes `DIR/Deadpan.app`, `DIR/Deadpan.app.SHA256SUMS` and `DIR/Deadpan.sbom.cdx.json`. An existing `Deadpan.app` is never replaced. |
| `--helpers-from ROOT` | Managed helper root to copy yt-dlp and Deno from. Defaults to `~/Library/Application Support/Deadpan/helpers`. Run `deadpan-cli downloader install --root ROOT` first; the build refuses anything but the verified pinned files. |
| `--identity NAME` | Sign with this keychain identity, with secure timestamps, instead of ad hoc. |
| `--notary-profile PROFILE` | After Developer ID signing, notarize, staple and assess the staged bundle before publishing it. Requires `--identity`. |
| `--allow-dirty` | Permit `--identity` builds from a working tree with uncommitted or untracked changes. They are refused otherwise; ad hoc builds warn. Either way `build-provenance.json` records the state. |
| `--no-build` | Reuse the executables in `<cargo target>/bundle/release`. |
| `--without-ai-runtime` | Leave out the private AI runtime; AI pauses are then unavailable. |
| `--ai-runtime-cache DIR` | Build-input cache for the AI runtime (default `~/Library/Caches/Deadpan/build-inputs`). |
| `--ltx-checkout DIR` | Copy the pinned LTX source from this checkout (each file is still verified) instead of fetching the commit from GitHub. |
| `--allow-gpl-ai-codec` | The owner's decision to distribute the runtime's GPL `ffmpeg`/`ffprobe`; required with `--identity` unless `--without-ai-runtime`. |

The build runs `cargo build --release --locked --bins
--message-format=json-render-diagnostics` for `deadpan-app`, `deadpan-cli`,
`deadpan-media-worker`, `deadpan-transcribe` and `deadpan-track`. It uses its own
target directory, `<cargo metadata target_directory>/bundle`, and takes each
executable path from Cargo's artifact messages, never a hard-coded
`target/release`. The build assembles the bundle in a hidden staging directory
and publishes it by rename only after these steps pass:

- signing and `codesign --verify --deep --strict`;
- the load-reference audit;
- the staged bundle's own `deadpan-cli downloader status`, run as a packaged
  app;
- notarization, when requested.

A failure leaves nothing published. Icon compilation reuses
`tools/brand/native_icons.py`, which needs Xcode's `actool`; Python is a
build-time tool only.

### Reproducibility

- The separate target directory builds Rust with `--remap-path-prefix`, mapping
  the checkout to `/deadpan` and the Cargo home to `/cargo`. C and C++ built by
  `cc` and CMake gets the same `-ffile-prefix-map` through `CFLAGS` and
  `CXXFLAGS`, so binaries carry no checkout path or user name. The audit
  reports any remaining `$HOME` strings. Compile-time `env!` paths, such as the
  disabled AI worker default, are not remapped.
- `SOURCE_DATE_EPOCH`, when set, fixes the SBOM and provenance timestamps.
- `xattr -cr` removes quarantine, provenance and Finder attributes from every
  copied file before nested signing and again before the bundle is signed.
- Builds are not yet bit-for-bit reproducible: code signatures, the icon
  compiler and linker output have not been compared across two builds.

## Layout

| Path | Contents |
| --- | --- |
| `Contents/MacOS/deadpan-app` | Main executable; also `--headless` and the render worker. |
| `Contents/MacOS/deadpan-cli`, `deadpan-media-worker`, `deadpan-transcribe`, `deadpan-track` | Helper tools. Workers already resolve beside the running executable, so no lookup changed. |
| `Contents/Frameworks/lib{avcodec.62,avformat.62,avutil.60,swresample.6,swscale.9}.dylib` | The transitive pinned LGPL FFmpeg 8.0.3 libraries the executables load. |
| `Contents/Resources/helpers/{yt-dlp,deno}/<version>/` and `manifest.json` | Read-only downloader baseline. |
| `Contents/Resources/ai-runtime/` | Private AI runtime: `python/` (CPython 3.12.13 with the locked wheels), `ltx-2-mlx/` (139 pinned source files), `worker/`, `bin/{ffmpeg,ffprobe}` (GPL) and `runtime.json`. See [AI runtime](#ai-runtime). |
| `Contents/Resources/Notices/` | `THIRD_PARTY_NOTICES.txt`, FFmpeg, yt-dlp, yt-dlp-ejs and Deno upstream notices, the aggregated Deno/V8 notices (`deno/THIRD_PARTY_NOTICES.txt`, `deno/notices-manifest.json`), SPDX texts, `sbom.cdx.json`. |
| `Contents/Resources/build-provenance.json` | Commit, tracked-change flag, rustc, Xcode, SDK, FFmpeg configuration, signature kind and limitations. |
| `Contents/Resources/Assets.car`, `Deadpan.icns` | Layered icon and ICNS fallback. |

`Info.plist` declares `DeadpanPackaging = xtask-bundle-1`, which marks a packaged
bundle (below), and `dev.deadpan.Deadpan`, the workspace version,
`LSMinimumSystemVersion` 15.0, arm64 and folder-access usage strings for
Documents (the project library), Desktop, Downloads and removable volumes. The
app uses no camera, microphone or other privacy-gated service.

## FFmpeg relocation

Every non-system load command must name a file in the pinned prefix. Each such
library is copied to `Contents/Frameworks`, its install name becomes
`@rpath/<name>`, and every reference in executables and libraries is rewritten
to `@rpath/<name>`. Executables gain `@executable_path/../Frameworks`, libraries
gain `@loader_path`, and absolute build-prefix search paths are deleted.
`install_name_tool` invalidates the linker's ad hoc signatures, so all code is
signed afterwards.

`cargo xtask bundle-audit APP` checks the Deno notices (below) and every
Mach-O file in a bundle. Each
dependency must be an absolute, already-normalized path under
`/System/Library/` or `/usr/lib/`, or it must resolve through `@rpath`,
`@executable_path` or `@loader_path` to a file inside the bundle. Paths are
normalized and symbolic links resolved first. Every `LC_RPATH` must be
bundle-relative and must normalize to a directory inside the bundle. The
`otool -L` parser drops only each architecture block's own `LC_ID_DYLIB` line.

The audit also reports, without failing, non-load strings that mention the
build prefix, `/opt/homebrew`, `/usr/local/` or the building user's `$HOME`.
The FFmpeg libraries embed their configure line, and the AI development
runtime's disabled defaults remain as strings; neither is loaded.

## Runtime lookup

No shipped code reads the working directory or `DEADPAN_FFMPEG_PREFIX` at run
time. `deadpan_cli::bundle::running_contents()` recognizes
`X.app/Contents/MacOS/<executable>` after resolving the executable path.
`packaged_contents()` also requires the `DeadpanPackaging` key in that bundle's
`Info.plist`, which the main executable's signature binds. Developer wrappers
from `tools/build-app.py` therefore keep development behavior.

- Workers: `deadpan-media-worker`, `deadpan-transcribe` and `deadpan-track`
  resolve beside the running executable, which inside the bundle is
  `Contents/MacOS`. Rendering re-executes the current executable.
- FFmpeg: dyld resolves `@rpath` to `Contents/Frameworks`. `doctor` matches
  each loaded libavcodec, libavformat, libavutil and libswscale image to a
  `Contents/Frameworks` file by mapped device and inode, so it reports where the
  loader actually mapped them.
- Downloader: a compatible active [signed update](UPDATES.md#downloader-updates)
  under `~/Library/Application Support/Deadpan/helpers` comes first. Otherwise
  a packaged bundle uses its own `Contents/Resources/helpers`; when that
  baseline is missing or damaged, `downloader status`, `--probe`, imports and
  `doctor` report `DownloaderHelperInvalid` and never fall back to another
  copy. Outside a packaged bundle the baseline is the managed root's compiled
  pins. An explicit `--root`/`--helpers` names a managed root (with its own
  active update). `downloader install` and `downloader update` write only the
  managed root, never the bundle (§15.2).
- Models: whisper.cpp, the AI pause weights and other packs stay in
  `~/Library/Application Support/Deadpan/Models`, installed on explicit request
  by download or offline import ([model packs](MODEL_PACKS.md)). None ship in
  the bundle.
- AI bridge runtime: a packaged app uses only its own
  `Contents/Resources/ai-runtime` and the `ltx-2.3-q4-bridge` pack installed in
  the models root. It ignores all `DEADPAN_BRIDGE_*` variables and development
  defaults, including Homebrew FFmpeg, the checkout's worker script, and the
  `/private/tmp` and cache locations. A developer may opt in with
  `DEADPAN_DEVELOPER_BRIDGE=1`, after which only explicitly set
  `DEADPAN_BRIDGE_*` variables are used. Development builds and developer
  wrappers keep the variables and defaults. `doctor` reports the lookup, the
  bundled runtime's identity and whether AI pauses are ready.
## AI runtime

The standard download carries the private model runtime (specification
§14.1): `cargo xtask bundle` runs
[`tools/ai-runtime/build.py`](../tools/ai-runtime/build.py), which assembles it
from the pins in [`pins.json`](../tools/ai-runtime/pins.json) into the
build-input cache and prints its location; the bundle copies it with `ditto` to
`Contents/Resources/ai-runtime`. Every input is verified before use: the
python-build-standalone archive and each wheel by SHA-256 (wheel hashes from the
qualified checkout's `uv.lock`), the 139 LTX source files against
`ltx-source-manifest.json`, x264 by Git commit and tree, FFmpeg by archive
hash. The wheels are installed offline (`pip --no-index --no-deps
--only-binary=:all:`), then `pip`, `ensurepip`, Tcl/Tk, IDLE, tests, headers,
manual pages and console scripts are removed and all bytecode is compiled once
with checked hashes, so the runtime never writes into the signed bundle (the
worker runs `python -I -B`). Absolute `LC_RPATH` entries left by upstream
wheel builds are deleted. The cache is keyed by the pins, the builder and the
worker sources; a full assembly took 54 s (52 s of it the static GPL
`ffmpeg`/`ffprobe` build) and a cached reuse a few seconds.

**Size.** The runtime is 494 MiB on disk: Python and packages 447 MiB
(MLX and its Metal library 203 MiB, transformers 98 MiB, 94 MiB of
precompiled bytecode), `ffmpeg`/`ffprobe` 43 MiB, LTX source 3.2 MiB. The
2026-10-05 bundle is 710.1 MiB on disk and 288.5 MB as a `ditto` ZIP, against
206.0 MiB and 115.3 MB without it. That is practical for one application
download, while the 36.2 GB of weights are not: they stay a separately
accepted, resumable [model pack](MODEL_PACKS.md). Bundling also follows §27.3
("only application-signed runtime updates introduce executable code"): a
downloadable runtime pack would be new executable code outside the app's
signature. The MLX wheels are the macOS 26 builds, measured 2.2 times faster
than the macOS 15 builds on the reference Mac, so AI pauses require macOS 26
while the app keeps its macOS 15 floor
([decision](DEPENDENCIES.md#private-ai-runtime)).

**Signing.** Inside out with the hardened runtime, after the FFmpeg libraries:
59 libraries and extension modules (`.so`, `.dylib`, including Pillow's and
MLX's bundled libraries) without an identifier, then the three executables
`python3.12`, `ffmpeg` and `ffprobe` as `dev.deadpan.Deadpan.ai.<name>`. No
entitlements: under the ad hoc hardened runtime, Python loaded every
extension module, MLX compiled and ran its Metal kernels and the worker ran a
full generation (below). MLX's Metal kernels go through the system Metal
compiler, not CPU JIT memory, so neither `allow-jit` nor
`allow-unsigned-executable-memory` was needed. Under a Developer ID every
nested file is signed by the same team, so library validation should hold;
this has not run. The audit treats any `MH_EXECUTE` image as its own process
root (`@executable_path` is its folder), so Python's
`@executable_path/../lib` search path is checked like the app's.

**Checks before publication.** The staged bundle's Python, run with a cleared
environment, must resolve its own `sys.prefix`, import MLX, NumPy, Pillow,
`mlx_lm`, safetensors and the LTX pipeline, and compute on the GPU; the staged
`ffmpeg` must list `libx264rgb`.

**GPL programs.** `ffmpeg` and `ffprobe` in `ai-runtime/bin` are a separate
GPL-2.0-or-later build (FFmpeg 8.0.3 with static x264 r3222) that the worker
runs for the model's CRF-33 H.264 conditioning and lossless `libx264rgb`
intermediates ([why](DEPENDENCIES.md#private-ai-runtime)). They are separate
executables, never linked into Deadpan, and distinct from the LGPL libraries in
`Contents/Frameworks`. `--identity` builds refuse to include them unless the
owner passes `--allow-gpl-ai-codec`; `build-provenance.json` records
`gpl_distribution_approved`. Before public distribution the owner must decide
this, and how the corresponding source is offered.

**Notices.** `Notices/AI_RUNTIME_NOTICES.txt` lists every component with
version, SPDX expression (validated like the crates'), source URL and hash, the
GPL statement with exact source and configuration, and every modified file.
`Notices/ai-runtime/` holds CPython's `LICENSE.txt`, each wheel's shipped
license files, ltx-2-mlx's `LICENSE` and the x264/FFmpeg GPL texts. The SBOM
adds the runtime components and the shipped executables' hashes.

## Downloader baseline

The build copies the helpers only after the release `deadpan-cli downloader
status --root ROOT` verifies them against the compiled pins, and rechecks the
copies.

- **Deno** keeps Deno Land's own Developer ID signature (team `2H4KBF436B`),
  hardened runtime and entitlements, so its bytes still match the compiled pin
  `b73737579d5a84c160e3316487594783fa5c15f4e13252a6a07050b755317f1a`.
- **yt-dlp**: upstream is only ad hoc signed, which notarization does not
  accept. The build removes that signature and re-signs it with the hardened
  runtime. Removing the old signature first gives the new one fresh, zeroed
  padding; signing in place leaves stale signature bytes after the new
  superblob. `manifest.json` records the upstream and shipped hashes.

The manifest sits beside the files it describes, so it is not a trust root.
Before every launch a bundled helper must pass these checks:

1. **Manifest binding.** The entry must name the compiled pin's version,
   executable, upstream hash and size. The file must match the manifest's exact
   size and SHA-256, with owner-only write permissions up to the `.app`
   directory. A folder above the bundle, such as `/Applications`, is
   admin-writable by design.
2. **Compiled anchor.**
   - For an `upstream` entry, the shipped bytes must equal the compiled
     upstream hash, and the pinned signer requirement must hold.
     `/usr/bin/codesign --verify --strict -R` checks `anchor apple generic and
     identifier "deno" and certificate leaf[subject.OU] = "2H4KBF436B"`.
   - For a `resigned` entry, the compiled pin must carry a
     signature-independent content hash, and the file's must match it.
     `YT_DLP.content_sha256` is
     `97335294737302995ed4dc5cd8a81c709a88ff52fe12a27cb7abab47ef5373c7`.
     That hash covers every architecture slice up to its code signature, with
     only the signature size and `__LINKEDIT` sizes zeroed
     ([`macho_content.rs`](../crates/deadpan-cli/src/youtube/macho_content.rs)).
     It therefore fixes all code and the appended PyInstaller archive. Layout
     rules refuse any data after the signature superblob or outside the
     universal file's slices.
3. **Signer.** A re-signed helper must pass `codesign --verify --strict`. When
   the running app has a Developer ID team, the check also requires `anchor
   apple generic and certificate leaf[subject.OU] = "<that team>"`. That covers
   the superblob bytes. An ad hoc development bundle has no signer identity, so
   only signature validity and the content pin apply.

Newer helpers never replace these files. [Signed updates](UPDATES.md)
install under the managed Application Support root and take precedence only
while compatible; rollback returns to the previous selection or this baseline.
## Signing

Signing proceeds inside out with `--options runtime`: FFmpeg libraries, the
re-signed yt-dlp, the four helper tools (identifiers
`dev.deadpan.Deadpan.<name>`), then the bundle, which signs `deadpan-app` and
seals every resource. Ad hoc builds use `--timestamp=none`; identity builds
use `--timestamp`.

Entitlements are minimal:

| Code | Entitlements | Reason |
| --- | --- | --- |
| App, CLI, workers, FFmpeg | None | Not sandboxed. Metal, Core Audio, Vision and outbound HTTPS need no hardened-runtime exception. whisper.cpp and wgpu compile Metal shaders through the system compiler, not JIT. All libraries carry the same signature, so library validation stays on. |
| yt-dlp | None | Measured on 2026-10-04 with the hardened runtime, ad hoc, and no entitlements: `downloader status --probe`, the scrubbed `bundle-verify` and a real YouTube metadata fetch and import all passed. Under a Developer ID, library validation may reject the Python modules that the PyInstaller one-file executable extracts and loads, because their builder signed them, not this team. Ad hoc signing cannot show this. If the first Developer ID run fails there, add `com.apple.security.cs.disable-library-validation` for yt-dlp only, with that failure as evidence. |
| Deno | Deno's own: allow-jit, allow-unsigned-executable-memory, disable-executable-page-protection, allow-dyld-environment-variables, disable-library-validation | Kept from the upstream signature. V8 needs JIT. These exceptions apply only to the Deno process, never to Deadpan. |
## Personal signing

When a feature needs a stable designated requirement (for example macOS privacy
permissions that should survive rebuilds), sign with the dotfiles local signing
identity, the same one Kestrel uses. Run `make maintain` in `~/.dotfiles` once so
`~/Library/Keychains/dotfiles-signing.keychain-db` holds the identity, then pass
its certificate ("Michael Dotfiles Local Signing") to `--identity`. The
GPL-codec refusal applies only to Developer ID identities, so a personally signed
build keeps the AI runtime's ffmpeg.

## Developer ID and notarization (not used)

Deadpan is not distributed, so this path is unused. It is kept, scripted but
never run, only in case that decision changes.

This Mac has no Developer ID identity. A distributable build requires the owner
to supply:

1. An Apple Developer Program membership and a **Developer ID Application**
   certificate with its private key in the login keychain. Confirm it with
   `security find-identity -v -p codesigning`.
2. Notary credentials stored once as a keychain profile, using an App Store
   Connect API key or an app-specific password:

   ```sh
   xcrun notarytool store-credentials deadpan-notary \
     --apple-id APPLE_ID --team-id TEAM_ID --password APP_SPECIFIC_PASSWORD
   ```

3. Then:

   ```sh
   cargo xtask bundle --output /tmp/deadpan-release \
     --identity "Developer ID Application: NAME (TEAM_ID)" \
     --notary-profile deadpan-notary
   ```

With both flags the build signs with secure timestamps and confirms a Developer
ID Application authority. It then works on the staged bundle:

1. submits `ditto -c -k --keepParent` output with `xcrun notarytool submit
   --wait`;
2. staples with `xcrun stapler staple`, then runs `stapler validate`;
3. checks `spctl --assess --type execute`.

Only after all three succeed does it publish the bundle, then write
`Deadpan.app.SHA256SUMS` over the stapled files and `Deadpan.release.json`
(`notarized`, `stapled`, signature kind, commit and change state). Any failure
removes the staging directory, so neither an unnotarized app nor the archive is
left behind. `build-provenance.json` is sealed before notarization, so it
records `"notarization": "requested"` and not the outcome. This path is scripted
but has not run. Expected
first-run questions: whether the notary service accepts Deno's third-party
Developer ID signature inside the bundle (it is hardened and timestamped), and
whether yt-dlp needs more than library validation disabled under a Developer ID.
A signed distribution container (DMG or ZIP) and its checksum publication are
not built yet.

## Notices and SBOM

`THIRD_PARTY_NOTICES.txt` is generated from `cargo metadata --filter-platform
aarch64-apple-darwin` for the five shipped packages' normal dependency closure.
Build-only and development dependencies are excluded. Because Cargo unifies
workspace features, the set can be a superset.

Each crate's license expression is normalized and validated before anything is
written. Legacy `MIT/Apache-2.0` becomes `MIT OR Apache-2.0`, and every
identifier must be in the vendored SPDX License List 3.27.0 identifier and
exception lists. A crate with only `license-file` becomes
`LicenseRef-cargo-<name>-<version>` and its file is reproduced. A crate with
neither fails the build.

Each entry lists the expression, authors and repository, and reproduces the
license files published in the crate source, up to three directories deep.
This includes vendored whisper.cpp, SQLite and Signalsmith notices and font
licenses. Identical texts are printed once. About 40 crates, mostly egui and
objc2, publish no license file. Each of those gets a `Copyright (c) the <crate>
authors (<authors>)` line, and the build fails unless one satisfiable choice in
its expression is covered by a bundled SPDX text (MIT, Apache-2.0 and Zlib are
vendored; an exception such as `LLVM-exception` would need its own text).

Non-Rust notices are vendored under [packaging/notices](../packaging/notices),
byte-for-byte with hashes and source URLs in `sources.json`. The SPDX identifier
lists are recorded as derived files. The build refuses a changed file.

FFmpeg's section states:

- LGPL-2.1-or-later and dynamic linking;
- that the libraries are built from the unmodified source, then relocated
  (install names and references rewritten to `@rpath`) and re-signed;
- that they can be replaced;
- the exact source archive URL with its SHA-256, tag and commit;
- each shipped library's hash;
- the configure line with only the build-host `--prefix` and `--sysroot` values
  replaced by placeholders.

Before public distribution, the owner must decide whether to host the exact
FFmpeg source alongside the binary or provide a written offer. The notice does
not promise one yet. yt-dlp ships its aggregated PyInstaller/Python notices.

Deno publishes only its MIT license, so Deadpan aggregates the notices for
everything statically linked into the pinned executable itself. The input is
[packaging/notices/deno-2.9.7](../packaging/notices/deno-2.9.7): `manifest.json`
plus each distinct license text once as `texts/<sha256>.txt` (553 texts,
1,687,073 bytes). The manifest records the Deno tag `v2.9.7` and commit
`0c071246a412575e07423263404a5d13e7ed6aa2`, the pinned executable SHA-256,
rusty_v8 `v150.4.0` (commit `5c15a6995c9bb4bacd3e341b59fff32c909c80bf`), and
for every file its upstream path, source URL and SHA-256. It covers:

- Deno's `LICENSE.md`, the license files beside its embedded Node and undici
  type declarations, the TypeScript compiler's leading license comment,
  and copyright/license comments from 275 runtime, extension and compiler
  source files, including the Node/Joyent/Feross notices. This source inventory
  conservatively includes notices beyond the macOS executable's linked code;
- rusty_v8, V8 (`LICENSE`, `LICENSE.fdlibm`, `LICENSE.strongtalk`,
  `LICENSE.v8` and its compiled third-party directories: glibc trigonometry,
  inspector_protocol, rapidhash, siphash, utf8-decoder and V8's own derived
  code), ICU, Abseil, the statically linked Chromium libc++/libc++abi and
  LLVM libc, FP16, fast_float, Dragonbox, Highway and simdutf, each at the
  submodule commit of the rusty_v8 tag;
- the Rust standard library at the rustc commit embedded in the executable;
- the 768 crates.io packages in the first `deno` executable tree from
  `cargo tree -p deno -p denort -p test_server
  --features deno/panic-trace -e normal --target aarch64-apple-darwin` (Deno's
  release build command, retaining its feature resolution) at that commit,
  with license files found by the same
  rules as Deadpan's crates, and SPDX 3.27.0 texts for the 89 that publish
  none. Every crate path embedded in the executable is in this set.

Components present in the sources but not linked in this configuration
(partition_alloc, libunwind, Chromium's Rust crates, test and build tools) are
listed with the reason. `tools/notices/deno_notices.py` regenerates the set
from a Deno checkout, the pinned executable and pinned partial clones.
The source-comment extraction was rerun against the exact Deno commit on
2026-10-06; every retained text matches its filename hash. Full independent
regeneration of the complete set has not been repeated in this session.
A new Deno pin needs a new set.

`cargo xtask bundle` selects `deno-<version>` by the shipped Deno version,
refuses a set whose version or executable hash differs, a changed or unlisted
text, a dangling reference or a crate without a license file whose license
lacks a bundled standard text. It renders one deterministic
`Notices/deno/THIRD_PARTY_NOTICES.txt` (about 2 MB), copies the manifest to
`Notices/deno/notices-manifest.json` and references both from the Deno entry of
`THIRD_PARTY_NOTICES.txt`. The bundle audit then re-renders the notice from the
vendored set and compares bytes.

`sbom.cdx.json` (CycloneDX 1.5) lists every crate with purl, validated SPDX
expression, Cargo.lock SHA-256 and dependency edges. It also lists:

- FFmpeg (`LGPL-2.1-or-later`) with its archive hash, sanitized configuration
  and shipped library hashes;
- both helpers with upstream and shipped hashes. The Deno helper nests the
  V8 and C/C++ sources (by commit) and the 768 crates (purl, crates.io
  SHA-256 and the declared license when it is a valid SPDX expression;
  otherwise a `deadpan:declared-license` property);
- whisper.cpp, SQLite and Signalsmith with versions read from their vendored
  sources. SQLite uses `blessing`, which is an SPDX identifier, so no
  `LicenseRef` is needed.

`Deadpan.app.SHA256SUMS` beside the bundle hashes every final file.
## Verification

`cargo xtask bundle-verify APP [--fixture VIDEO] [--keep]` copies the bundle
with `ditto` to a new temporary directory. It runs everything with `env -i` and
`PATH=/usr/bin:/bin` from an empty working directory, with an isolated `HOME`.
That `HOME` holds only a copy of the build user's verified managed helpers, if
any, so a fallback would be visible.

Positive checks:

- `codesign --verify --deep --strict`, the load-reference audit and the Deno
  notice audit;
- `deadpan-app --smoke-test`;
- `deadpan-cli doctor` and `deadpan-app --headless doctor`: the executable,
  every worker, the four loaded FFmpeg images and both downloader helpers are
  inside the copied bundle, and the downloader source is `bundled`;
- `downloader status --probe`: both bundled helpers verify, including the
  content pin and signer checks, and run under the hardened runtime;
- `project create-original` from
  `native/deadpan-source/tests/fixtures/cfr-bframes.mp4`, then `render` to an
  MP4;
- `doctor` locates the bundled AI runtime inside the copy with its
  `runtime.json` identity and, without a model pack, reports only the pack as
  missing;
- the copied runtime's Python imports MLX and the LTX pipeline and computes on
  the GPU from the scrubbed environment, and its `ffmpeg` has libx264;
- with `--ai-models-from <folder or .tar>`, the copy's own CLI imports the
  bridge pack into the isolated home (`--accept-license`), its smoke test runs
  through the bundled runtime, and `doctor` then reports AI pauses ready with
  every path inside the copy.

Negative checks run on separate copies, with the managed fallback present:

- a byte flipped in the bundled yt-dlp;
- `Contents/Resources/helpers` deleted;
- a changed byte in the AI worker, which `codesign --verify --deep --strict`
  must refuse;
- `Notices/deno/THIRD_PARTY_NOTICES.txt` deleted, which the Deno notice audit
  must refuse.

In both cases `downloader status` must refuse the bundled baseline, `--probe`
must fail with `DownloaderHelperInvalid`, and for the deleted directory
`doctor` must report the problem.

`cargo xtask bundle-audit APP` runs the load-reference and Deno notice audits
alone. The notice audit reads the bundled helper manifest and fails when there
is no vendored set for the bundled Deno version, when the set's version or
executable hash differs from the bundled Deno, when the aggregated notice is
missing or differs from a fresh rendering of the hash-checked set, when the
bundled manifest copy differs, or when `THIRD_PARTY_NOTICES.txt` does not
reference it. `bundle` runs it before publishing and `bundle-verify` runs it on
the copy. Unit tests cover:

- the `otool` text parsers, including universal files and install-name
  subtraction;
- system-prefix normalization;
- relocation and audit of a real clang-built library pair and executable,
  which are signed and run after the prefix is deleted;
- rpath escapes and external references;
- SPDX parsing and satisfiability, and SBOM shape and validity;
- the content hash, including tamper and appended data;
- bundled-helper tamper with a matching manifest, signer requirements and
  packaged-bundle detection;
- Deno notice sets: changed, missing and unlisted texts, dangling references,
  uncovered crates, a different Deno version or executable, missing or edited
  bundled notices, SBOM nesting and the vendored 2.9.7 set itself.
### 2026-10-04 result

Apple M5 Max, macOS 26.5.2, Xcode 26.6 and SDK 26.5, Rust 1.97.1, ad hoc
signature, commit `0dda7a8f` plus uncommitted changes (recorded as
`git_changes: true`).

- `bundle`:
  - The first build in the separate, remapped `target/bundle` took 2 min 40 s.
  - The audit found 0 problems across 12 Mach-O files.
  - The notices cover 247 crates, 40 of them without a license file, and every
    one validates.
  - The bundle is 206.0 MiB on disk, and its `ditto` ZIP is 115.3 MB (Deno
    77 MiB, `deadpan-app` 41 MiB, yt-dlp 35 MiB, `deadpan-cli` 29 MiB, FFmpeg
    16 MiB).
  - Path remapping cut `/Users/michael` strings in `deadpan-cli` from 387 to 1,
    the disabled `env!` AI worker default.
- `bundle-verify`: every positive check passed.
  - The probe reported yt-dlp `stable@2026.08.19`, EJS `0.8.0` and
    `deno 2.9.7`, with `matches_pins: true`.
  - The render produced a 37,565-byte MP4.
  - `doctor` reported the AI runtime disabled.
- Negative checks: all passed. A tampered yt-dlp and a deleted helpers directory
  were each refused by `downloader status`, `--probe`
  (`DownloaderHelperInvalid`) and, for the deleted directory, `doctor`. A
  verified managed copy was present in the isolated `HOME` throughout.
- yt-dlp ran without entitlements: from the same scrubbed copy, with the
  isolated `HOME`, `project create-from-url 'https://youtu.be/Z4C82eyhwgU'`
  (Blender, CC-BY) fetched metadata and created a valid 3,507-frame 1080p24
  project in 11.5 s using the bundled hardened yt-dlp and Deno.
- `open -n -W Deadpan.app --args --smoke-test` exited 0 through Launch
  Services.
- An earlier build, before the review fixes, was rejected by `spctl --assess`,
  as expected for an ad hoc signature.

### 2026-10-05 AI runtime result

Same Mac with macOS 26.5.2, ad hoc, uncommitted tree. `bundle` audited 74
Mach-O files with no problems and signed 59 runtime libraries and 3 runtime
executables without entitlements; the bundle is 710.1 MiB (288.5 MB ZIP).
`bundle-verify --ai-models-from ~/Library/Caches/Deadpan/ltx-qualification`
passed every positive and negative check, including the doctor and runtime
checks above, an offline import of the 36.2 GB bridge pack by the copy's own
CLI with the bundled smoke test in 12.4 s, and the tampered-worker refusal. A
separate scrubbed run of the copied app generated, accepted and rendered an AI
pause (76.8–98.9 s of generation depending on load)
([record](qualification/ai-runtime-2026-10-05.md)).

Not verified:

- a quarantined download, a second Mac or a clean user account;
- Developer ID signing, notarization and stapling;
- yt-dlp library validation under a Developer ID;
- offline launch, VoiceOver and a physical display;
- bit-for-bit reproducibility.

This is not the clean-machine release test in specification §26.6.

## Offline distribution

Specification §14.1 calls for a separate full offline distribution that
carries approved model packs as data and needs no external installation
command. `cargo xtask offline-dist` builds one from an existing `bundle`
output; it never rebuilds the app, so the distributed app is exactly the one
that `bundle` signed and `bundle-verify` exercised.

```sh
cargo xtask offline-dist --app /tmp/deadpan-bundle/Deadpan.app \
  --output /tmp/Deadpan-offline \
  --pack whisper-base-en [--pack ltx-2.3-q4-bridge=<models root>] \
  [--models-root <models root>]
cargo xtask offline-dist-verify /tmp/Deadpan-offline [--keep]
```

`--pack ID[=ROOT]` names an approved pack and, optionally, the models root
holding its installed copy; otherwise `--models-root`, otherwise
`~/Library/Application Support/Deadpan/Models`. Roots are only read.

Layout:

| Path | Contents |
| --- | --- |
| `Deadpan.app` | `ditto` copy of the bundle, signature intact. |
| `Packs/<pack>-<version>.tar` | The pax archive written by the copied app's own `deadpan-cli models export <pack> <tar> --root <root>` (`PackStore::export`). |
| `Licenses/<pack>-<version>.txt` | The app's `models license <pack>` output (summary, attribution, access, link and every compiled full text), plus the bundle's standard SPDX text for each SPDX license the app has no compiled text for. |
| `Deadpan.app.SHA256SUMS`, `Deadpan.sbom.cdx.json`, `Deadpan.release.json` | Copied from beside the bundle when present. |
| `distribution.json` | Schema 1, kind `deadpan-offline-distribution`: app version, commit, tracked-change flag, signature kind and AI runtime presence (from `build-provenance.json`); each pack's id, version, title, archive name, archive bytes and SHA-256, installed bytes, license file, license ids with SPDX, `acceptance_required` and `redistribution`, and whether its smoke test needs the AI runtime. |
| `README.txt` | Plain install steps: open Models (`:models`), Install from archive…, choose the tar; or `deadpan-cli models import`. |
| `SHA256SUMS` | `shasum -a 256 -c` format over `distribution.json`, `README.txt`, every archive and license file and the copied sidecars. |

Build steps and refusals:

- The app must carry the `DeadpanPackaging` marker and pass `codesign
  --verify --deep --strict`, before and after the `ditto` copy.
- Each pack's selected version must be a compiled one. A version from a
  [signed update](UPDATES.md#model-pack-updates) is refused with the
  rollback command to use, because installing it would also need its
  envelope and `models update --from`, which Install from archive… does not
  run. Apps with signed updates are asked for the selected version
  (`selected`) and export and describe exactly that version (`--version`).
- Each pack must be compiled into that app and reported installed (receipt
  matches the approved manifest, every file of exact size) by the copied
  app's `models list --root ROOT`. Otherwise the build refuses and names the
  `models install`/`import` command.
- A pack with any license whose `redistribution` is false is refused.
- A pack whose smoke test needs the AI runtime (`bridge_hold`) is refused when
  the app was built `--without-ai-runtime`, since it could not be installed.
- Repeated pack ids and an existing `--output` are refused. Assembly runs in
  the hidden sibling `.<name>.staging-<pid>`; the build runs the verifier's file
  checks on it and publishes by rename. A failure removes the staging folder
  and publishes nothing.
- The copied app's CLI runs with only `HOME`, `PATH=/usr/bin:/bin` and `TMPDIR`
  set to fresh directories.

`offline-dist-verify` checks, in order:

1. `distribution.json` parses as schema 1 with contained relative paths;
   `SHA256SUMS` matches every listed file and covers the manifest, README,
   every archive and license file; each archive's size and SHA-256 match the
   manifest; `Deadpan.app.SHA256SUMS`, when carried, matches every app file
   (internal links must resolve inside the folder).
2. `codesign --verify --deep --strict` on the app, its packaging marker, and
   its provenance version and commit against the manifest and AI runtime
   presence.
3. For each pack, the distribution's own
   `Deadpan.app/Contents/MacOS/deadpan-cli models import <pack> <tar> --root
   <fresh root> --accept-license`, with `env_clear()` and only a fresh `HOME`,
   `PATH=/usr/bin:/bin` and a fresh `TMPDIR`. Import never downloads; its
   smoke test runs locally (whisper in-process, the bridge pack through the
   bundled runtime). `models list` must then report the version installed
   inside the fresh root. A pack that needs an absent AI runtime is reported
   as skipped (the build already refuses that combination).

`--accept-license` in the verifier stands for the owner's prior acceptance
of the pack licenses; it installs only into a throwaway root that is deleted
afterwards (`--keep` retains it). Real users accept in the Models panel or
with `models import` after `models license`. The verifier prints a JSON
report with each pack's installed directory and import time.

Unit tests (`cargo test -p xtask offline`) cover the manifest round trip and
rejection of other schemas, escaping paths, mismatched archive names and
uppercase hashes; checksum parsing; and refusal of a tampered archive, a
missing license file, checksums that omit an archive, an edited manifest, a
link escaping the folder, an existing output and repeated pack ids.

### 2026-10-05 offline distribution result

Apple M5 Max, macOS 26.5.2. The app was the existing ad hoc 2026-10-05 bundle
with the AI runtime (commit `131bd088` plus uncommitted changes, 710 MiB).
The xtask ran from `c3479acc` plus uncommitted changes. whisper-base-en
version 2 was imported into a scratch models root first; the owner's models
folder holds only version 1 and was not used.

- `offline-dist ... --pack whisper-base-en=<scratch root>` took 9.1 s: the
  export took 0.8 s for a 148,853,760-byte archive (SHA-256
  `28f15a7a…3636db`) of the 148,849,309-byte pack. The folder is 854 MiB.
- A second run with the same `--output` was refused without changes.
- `offline-dist-verify` passed in 11.6 s. The checksums (including all
  app files in `Deadpan.app.SHA256SUMS`) and codesign passed, and the scrubbed import with the whisper smoke test took
  6.1 s into a fresh temporary root.
- A distribution copy with one archive byte changed was refused (`SHA256SUMS:
  Packs/whisper-base-en-2.tar does not match its SHA-256`). A bridge pack not
  installed in the named root was refused before anything was published.

Bridge pack (2026-10-06, same stale AI-runtime bundle): the 36.15 GB
`ltx-2.3-q4-bridge` pack was imported by clone from the qualification cache
into a scratch root (14.7 s including the bundled runtime's smoke test; the
cache was only read). `offline-dist --pack ltx-2.3-q4-bridge=<root>` wrote a
36,152,920,576-byte archive (SHA-256 `2f672a6f…5d721`) in 787 s, dominated by
the debug xtask's hashing. `offline-dist-verify` passed in 412 s: checksums
and codesign, then the scrubbed import with the bundled AI runtime's smoke
test in 22.4 s. The 72 GB of scratch output was deleted afterwards.

Not verified:

- a second Mac, a clean account or a quarantined copy of the folder;
- installing through the Models panel's Install from archive… from the
  distribution (the panel uses the same `install_pack`).

The pack license files reproduce the standard MIT text with its
`<year> <copyright holders>` placeholders and name the holders in the
attribution line. The exact upstream copyright notices of OpenAI Whisper and
Silero VAD are not compiled into Deadpan yet. Add them before redistributing
the folder to anyone else.

To verify (owner):

1. Bridge pack from a current AI-runtime bundle (verified on 2026-10-06 with
   the older bundle above): install it into the default root by clone, then
   build and verify. This needs about 2 × 36 GB free for the archive and the
   verifier's import.

   ```sh
   deadpan-cli models import ltx-2.3-q4-bridge ~/Library/Caches/Deadpan/ltx-qualification --accept-license
   cargo xtask offline-dist --app <bundle>/Deadpan.app --output <new dir> \
     --pack whisper-base-en --pack ltx-2.3-q4-bridge
   cargo xtask offline-dist-verify <new dir>
   ```

2. Second Mac: copy the folder offline (for example on an external drive, or
   download it so it is quarantined) and run `shasum -a 256 -c SHA256SUMS`
   in it. Then open `Deadpan.app` with the network off, run `:models` →
   Install from archive… → `Packs/whisper-base-en-2.tar`, and transcribe a
   project. Record the macOS version, quarantine state and timings.

## Remaining work

- Clean-machine online and offline acceptance under quarantine (§26.6),
  recorded as To verify (owner) under §29.1.
- Licensing and signing decisions for any future public distribution. Personal
  use requires neither Developer ID/notarization nor a public redistribution
  decision for the AI runtime's `ffmpeg`/`ffprobe`.
- The app uses verified manual bundle replacement with retained previous
  versions, following the [application rollback policy](UPDATES.md#application-versions-and-rollback).
  Helper and model updates have independent signed manifests and rollback.
- An FFmpeg source-hosting or written-offer decision if distribution begins.
- Aggregated Deno/V8 notices are generated from upstream sources by Deadpan,
  not published by Deno. The linked third-party set was determined from
  rusty_v8's GN configuration, V8's build files and the executable's embedded
  paths, not from Deno's own build graph; an owner or legal review of that
  completeness belongs to any future public distribution. The retained
  source-level comment inventory includes Deno's Node-derived JavaScript
  polyfills. Complete regeneration reproduced all 553 texts and the manifest
  byte for byte on 2026-10-06; the rebuilt bundle also passed its native load
  audit ([evidence](RELEASE_AUDIT.md)). Personal use does not require public
  distribution.
- Bit-for-bit reproducibility: comparing two builds, deterministic signing and
  `env!` paths.
- `tools/build-app.py` remains the quick wrapper for an existing debug
  executable. It is not relocatable.
