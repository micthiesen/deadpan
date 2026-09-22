# Audio output dependency notices

This inventory records `deadpan-output`'s locked macOS normal/build dependency
closure on 2026-09-21. Both `aarch64-apple-darwin` and
`x86_64-apple-darwin` resolve the 22 external package/version pairs below.
It excludes test-only dependencies and CPAL backends for other operating systems.
CPAL is selected with `default-features = false`; rtrb uses its default `std`
feature. No optional third-party audio backend is selected for macOS.

The eight **new** packages have complete license texts and available upstream
license declarations retained below. **Reused** packages already existed in
`Cargo.lock` before this output boundary; their licenses remain part of the
workspace distribution inventory. Reuse is not an exemption from notices.
This directory does not claim to be the complete application notice bundle.

Every crate archive was checked against its locked SHA-256. The machine-readable
[manifest](licenses/manifest.json) records all 22 checksums, upstream revisions
when supplied by the archive, notice origins and hashes. The
[source audit](../../docs/qualification/audio-output-dependency-audit-2026-09-21.md)
records callback and lifecycle limits. No third-party implementation source was
changed or vendored by this notice collection.

## Resolved macOS inventory

| Package | Declared license | Scope | Notice source |
| --- | --- | --- | --- |
| `bitflags` 2.13.2 | `MIT OR Apache-2.0` | Reused workspace | [published source](https://docs.rs/crate/bitflags/2.13.2/source/) |
| `block2` 0.6.2 | `MIT` | Reused workspace | [published source](https://docs.rs/crate/block2/0.6.2/source/) |
| `coreaudio-rs` 0.14.2 | `MIT/Apache-2.0` | New | [LICENSE-APACHE](licenses/coreaudio-rs-0.14.2/LICENSE-APACHE), [LICENSE-MIT](licenses/coreaudio-rs-0.14.2/LICENSE-MIT) |
| `cpal` 0.18.2 | `Apache-2.0` | New | [LICENSE](licenses/cpal-0.18.2/LICENSE) |
| `dasp_sample` 0.11.0 | `MIT OR Apache-2.0` | New | [LICENSE-MIT](licenses/dasp_sample-0.11.0/LICENSE-MIT), [LICENSE-APACHE](licenses/dasp_sample-0.11.0/LICENSE-APACHE) |
| `dispatch2` 0.3.1 | `Zlib OR Apache-2.0 OR MIT` | Reused workspace | [published source](https://docs.rs/crate/dispatch2/0.3.1/source/) |
| `libc` 0.2.189 | `MIT OR Apache-2.0` | Reused workspace | [published source](https://docs.rs/crate/libc/0.2.189/source/) |
| `mach2` 0.6.0 | `BSD-2-Clause OR MIT OR Apache-2.0` | New | [LICENSE-APACHE](licenses/mach2-0.6.0/LICENSE-APACHE), [LICENSE-BSD](licenses/mach2-0.6.0/LICENSE-BSD), [LICENSE-MIT](licenses/mach2-0.6.0/LICENSE-MIT) |
| `objc2` 0.6.4 | `MIT` | Reused workspace | [published source](https://docs.rs/crate/objc2/0.6.4/source/) |
| `objc2-audio-toolbox` 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | New | [LICENSE.md](licenses/objc2-frameworks-0.3.2/LICENSE.md), [LICENSE-APACHE](licenses/objc2-frameworks-0.3.2/LICENSE-APACHE) |
| `objc2-core-audio` 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | New | [LICENSE.md](licenses/objc2-frameworks-0.3.2/LICENSE.md), [LICENSE-APACHE](licenses/objc2-frameworks-0.3.2/LICENSE-APACHE) |
| `objc2-core-audio-types` 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | New | [LICENSE.md](licenses/objc2-frameworks-0.3.2/LICENSE.md), [LICENSE-APACHE](licenses/objc2-frameworks-0.3.2/LICENSE-APACHE) |
| `objc2-core-foundation` 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | Reused workspace | [published source](https://docs.rs/crate/objc2-core-foundation/0.3.2/source/) |
| `objc2-encode` 4.1.0 | `MIT` | Reused workspace | [published source](https://docs.rs/crate/objc2-encode/4.1.0/source/) |
| `objc2-foundation` 0.3.2 | `MIT` | Reused workspace | [published source](https://docs.rs/crate/objc2-foundation/0.3.2/source/) |
| `proc-macro2` 1.0.107 | `MIT OR Apache-2.0` | Reused workspace; proc-macro/build | [published source](https://docs.rs/crate/proc-macro2/1.0.107/source/) |
| `quote` 1.0.47 | `MIT OR Apache-2.0` | Reused workspace; proc-macro/build | [published source](https://docs.rs/crate/quote/1.0.47/source/) |
| `rtrb` 0.4.0 | `MIT OR Apache-2.0` | New | [LICENSE-APACHE](licenses/rtrb-0.4.0/LICENSE-APACHE), [LICENSE-MIT](licenses/rtrb-0.4.0/LICENSE-MIT) |
| `syn` 3.0.6 | `MIT OR Apache-2.0` | Reused workspace; proc-macro/build | [published source](https://docs.rs/crate/syn/3.0.6/source/) |
| `thiserror` 2.0.20 | `MIT OR Apache-2.0` | Reused workspace | [published source](https://docs.rs/crate/thiserror/2.0.20/source/) |
| `thiserror-impl` 2.0.20 | `MIT OR Apache-2.0` | Reused workspace; proc-macro/build | [published source](https://docs.rs/crate/thiserror-impl/2.0.20/source/) |
| `unicode-ident` 1.0.26 | `(MIT OR Apache-2.0) AND Unicode-3.0` | Reused workspace; proc-macro/build | [published source](https://docs.rs/crate/unicode-ident/1.0.26/source/) |

The legacy `MIT/Apache-2.0` spelling is copied from coreaudio-rs's manifest;
its archive provides both license alternatives. `unicode-ident` additionally
requires `Unicode-3.0`; do not reduce its expression to MIT or Apache alone.
Proc-macro packages are listed even though their implementation executes during
compilation. The workspace's test-only `serde_json` closure is outside this
runtime/build inventory.

## New package archive checksums

| Package | SHA-256 of crates.io archive |
| --- | --- |
| `coreaudio-rs` 0.14.2 | `7d5d7dca3ebcf65a035582c9ad4385371a9d9ee6537474d2a278f4e1e475bb58` |
| `cpal` 0.18.2 | `6f02e8d0327b42d3e2e4ab2119af397344eb9fc54a34bf0ddeaa1277af8681f1` |
| `dasp_sample` 0.11.0 | `0c87e182de0887fd5361989c677c4e8f5000cd9491d6d563161a8f3a5519fc7f` |
| `mach2` 0.6.0 | `dae608c151f68243f2b000364e1f7b186d9c29845f7d2d85bd31b9ad77ad552b` |
| `objc2-audio-toolbox` 0.3.2 | `6948501a91121d6399b79abaa33a8aa4ea7857fe019f341b8c23ad6e81b79b08` |
| `objc2-core-audio` 0.3.2 | `e1eebcea8b0dbff5f7c8504f3107c68fc061a3eb44932051c8cf8a68d969c3b2` |
| `objc2-core-audio-types` 0.3.2 | `5a89f2ec274a0cf4a32642b2991e8b351a404d290da87bb6a9a9d8632490bd1c` |
| `rtrb` 0.4.0 | `9278fb35b3e730abe136e9b395b5b81b96d06b9f5478a50f0c8430a2237b22de` |

## Notice provenance

- CPAL, coreaudio-rs, mach2 and rtrb notices are exact copies from their verified
  crate archives. All license alternatives shipped at the archive root are kept.
- `dasp_sample` 0.11.0 omits license files from its archive. Its
  `.cargo_vcs_info.json` records revision
  `97c3bb9b2363c0b46ac1633858bf1054fd02a980`. Its Apache copyright/license notice
  and full MIT license were retrieved
  from [that RustAudio source revision](https://github.com/RustAudio/dasp/tree/97c3bb9b2363c0b46ac1633858bf1054fd02a980),
  including its RustAudio Developers copyright notice. The complete Apache-2.0
  terms are retained in [the shared full text](licenses/cpal-0.18.2/LICENSE).
  The archive's older
  `rustaudio/sample` repository address points to the same project history.
- The three new objc2 audio binding archives omit their root license file. All
  three record revision `7b1abfd750a2cacaea71d6a56ecfb83cb7de560b`.
  The retained [upstream declaration](licenses/objc2-frameworks-0.3.2/LICENSE.md)
  is copied exactly from [that revision](https://github.com/madsmtm/objc2/blob/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md).
  It grants the Zlib, Apache-2.0 or MIT alternatives for these bindings and
  separately discusses derivation from Apple SDKs. This inventory uses the
  Apache-2.0 alternative for the three new bindings. The adjacent full
  `LICENSE-APACHE` is the standard Apache-2.0 text copied byte-for-byte from
  CPAL's retained license, not a newly invented objc2 copyright notice.
  No separate upstream `NOTICE` file was found in the pinned tree.
- The archive links, exact notice origins and retained-file SHA-256 values are
  listed in [licenses/manifest.json](licenses/manifest.json). Keep that manifest
  alongside the notices when updating a pin.

## Apple frameworks and build boundary

The resolved bindings link the operating system's **AudioToolbox**,
**CoreAudio**, **CoreFoundation** and **Foundation** frameworks, plus the
Objective-C runtime and `System` support for blocks/dispatch. `mach2` supplies
Mach interface bindings. `objc2-core-audio-types` supplies type bindings and
contains no separate CoreAudioTypes framework link in this pin. Relevant pinned
link declarations are:

- [AudioToolbox](https://github.com/madsmtm/objc2/blob/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/framework-crates/objc2-audio-toolbox/src/generated/mod.rs#L20),
  [CoreAudio](https://github.com/madsmtm/objc2/blob/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/framework-crates/objc2-core-audio/src/generated/mod.rs#L20),
  [CoreFoundation](https://github.com/madsmtm/objc2/blob/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/framework-crates/objc2-core-foundation/src/generated/mod.rs#L20-L23),
  and [Foundation](https://github.com/madsmtm/objc2/blob/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/framework-crates/objc2-foundation/src/generated/mod.rs#L20).
- [objc2 runtime/Foundation](https://github.com/madsmtm/objc2/blob/8852b424193ca41602281b3d7540d7c8ed51e49a/crates/objc2/src/lib.rs#L227-L234),
  [block2 System](https://github.com/madsmtm/objc2/blob/b4167b582b2f75f9a1be75495c41b765344fd03c/crates/block2/src/lib.rs#L394-L410),
  and [dispatch2 System](https://github.com/madsmtm/objc2/blob/8852b424193ca41602281b3d7540d7c8ed51e49a/crates/dispatch2/src/lib.rs#L135).

Building this boundary requires the Rust toolchain and a compatible Apple SDK
and linker. It does not add a user-installed runtime, FFmpeg dependency, model
runtime, or external audio service to `deadpan-output`. The rest of Deadpan has
its own native dependencies. Other-platform packages present in the workspace
lockfile, including ALSA and Windows/Android bindings, are not selected by this
macOS closure. A different target or feature set needs a fresh inventory.

## Distribution obligations still to verify

Carry the complete applicable license and copyright notices into the shipped
application's notice bundle, including reused workspace dependencies and the
additional Unicode license. Keep upstream notices intact and record any future
changes to third-party sources. The copied objc2 declaration explicitly leaves
questions about SDK-derived bindings; the crate license identifiers alone do
not settle Apple SDK terms or release distribution policy. Use the installed
SDK under its applicable agreement and retain this upstream caveat.

The application should link to Apple's installed system frameworks rather than
copy SDK/framework binaries into its bundle. Verify the actual release Mach-O
load commands, supported macOS deployment target, CPU architecture, code signing,
notarization and clean-machine behavior. This inventory neither selects a
minimum deployment target nor proves those packaging properties. It also does
not qualify the full release audio callback contract; see the source audit.
