# Retained Signalsmith sources

Only the unmodified headers required by the portable implementation and their
complete MIT notices are retained. The include wrappers preserve upstream paths.
`pins.json` is copied from `tools/audio-qualification/pins.json`; it fixes source
repositories, revisions, versions, immutable archive URLs and archive SHA-256.
`SHA256SUMS` records each retained header and notice. Every byte was checked
against `sources.*.header_sha256` and `license_sha256` in the committed canonical
qualification report before copying. Rust tests recheck this inventory.

| Library | Version | Commit | Notice |
| --- | --- | --- | --- |
| Signalsmith Stretch | 1.3.2 | `57b93f4e9206a089a45387eaa39bdc9f310d3308` | `signalsmith-stretch/LICENSE.txt` |
| Signalsmith Linear | 0.3.1 | `5668673560146a9cfe38c25315071e3fd68c8317` | `signalsmith-linear/LICENSE.txt` |

Preserve both complete copyright and permission notices in distributed copies
and substantial portions. Stretch is Copyright (c) 2022 Geraint Luff / Signalsmith
Audio Ltd.; Linear is Copyright (c) 2025 Signalsmith Audio. These dependency
notices are independent of Deadpan's original MIT code. No model, FFmpeg,
alternate FFT library, upstream test framework, or CMake fetch is included.
