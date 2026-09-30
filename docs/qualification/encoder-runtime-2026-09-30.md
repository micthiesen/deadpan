# Encoder runtime binding qualification

Fresh automatic admission now supplies a real committed-project encode through
[`QualifiedEncoder::encode`](../AUTOMATIC_ENCODER_ADMISSION.md#consuming-a-fresh-admission).
The project worker must use the same mapped helper and FFmpeg backing objects,
platform observations and frozen SDR controls that passed the probe. A changed
or missing runtime stops the attempt. Database schema 41 and historical encoded
manifests are unchanged; durable automatic decisions and public Render remain open.

## Native project run

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and pinned LGPL
FFmpeg 8.0.3. This uses the existing synthetic structural project at immutable
revision `workflow-live-restored`, document SHA-256
`564af137ff203f7a10344a9e50db7a4167cee0e1dfbbecf44ed3b39a3c6ef638`.
The package was copied with SQLite's backup API. All cells in its 19 database
tables remained unchanged after both runs.

The 320x180, 30000/1001 probe retained the typed hardware B-frame PTS-before-DTS
rejection, then qualified hardware without B-frames. Its 46 pictures and 73,674
audio samples passed full file and expected-content checks. All six signed
stereo markers landed at their exact samples; maximum Y/Cb/Cr errors were
4/7/5 code values. Both probe attempts and project encoding reported the same
five mapped objects and platform facts.

The consumed admission encoded the complete project range `[0,128)`, with
205,005 audio sample frames. The frozen controls were 1,500,000 video bits/s,
384,000 audio bits/s, a 15-frame GOP, no B-frames and a 240,000 Hz movie clock.
The normal finished-file verifier checked all pictures, fresh GOP decodes,
MP4 tables and ordinary/manual AAC presentation. Its retained 32,553-byte file
has SHA-256 `ac3d0068aa42a07a173bf84493db4c0ea4c7168dc581da6b42d240a105c1b4e9`.
The final qualification example also retained fresh full-range shared-renderer
I420 and canonical PCM references under its original 240-second deadline.

The helper backing-object SHA-256 was
`21f594f7520c9ee5de2b66acf44f25507433d8644e979448e81c1b294bca78a0`.
The reports retain each loaded UUID, device/inode, metadata, SHA-256 and
platform observation. A separate host read matched all five files to those
observations after encoding. Executables and libraries are not committed.

## Independent comparison and checks

The [retained evidence](../../tools/media-qualification/evidence/2026-09-30-encoder-runtime/README.md)
includes the exact project backup, probe and project movies, canonical references,
decoded bytes, source inventories, commands, reviews and failed runs.

Independent FFmpeg decoding passed all 128 picture clocks and all 384 complete
I420 planes against the fresh references. The largest absolute plane error was
42 code values, within the unchanged 48-code bound; every plane also passed the
unchanged mean and squared-error bounds. Ordinary FFmpeg, manual FFmpeg and
AVFoundation each passed the complete 205,005-sample authored interval at observed
absolute PTS. Physical sample counts were 205,824, 206,848 and 205,005 respectively;
priming/padding was retained and accounted for. No PCM alignment or gain change
was applied. The largest audio error was 0.064974 and largest RMS error 0.000283,
below the unchanged 0.25 and 0.02 bounds.

The first comparison stopped at its existing four-second experiment limit. This
project lasts about 4.27 seconds. Reviewed scratch copies of the independent
Python oracles and AVFoundation reader extend only fixture capacity to five
seconds/240,000 samples. Repository oracles, production admission, fidelity
tolerances, exact PTS, trim and format checks remain unchanged. Original source
snapshots, exact edits/diffs and hashes are retained. Eight pure checks establish
equivalence within the old capacity, admission through five seconds, rejection
beyond it, and rejection of missing, shifted, nonfinite or discontinuous data.
The complete native rerun passed. The original capacity failure remains recorded.

- Strict workspace Clippy across all targets and formatting passed.
- The locked full workspace run passed 2,376 tests and failed 21 tests sharing
  an outdated encoder transport fixture. All independent targets and doctests
  completed because the run used `--no-fail-fast`.
- Updating that fixture from encode protocol 2 to 3 with explicit null binding
  passed all 21 on the focused rerun. Verifier protocol 1 stayed unchanged. Its
  Python syntax check passed. Final coverage is 2,397 passing tests across 171
  result groups, with no remaining failures or ignored tests. The original
  exit-101 log is preserved; unrelated passing suites were not repeated.
- Native Metal startup, window creation and the shutdown callback passed.
- Independent reviews found no remaining actionable issues, including in the
  fixture correction and the separate comparison-capacity extension.

The production/native validation source inventory was
`1e557942d104f3990ce6a9edf0c9310f4bab3fa2b4e67906321b066a7ece6cbf`.
The final inventory is
`46892abe2157dbb43b41cf84889b25cde5eb840f029293fea063216776cea20f`;
the sole difference is `tests/encoded_verification/fixture.py` in `deadpan-cli`.

## Failure boundaries

Native tests reject another vnode even when its copied Mach-O header and length
match a loaded library. They also check exact descriptor identity, stable file
cursor, closed serialized vocabulary and bounded platform values. CLI checks
cover missing runtime evidence, wrong helper bytes, library changes between
probes, invalid frozen controls and an omitted or altered completion binding.
Runtime changes invalidate otherwise eligible capability failures. An invalid
binding fails before project media preparation. Serialized facts cannot create
a fresh `QualifiedEncoder`.

Three actual tiny probes retain the precise decoder diagnostic:

| Requested raster | Rejected coded raster | Maximum pixels |
|---|---|---:|
| 14x16 | 192x96 | 256 |
| 16x16 | 192x96 | 256 |
| 64x64 | 192x96 | 4,096 |

Each had `max_dimension=8192`, stopped on an Output failure after the preceding
typed B-frame rejection, and confirmed teardown. Only the diagnostic changed.
No allocation, geometry or cropping limit was widened. A qualified distinction
between coded and display geometry remains necessary to support these rasters.

## Proof limits

The native adapter identifies mapped backing objects under trusted installed
code. It does not hash relocated resident memory or attest OS frameworks,
kernel, drivers or hardware. Private memory changes and in-place changes made
before capture while preserving a UUID remain outside that proof. All five
matching descriptors remain open through protected work, with complete hashes
and mapping/metadata checks before and after. Native filesystem and kernel calls
remain cooperative, despite bounded reads and supervisor deadlines.

This is a normal development build and one hardware/OS combination. Sanitizers,
physical listening, painted UI, optional UI replay and GUI performance were not
rerun. UI source did not change. Full mastering/effects, HDR, small-raster
support, durable automatic decisions, native/public headless Render and release
qualification remain open. No DP requirement or delivery gate is promoted.
