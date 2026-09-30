# Finished-file inspector review

Read-only source review of `encoded_render/verification/inspect.rs`, its picture/audio inspectors, and `VerificationReport::validate`. No build, test, formatter, native executable, or repository edit was performed. Line references identify the source read on 2026-09-29 and may move during integration.

## Findings

**Resolution status:** the parent reproduced the coded-padding failure on a retained real candidate, then changed the verifier to a 16x16-rounded coded-storage budget while preserving exact visible geometry. The parent also added the minimum GOP-count admission. Both fixes are resolved in source, with rebuild and retained-file verification pending at this review's close. The retained matrix includes 320x180, 318x178, and 1920x1080.

### P1: Visible pixel budget rejects legal H264 coded padding

- **Location:** `crates/deadpan-cli/src/encoded_render/verification/inspect.rs:55` and `:66`.
- `max_pixels` is exactly the visible raster product. The native source boundary checks both `context->coded_width * context->coded_height` and the allocated, uncropped frame against this budget (`native/deadpan-source/src/decoder.c:311`, `:329`). Declared cropping is applied later (`:516`).
- Progressive H264 codes macroblocks. A valid 1920x1080 output normally needs 1920x1088 coded storage; a small 62x34 output needs 64x48. These are admitted even output rasters, but this verifier rejects them before their exact visible rectangle can be checked. The existing qualification raster can conceal the failure when both axes are multiples of 16.
- **Change:** derive a checked, narrowly bounded coded-storage allowance for this progressive H264 path, such as each axis rounded upward to 16, within the unchanged native hard bounds. Retain all exact visible raster and crop checks. Do not increase or relax the source adapter's global hard limits.
- **Verification:** verify a real encoded 62x34 or 1920x1080 candidate through the production verifier, including its fresh GOP decode. Retain a case with coded storage beyond the derived allowance to prove rejection.

### P2: A report can claim too few GOPs to satisfy its own frame bound

- **Location:** `crates/deadpan-cli/src/encoded_render/verification/mod.rs:118`.
- Report admission only bounds `gops` to `1..=video_frames`. The actual inspector separately requires every GOP to contain at most `gop_frames + 1` pictures (`inspect/pictures.rs:58`, `:103`). For example, an otherwise valid 60-frame NTSC report with a 15-frame requested GOP currently admits `gops = 1`, even though one GOP cannot cover its pictures under the accepted 16-frame maximum.
- This is a contradictory untrusted-report admission, not a failure of the actual packet/picture scan. The host cannot establish full GOP structure from the count alone, but it can reject this impossible count.
- **Change:** require `gops >= video_frames.div_ceil(u64::from(native.policy().gop_frames) + 1)` in report validation.
- **Verification:** clone a valid long report, set `gops = 1`, and assert report validation and protocol completion admission fail. Keep the minimally sufficient count accepted.

## Reviewed invariants without further findings

- Absolute committed range endpoints remain in the captured contract. The verifier compares output-relative video ticks and exactly `B(end) - B(start)` audio samples; it does not round duration to derive the audio extent.
- Container validation requires the expected movie/media scales, exact durations, identity matrices, exact visible geometry, one normal-rate media edit per track, the 1024-sample AAC priming edit, and bounded frame-unit video reordering. It rejects missing or contradictory Rec.709 `nclx`, non-square `pasp`, and unexpected tracks.
- Packet scan checks the complete video PTS permutation, exact DTS/duration, sync-table/actual-IDR agreement, AAC priming and final short packet duration, and opening IDR. Decoding then checks exact presentation order, counts, endpoint, High profile, progressive I420, chroma siting, and corruption evidence.
- Actual per-frame Rec.709 limited-range admission is in native `next_i420` conversion, so the absence of a repeated Rust color comparison is not a gap.
- Fresh GOP checks use a newly allocated codec context for each seek, require the exact requested first IDR/key/I picture, compare all decoded GOP plane bytes and timestamps with continuous decode, and enforce whole-file work budgets. Actual IDR checks supplement the sync table and fresh-decode comparison.
- Both AAC modes consume the finished file through EOF. Manual mode accounts for the physical priming block, ordinary mode starts at zero, both require contiguous fixed coordinates and exact presented coverage, and the complete presented float PCM digests must agree without realignment. Native audio rejects corrupt/decode-error frames and preserves finite PCM.
- The current report validator already checks exact ordinary/manual physical counts, pinned runtime versions, reorder-edit units/bounds, and actual B-picture presence for long requested-B streams. Earlier weaker versions of those checks are no longer findings.
- These checks establish structural and decoder consistency of this emitted candidate. Fixed-coordinate fixture evidence against the authored reference still owns measured AAC event alignment and lossy picture quality; agreement between two modes of the same decoder does not independently establish that reference relationship. No publication or full-product completion is implied.

Both findings were sent to the parent as soon as established. The parent owns fixes and execution.
