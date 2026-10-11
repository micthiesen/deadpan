# Exact timing acceptance and source-format edits, 2026-10-11

DP-02 is Complete under specification §29.1. Its exact frame/sample/source-time
contract is implemented and verified across typed arithmetic, structural plans,
durable edits, actual source decoding and emitted files. This does not close
DP-07's remaining operations, DP-16's renderer/playback work, DP-17's output
qualification or DP-24's performance budgets.

## Section 4 audit

| Requirement | Implementation and evidence |
| --- | --- |
| Separate exact project, sample and source coordinates; rational rates; half-open ranges; checked arithmetic | Core `time.rs`, exact source ratios and validated ranges retain separate types and checked wide intermediates. Unit/property tests cover ties on both sides of zero, signed sample positions, duration/range overflow and invalid rates. Authored timing never accumulates floating-point durations. |
| Frame boundaries use origin-based ties-to-even B(f); arbitrary audio-only sample starts | Core `FrameRate::audio_boundary`, independent source/sound clocks and [sound events](../SOUND_EVENTS.md). The [10,000 fractional-rate edit run](aac-preroll-2026-10-08.md#10000-fractional-rate-edits) exercises all five 30000/1001 allocation residues, independent PCM expectations, durable reopen, shared audition/offline samples and a real 10,100-picture export with 34 passing audio windows. This is a timing workload on six live nodes, not 10,000-node performance evidence. |
| Presentation order, original PTS/duration and explicit terminal evidence; source selection at project-frame centers | Retained `SourceFrameIndex`/`SourceIndexSnapshot`, exact picture mappings and persistent decoder sessions. [VFR qualification](vfr-recipes-2026-10-09.md) independently authors 1001/2002/3003-tick intervals and both terminal cases; it found and fixed reverse/bleep origin and ping-pong endpoint errors. Eleven recipes pass 867 exported pictures and 28 audio windows. This audit adds combined operations over two other VFR clock patterns. |
| Proxy identity is subordinate to Original timing; stale replies cannot replace current requests | [Proxies](../PROXIES.md), separate exact/proxy presentation identities and retained original ordinal/PTS mapping. Proxy and playback-picture tests verify mapping, exact-gate exclusion and latest-request admission; playback uses the delivered audio clock. Proxy latency, decode-ahead and full playback qualification remain separate DP-16/DP-24 obligations. |
| Preserve rotation, aspect, clean aperture, negative/nonzero origins, independent audio offsets and codec delay | [Import timing](../SOURCE_IMPORT_TIMING.md), [source origins](source-origins-2026-10-01.md), [fractional apertures](fractional-aperture-2026-10-09.md), [AAC priming](aac-preroll-2026-10-08.md), [Opus](opus-sources-2026-10-09.md), [MP3](mp3-sources-2026-10-09.md) and [PCM](pcm-sources-2026-10-09.md) retain actual timestamps and measured coverage. Common-origin normalization is explicit; an audio tail never invents a video endpoint. [Source interpretation](source-interpretation-2026-10-11.md) is reversible and preserves both clocks. |
| Exact Source/Sequence/Hold/Repeat/Retime durations; total plays and gaps only between them; attachments do not add host time | Core duration/command properties, immutable plan tests, [Section 8 coverage](../SECTION8_COVERAGE.md) and current release recipes. The new matrix independently flattens a nested Retime/Repeat, its single gap and an inserted freeze; every source must emit exactly 26 frames and the independent B(26) sample endpoint. |
| Typed time input rounds once; linked selection snaps outward while retaining its exact source/sample boundary | Core/command parsers, [Original moments](../SOURCE_MOMENTS.md), [source moment qualification](source-moments-2026-09-26.md) and exact range/mapping tests. Native controls display resolved frames and keep Original and Edit coordinates separate. Audio-only roles and sample placements do not pretend to split a picture frame. |
| Deterministic sampling; no implicit optical-flow import or invented timing | Source selection is interval containment with explicit adjacent-endpoint policy. [Admission](../SOURCE_ADMISSION.md) preserves the measured clock; ambiguous cadence or absent terminal evidence receives an explained refusal instead of fabricated metadata. Creative materialized treatments retain their separate authored admission path and do not redefine the Original clock. |

## Added source-format operation matrix

The new `preview_export::recipes::formats` module uses existing retained source
fixtures and the ordinary CLI. It keeps Original [0,12), splits at 4 and 8,
stretches [4,8) to six frames, repeats it twice with one three-frame silent
Background gap, then inserts a three-frame silent freeze at Edit 22. Each case
runs once with Preserve pitch and once with Tape pitch. It does not inject a
document or bypass source/store admission.

The expected picture sequence is calculated from manually flattened operations
and the fixture generators' clocks, independently of the decoder index and
compiled plan. The VFR oracles cover repeating 1/2/3-frame durations, omitted
positions on a rational grid, millisecond-rounded omitted positions, and VP9's
alternating 60/40 ms intervals. Plan queries and decoded export provenance must
agree with these expected ordinals at all 26 output frames, including exact
fractional retime points, held pictures and both sides of each seam.

| Sources | Fixtures | Exports | Pictures |
| --- | ---: | ---: | ---: |
| H.264 CFR/B-frames, VFR, High10 full range, both field orders, telecine and fractional aperture | 7 | 14 | 364 |
| HEVC eight-bit limited and ten-bit full range | 2 | 4 | 104 |
| VP9 ten-bit MP4 and alternating-interval WebM | 2 | 4 | 104 |
| AV1 eight/ten-bit SDR, VFR MP4/WebM, film grain, PQ and HLG | 7 | 14 | 364 |
| ProRes HQ and VFR | 2 | 4 | 104 |
| **Total** | **20** | **40** | **1,040** |

All exports pass normal Render's independent finished-file verification and
`verify-export --every 1`. All **36 signal windows have zero measured offset**;
four video-only windows remain silent and make no offset claim. Minimum
luma/chroma PSNR is **41.7307 / 51.2181 dB**; minimum informative audio block
SNR is **29.5347 dB**, and maximum level difference is **0.1145 dB**. Repeated
or held identical neighboring pictures correctly have zero identity margin;
they are not reported as distinguishable images.

The reports also match the independently calculated terminal sample counts:
**41,642** at 30000/1001, **24,960** at 50 and **20,821** at 60000/1001.
Every export declares and verifies the current **2,048-sample AAC priming**.
No timing tolerance, pixel threshold, codec policy or runtime source changed.

## Verification and provenance

Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1 and qualified FFmpeg 8.0.3
with pinned Opus/dav1d. Source base `ad6709f2`; only three Rust test files differ.
The sole working agent implemented and reviewed the test, including the exact
clock oracles, fixture-generating recipes, ID construction and complete diff.

The optimized matrix passes in **85.79 s**, or **121.88 s** including its final
build. Strict CLI all-target Clippy and formatting pass. The first run stopped
before edits/export because the helper used a filename containing a dot in a
revision ID; the helper now separates the valid ID prefix from the report label.
Review also corrected the VP9 oracle's interval pattern. A later console-only
statistics command tried to compare optional silent-window level values with
numbers; all verifier reports and the already saved summary were unaffected.
All failures and final reports are retained.

| Executed artifact | SHA-256 |
| --- | --- |
| Release CLI | `8aa000be69447d6d26a68fe0f5c44d52328a8e283c8110dcbddadea1f5c4504a` |
| Release integration test | `c18e48621acb7a51c96bda1e579865473cac28d01fd12bd1e92d7edf6fe5816b` |

The [evidence directory](../../tools/media-qualification/evidence/2026-10-11-source-operations/README.md)
retains every emitted movie, full quality report and publication report, source
and fixture pins, command logs, and a hash inventory of the complete scratch
projects. The new test runs in the ordinary integration target; reproducing the
release command and retaining its packages is documented there.

This audit reconciles DP-02's old broad source/operation remainder with the
implemented exact-time contract and the accumulated independent fixtures. It
does not require every possible codec/profile/operation Cartesian product to
establish that contract, and it does not claim such coverage. New operations
must continue to prove their timing; unimplemented effects and renderer or
device work remain on their owning requirements. Native-window playback,
acoustic/physical display timing, a fresh package build and sanitizers were not
rerun for these test-only changes. Existing qualification and the explicit
owner checks retain their original scope and limitations.
