# Native video export inspection source review

Scope: new I420, fresh-IDR open/restart, cumulative work observations, runtime observations, and associated FFI in `native/deadpan-source/src/{lib.rs,decoder.c,decoder.h}` plus `tests/export_decode.rs`. Existing admission, frame validation, and normal source decode paths were read for compatibility. The input parser changes are outside this review. No build, test, formatting, or native execution was run.

## Finding

### Resolved P2: Include owned I420 allocation in the decode call deadline

`lib.rs:455-475`: `SourceDecoder::i420()` checks `DecodeControl` once, allocates and zero-fills the output, then passes the original full timeout into the native operation. The native deadline starts after this allocation. A short budget can already be exhausted before native work begins, yet `next_i420` can still advance the decoder and succeed under a newly started timeout. At the admitted maximum raster, the excluded allocation is roughly 96 MiB.

Reviewed the video agent's fix: `i420()` captures an entry `Instant` before preflight/allocation, subtracts elapsed time after allocating and filling the output, rejects zero remaining time with `deadline_exceeded`, and passes only the remainder to FFI. An exhausted allocation does not invoke native code. The new `i420_allocation_timeout_preserves_the_retained_picture` unit test source covers both next and copy, unchanged work counters/current picture after rejection, and a subsequent valid next picture. The source change resolves the finding. Test execution remains with the parent.

## Other source observations

No actionable finding remains open in the reviewed native video source.

- Fresh open initializes the codec without decoding, seeks to the requested key PTS, and only then enters `receive_frame`. Header admission does not use `find_stream_info`; NOPARSE/NOFILLIN remain enabled. The new first-packet check requires exact PTS, a key flag, IDR slices, and no ordinary non-IDR VCL slices.
- Restart frees the entire old `AVCodecContext`, retains admitted demux state, and allocates a new codec with the original allocation callbacks and limits. It verifies the first decoded picture is the requested exact key/I frame before retaining it for `next_*`. No old decoded reference pictures are retained in the codec.
- Per-seek frame/packet counters still reset at seek, matching the existing documented source semantics. The added work counters remain cumulative across both ordinary seeks and fresh restarts and include opening/header I/O. Overflow checks precede counter increments.
- I420 admission requires even, eight-bit, limited-range Rec.709 YUV420. The copy uses the decoded planes directly, checks positive strides and each plane's owning `AVBufferRef` extent, and performs no RGB conversion or chroma resampling. Existing frame geometry, crop, corruption, interlace, HDR, and interpretation checks remain in use.
- The new C and Rust report layouts and function signatures agree by source inspection. Rust validates enum/boolean/SAR report vocabulary. Runtime getters query actual linked library version functions after an open that checks the pinned runtime and LGPL configuration.
- Ordinary source opening uses the same decoder configuration after extracting it into `allocate_decoder`; existing RGBA and metadata APIs retain their behavior. Unsupported fresh-codec targets fail and poison the native session. Rust pre-cancellation rejects before restart or next can alter retained state.
- The added test source exercises default/current-frame behavior, exact planes across fresh GOPs, cumulative work, pre-cancellation, non-key rejection, and unqualified I420 formats. The one-frame open test also checks cumulative work, so an earlier decoded opening frame cannot be hidden by the per-seek counter reset.

These source observations do not establish runtime correctness, sanitizer safety, valid emitted-file coverage, or actual fresh-GOP equivalence. The parent owns execution and qualification.
