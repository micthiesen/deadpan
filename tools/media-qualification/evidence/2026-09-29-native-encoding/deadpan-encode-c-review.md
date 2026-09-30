# Native encoder C/header review

Read `native/deadpan-encode/src/encoder.c` and `encoder.h`, plus the pinned FFmpeg 8.0.3 VideoToolbox encoder, interleave muxer and fast-start relocation sources. No build, formatter or native/test execution was performed.

## Actionable integration finding

P1 for native admission: Rust `ffi.rs::Info::admit` currently requires `video_profile == 100`, while C requests `profile=high` through the VideoToolbox private option and reports `s->video->profile`. Pinned `videotoolboxenc.c:2802` stores that option in `VTEncContext.profile`; `vtenc_init` at lines 1784-1785 only reads `avctx->profile` as a fallback and never writes it back. The observed public context profile may therefore remain `AV_PROFILE_UNKNOWN` (-99), causing safe Rust open to reject an otherwise valid explicitly requested High profile. Preserve the honest queried value and distinguish it from the explicit request, or query/report the private option separately. Actual High bitstream qualification must remain in the emitted-file verifier. This was sent to the parent before native execution; the source was left frozen for the parent to fix.

## Other reviewed boundaries

No further actionable C/header defect found in this read-only pass. The implementation independently validates configuration, descriptor ownership/private mode, fixed clocks and native version/license; keeps the descriptor borrowed; bounds input, pending frames, packet sizes/counts and file extent; copies input before returning; validates consecutive input clocks; retains negative reorder/priming timestamps; rejects duplicate video presentation ordinals; records absent packet durations filled from the authored CFR contract; requires complete packet counts and encoder EOF; and allows only one same-descriptor fast-start read opening. Cancelled or failed operations poison the session, clear callback pointers before return, and never finalize from close.

The fast-start allocation bound is derived from pinned mux tables and packet count rather than measured allocation behavior. Codec controls, requested B frames/GOP/profile, successful trailer and reported delay are not emitted-file correctness evidence. Real encode/decode qualification, independent timing readers, fresh GOP decoding and sanitizer checks remain required.
