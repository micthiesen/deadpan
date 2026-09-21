# Source import timing qualification, 2026-09-21

This slice adds exact independent stream placements and pure measured import
timing/presentation candidates. It does not establish authored import, asset
qualification receipts, document basis adoption, playback or export.
[Source import timing](../SOURCE_IMPORT_TIMING.md) defines the policy and limits.

The run used Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1 and pinned LGPL
FFmpeg 8.0.3 at `/tmp/deadpan-media-compatible-xyhilms4/prefix`.
The [source manifest](../../tools/media-qualification/evidence/2026-09-21-import-timing/source-manifest.json)
records the base revision, changed implementation and exact fixture hashes.

## Actual decoded inputs

`deadpan-media/tests/source_import_timing.rs` opens actual retained fixtures
through the native video and audio sessions before deriving candidates.

| Fixture | Measured result |
| --- | --- |
| `cfr-bframes.mp4` | Picture and audio occupy 120 frames at 30000/1001. Explicit skip/discard evidence remains applied by the audio index. |
| `offset-bframes.mp4` | Common origin 2971/1500 seconds. Picture starts at 640/1001 project frames and lasts 120 frames; audio retains original samples `[95072,288288)` and lasts 120760/1001 frames. Beat occupancy is 121 frames. No inferred 1024-sample trim. |
| `vfr.mp4` | Picture lasts 238 project frames; available audio lasts 240. Beat occupancy is 240, preserving audio while holding the last selected picture. Modal measured cadence selects 30000/1001 with explicit VFR evidence. |
| `rgb25_24.mp4` | All 25 measured source frames remain selected. At a 30000/1001 project rate, exact extent is 31250/1001 frames and occupancy is 32. Original timestamps remain unchanged. |
| `rgb30_30000_1001.mp4` | Exact measured fractional cadence and 30-frame occupancy are retained. |
| `rotated90.mp4` | A 4×2 raster becomes a 2×4 presentation candidate after rotation. |
| `anamorphic.mkv` | SAR 2 maps a 4×2 raster to 8×2. The one-frame decoded duration is 41 ms, so the measured candidate is 1000/41 fps. Nominal 24 fps is not inferred. |
| `pcm-mono-44100.wav` | Original sample span `[0,44117)` remains at 44.1 kHz. On the provisional 30 fps canvas, exact extent is 44117/1470 frames and occupancy is 31. |

A headless integration test also applies the offset and VFR candidates through
ordinary core asset/insert commands, checks exact inverse restoration and JSON
round-trip, compiles their picture plans and decodes the selected first/last
frames. The offset lead holds source frame 0; the VFR audio tail holds source
frame 119. This exercises routing into actual decoding, not audio playback.

Synthetic index cases cover negative and globally translated origins,
fractional cross-clock starts, interior unavailable audio, mismatched content
or stream identities, missing terminal evidence, high-rate divisors, ambiguous
VFR cadence, histogram bounds and odd/minimum/extreme-SAR geometry. Core/plan
cases cover signed placements, inverse source anchors, marks, selected endpoints,
nested retimes/repeats, occurrence isolation, reversible edits and overflow.

## Historical project preservation

The [schema-12 fixture generator](../../tools/media-qualification/evidence/2026-09-21-import-timing/fixture/generate.py)
archives and compiles commit `35b8011e775049af41ef5e12c40fa49e62467cef`, whose
doctor reports core 7 and database 12. That binary migrates the schema-11 seed;
its own store API then writes independent picture mappings, marks and branching
history while retaining the existing audio mappings and operational records.
The fixture is dumped through SQLite backup, then reproduced byte-for-byte in a
second run. [Manifest and output](../../tools/media-qualification/evidence/2026-09-21-import-timing/fixture/manifest.json).

The result contains 34 revisions, 18 history edits, one original-media record
and one pending redo. Migration to database 13 compares every historical
document, request and forward/inverse patch through frozen core-7 meaning,
retains all operational rows and the original backup, then exercises new video
and audio placements, undo/redo and reopen. Tampered documents, commands and
patches fail without promoting the candidate or modifying the original.

Core tests reject placement vocabulary in all seven legacy document versions.
The frozen core-6 audio adapter and new core-7 audio/video adapters preserve
their exact historical vocabulary, including rejection of added null fields.

## Verification and review

The repository gate passed formatting, workspace Clippy with warnings denied,
**508 Rust tests** with zero failures or ignored tests, workspace build and doctor.
Commands and output are retained in the
[gate report](../../tools/media-qualification/evidence/2026-09-21-import-timing/gate/report.json).
Independent review dispositions are retained in
[review.json](../../tools/media-qualification/evidence/2026-09-21-import-timing/review.json).

Independent timing and migration reviews found no defects. The general review
raised a proposed source-color relabeling issue, dismissed after checking the
specification and renderer: `SdrRec709` is a project/output policy, while source
primaries and transfer remain in native/per-frame metadata. The helper changes
neither pixels nor source metadata. The existing wide-gamut working transform
uses those source tags before the explicit SDR display transform. Documentation
now makes this distinction explicit; the reviewer withdrew the finding after
checking that explanation. Source qualification receipts remain open.

While the integrated decode test was being added, a review compile caught
`E0502`: an index frame was borrowed across mutable decoding. The test now clones
that small selected-frame record first. The final 10-test import suite and
508-test workspace gate pass. The reviewer-reported excerpt and correction are
recorded in the review report; no full log of that intermediate compile survives.

No native adapter, startup, focus or control changed. Native ASan/UBSan, GUI
aesthetics, keyboard navigation and lifecycle checks were not repeated for this
pure timing/migration slice. Actual decoding runs through the existing fixture
integration tests. Earlier [native preview evidence](source-preview-2026-09-21.md)
retains its limited visual and keyboard scope. The unchanged Python qualification
harnesses were not rerun locally.

## Remaining work

Qualification receipt identity, durable source indexes, atomic authored import,
first-primary/provisional basis state, native import workflow and full format
coverage remain open. Cadence ambiguity currently fails candidate derivation.
Clean-aperture interpretation, HDR, audio scheduling/resampling and emitted-file
equivalence remain unqualified. A timing candidate is never readiness evidence.
