# FFV1 admission implementation evidence

Production file: native/deadpan-source/src/video_codec.rs.
Protocol: RFC 9043 sections 3.8.1 and 4.1-4.3, https://www.rfc-editor.org/rfc/rfc9043.html.
The module includes the required Simplified BSD notice for RFC-derived Code Components; original admission glue remains MIT. No FFmpeg implementation was copied into Rust.

Allocation sources inspected: pinned FFmpeg 8.0.3 libavcodec/ffv1_parse.c, ffv1.c, ffv1dec.c, ffv1.h. One selected software decoder, frame threading disabled.

Admitted version is 3.4, coder 0/1/2, max64KiB configuration, max16slice raster, max8quantization tables, per-table native scale<=32768 (contexts<=16384), max1Mrange binary decisions. Configuration CRC is verified. Geometry max_dimension<=8192/max_pixels<=67108864 is validated independently from container dimensions.

State/scratch estimate capped at32MiB:
32*sum(context_count) + slices*(planes*max(context_count)*(range?32:8) +72*(min(max_dimension,max_pixels)+6)+16384)+1048576.
planes=2+alpha follows pinned v3 implementation. Initial state bytes always32/context; VlcState8/context vsrange32; both full-width sample buffers coexist and need72*(width+6)/slice. Conservative fixed allowances16KiB/slice+1MiBglobal. This is not a whole-decoder/process heap bound.

Independent positive header check used public FFmpeg API only, with width8192,height2048,max_pixels16777216,thread_count1,thread_type0. /tmp/deadpan-ffv1-public-config-check.c contains the bounded harness. /tmp/deadpan-ffv1-generated-test.rs is a scratch copy of the committed test-only inverse-interval encoder, writing the header files.

Results:
- valid8x8slice header,22bytes: avcodec_open2=0; guard resource_limit.
- valid16381-context table,16x1slices,alpha,range coder,24bytes: avcodec_open2=0; guard resource_limit.
- coder0/no initial states20bytes, with initial states197bytes: avcodec_open2=0 both.
- coder1/no initial states20bytes, with initial states197bytes: avcodec_open2=0 both.
- coder2/no initial states31bytes, with initial states208bytes: avcodec_open2=0 both.

Rust tests: all7committedMKV configs pass, allprefixtruncations/onebitcorruptions rejected, CRC-repaired truncatedsyntax/extremeinputs rejected, validlargeallocationcases rejected, unsupportedversions/fields rejected, explicitstoredinitialstates and signedscalar extrema covered, compact2Mstored-state symbols hit the1Moperation ceiling. 8tests passed in0.27seconds debug. Matroska worker separately reported10tests pass including all7completefixtures.

Current no-control API cannot poll cancellation/deadline inside its bounded CRC/range loops. Caller should check shared opening deadline/cancellation immediately before and after. Root informed.
