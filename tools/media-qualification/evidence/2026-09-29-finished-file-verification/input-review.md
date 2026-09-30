# MP4 observation parser review

Disposition: no actionable source findings in the reviewed changes. This was a source-only review. No repository files were changed; no build, tests, native programs, formatters, or commits were run.

## Scope

Reviewed the complete `input.rs` diff, the complete new `input/inspection.rs` implementation and its tests, and the retained parser operations those additions reuse: descriptor reads/cache, atom/full-box bounds, fixed/table sizing, sample descriptions, codec envelopes, edit lists, timing/sync tables, and chunk/sample extent validation. Read the new verifier's container and packet consumers to check how observations are interpreted, without treating that evolving verifier as reviewed in full.

SHA-256 bindings at review:

```text
9ee909cc53b9575bf8b3e447b0ac26bb5f12835228865f6c1fc584418f2e4464  native/deadpan-source/src/input.rs
81017be3409e7b4551b58bcd141dcb5a27cd32013f202081baa44df6fa9621a4  native/deadpan-source/src/input/inspection.rs
8f9602e0da04d8d703dae07a52bb822402a5e08fa695e0c782d59566d3a7d11e  native/deadpan-source/src/input/inspection/tests.rs
```

## Review results

- Box and table containment: observations are collected only after the existing closed grammar admits exact version-specific box lengths and table extents. New mvhd/tkhd/mdhd field offsets fit those admitted lengths. Unsupported boxes, fragmentation, external data references, duplicate identity/tables, ambiguous descriptions, and malformed edit grammar remain rejected.
- Allocation and traversal bounds: the 16 MiB header/read cap, 100,000 atom cap, 33 track cap, aggregate one-million row/sample cap, packet-size limit, and bounded table cache remain in force. The public summaries retain only bounded track/edit observations. Packet traversal retains cursors and table pages and reads NAL lengths/headers without allocating payload-sized buffers.
- Chunk/sample mapping: traversal reuses the previously validated sample-size accessor and monotonic per-track chunk mapping. Validation requires mapping coverage to equal sample count and each chunk's full sample extent to remain inside mdat. Run cursors and sync cursors advance monotonically within the admitted tables. Compact sizes retain the checked shared accessor.
- Clock math: raw media DTS starts at zero and advances by admitted positive stts deltas. Version-1 ctts offsets retain signed interpretation; version-0 offsets retain the established supported positive range. PTS and edit subtraction use checked signed arithmetic. Movie and media time scales remain separate. Version-specific all-ones duration sentinels remain absent observations. A sole normal media edit subtracts exact media ticks; empty/multiple edits produce no synthetic presentation mapping.
- NAL observations: packet extents and length widths are checked; zero/truncated/escaping NALs, forbidden-zero-bit violations, zero types, replacement-configuration envelopes, and more than 4,096 NALs fail. Counts and type masks remain bounded. IDR flags are explicitly header observations, not claims of decodability or closed GOPs.
- I/O, cancellation, and failure: table cache misses and NAL header reads share each call's I/O allowance. Each NAL read uses the descriptor directly, checks short reads and interruption, and checks cancellation/deadline before and after reads. Table traversal checks control repeatedly. Preflight-invalid calls do not advance cursors; traversal errors poison the reader. The caller must continue supplying shrinking time from its outer deadline and an immutable snapshot, as documented.
- Import compatibility: `mp4` retains its former stream-selection rules and delegates the same grammar to `mp4_layout`. The added fields retain duration, transform, edit, color, aspect, and sample-description observations without imposing export-only interpretation on source admission. Source empty edits remain admitted. Noncanonical color/aspect/matrix values remain visible to consumers instead of being normalized.
- Consumer interpretation checked: the production container consumer compares complete identity matrices, so the rotation convenience field cannot conceal translation. It independently requires exact export clocks, dimensions, color flags, expected tracks, and simple edits. Its packet scan treats sync-table claims and IDR NAL observations separately. Fresh-decoder and full-picture validation remain separate obligations.

## Test coverage inspected

Ten tests cover both MP4 clock versions, fast-start order, media and movie clocks, edit retention, reordered PTS, priming packets, signed composition offsets, empty source edits, varying chunk runs, compact sample sizes, contradictory color/aspect values, unknown durations, noncanonical matrices, malformed table/box/edit cases, NAL truncation and poisoning, per-call controls/budgets, and the real registered fixture's complete table inventory. These tests were read but not executed by this reviewer.

## Limits of this disposition

This parser observes admitted bytes and sample-table claims. It does not decode audio/video, establish H.264 closed GOP behavior, measure AAC presentation tolerance, authenticate an immutable snapshot, or authorize publication. Those checks belong to the parent verifier and its actual native qualification. No claim is made here that those runtime checks passed.
