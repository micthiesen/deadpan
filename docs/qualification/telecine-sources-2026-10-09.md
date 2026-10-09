# H.264 telecine source presentation, 2026-10-09

DP-02 / DP-16, specification §§4.3, 16.3 and 22.2. This extends the
[ordinary-interlace qualification](interlaced-sources-2026-10-09.md) to explicit
two/three-field picture sequences. The broader codec/profile and creative
operation matrix remains open.

## Interpretation

The pinned FFmpeg 8.0.3 decoder supplies owned pictures, but its H.264
`pic_struct` 3/4 interlace flag depends on decoder history. A real
progressive-coded telecine fixture produced different flags with one versus
eight threads, changing source picture counts. Deadpan now reads the bounded
SPS timing declaration and picture-timing SEI itself. At most nine payload
bytes are retained, including up to 64 HRD delay bits. The declaration travels
with its packet through FFmpeg's reference-counted `COPY_OPAQUE` mechanism,
preserving association across B-frame reordering. The decoder's guessed flag
does not determine the declared field sequence.

The existing three-picture BWDIF clock queue retains measured coded PTS and
duration. Six ticks represent one original tick, so halves and thirds are
exact even for odd intervals. An ordinary field pair divides its measured
interval in two; a repeated-field picture divides it in three and repeats
the first field's already produced pixels. The final picture requires its
own positive decoded duration. Repeats never extend a container interval or
shift audio. Progressive full-frame doubling/tripling hints retain the one
measured picture interval. All repeat hints are cleared before BWDIF, whose
ordinary handling can bypass an interlaced neighbor of a repeated progressive
picture. Single coded pictures retain the spatial bob recipe.

H.264 picture-timing support reserves the field clock before the first repeat.
Field receipts use the versioned wire flag `bwdif_fields_v2`; previous
development field receipts are refused, while ordinary progressive receipt
bytes stay unchanged. Core format 48 and SQLite 76 are unchanged. Explicit
progressive-only decoding protects exported movies, proxies and generated
masters. A progressive timing SEI is valid; declared interlace and field
repeats are refused instead of being repaired by source presentation.

Different SPS timing interpretations, unsupported standalone H.264 field
pictures, unknown terminal durations and unrepresentable clocks fail explicitly.
This does not implement inverse telecine or invent optical-flow pictures.

## Fixtures and regression evidence

The [manifest](../../native/deadpan-source/tests/fixtures/manifest.json) pins
the input/output bytes and producers. `generate_telecine_fixtures.py --check`
reproduces all eight fixtures. TFF/BFF, odd-interval, B-frame and single-picture
variants change only picture-timing SEI and required timing tables over pinned
moving-field videos; every compressed slice and AAC byte is retained. A
progressive-coded control and progressive HRD control use development
FFmpeg 9.0.1/libx264, which is not linked into Deadpan.

Native tests check independently authored moving bars in every interior row,
exact repeated-field pixels, negative/odd direct-adapter clocks, measured
terminal duration and one/eight/sixteen-thread equivalence. Every backward
seek matches sequential pixels and metadata. The progressive HRD control
produces identical pixels under source and strict output decoding. Saved
receipts for TFF, BFF, progressive-coded and single-picture telecine preserve
the 60000/1001 project basis, complete video span and the Original's unchanged
audio endpoint. Variable-interval and B-frame variants have native decoding
and seek evidence; they are not additional public Render cases in this record.

Initial fixture generation assumed the previous fixture manifest key and a
48 kHz movie timescale; those assumptions were corrected against the retained
files. The progressive encoder needed explicit bitstream transfer tags. The
thread-dependent native failure led to explicit SEI ownership. A preroll-work
test also compared generic seek's extra temporal probe with indexed seek;
it now compares the same measured anchor and isolates skipping itself.
All failed logs are retained in `/tmp/deadpan-telecine-*-20261009*.log`.

Review also reproduced a truncated picture-timing payload: 28 HRD bits and
`pic_struct` occupied the entire payload, leaving no clock flags. The direct
production-reader regression failed before the guard and passes afterward,
alongside duplicate timing, invalid emulation prevention, short HRD and size
limit cases. Both runs retain instrumented binaries and reports in
`/tmp/deadpan-telecine-header-{before,after}-20261009`.

## Verification

Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1, pinned LGPL FFmpeg 8.0.3.
Implementation, review and verification are owned by the sole working agent.

- Locked workspace with `deadpan-app/ui-harness`: 5,560 passed, 12 existing
  opt-in tests ignored. Strict all-target Clippy passed with and without the
  UI harness; formatting passed.
- Direct field/SEI adapter ASan/UBSan harness passed. The separate sanitized
  source build passed all 124 tests. Native C adapters and target C dependencies
  were instrumented; Rust and the pinned FFmpeg libraries were not.
- Debug and release public Render each passed all eight cases: 155 pictures
  and eight signal-bearing audio windows, with exact endpoints and zero
  measured offset. The four new telecine cases contribute 93 pictures; the
  three 30-picture clips end at sample 24,024, and the single-picture source's
  three fields end at sample 2,402, using the common origin's ties-to-even rule.
- Both fixture generators reproduced their checked-in outputs exactly.

The workspace and first debug Render were built before the final minimum
clock-flags refusal guard. The direct before/after regression, final source
sanitizer suite and release Render include it. The guard changes only malformed
header refusal; those later runs also retain the valid HRD/telecine controls.

The self-contained bundle passed relocation, strict signature checking, native
smoke launch, real telecine `create-original` and Render, bundled AI-runtime
checks and all helper-tamper/missing-resource refusals. A separate packaged run
used a fresh home, unrelated working directory and only `/usr/bin:/bin` on
PATH. It imported TFF telecine, rendered, and independently ran `verify-export`:
30 pictures at 60000/1001, an exact 24,024-sample endpoint and zero offset. Doctor
confirmed the loaded FFmpeg libraries, including `avfilter`, came from the bundle.
This is evidence on this Mac, not a clean-machine or physical-display claim.

Consolidated logs, reports, debug/release packages and movies, the staged patch,
source hashes and executed binary hashes are retained in
`/tmp/deadpan-telecine-final-20261009`. The source sanitizer target and signed
bundle remain in `/tmp/deadpan-telecine-source-asan-20261009` and
`/tmp/deadpan-telecine-bundle-20261009/Deadpan.app`; the bundle verification log
records its separately retained relocation/tamper directory. DP-02 and DP-16
remain partial for the outstanding source policies and full operation matrix.
