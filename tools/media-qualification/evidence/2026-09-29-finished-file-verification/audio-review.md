# Audio decoder review

Disposition: no actionable source findings in the reviewed changes. No repository files were changed. Build, tests, native execution, and formatting were not run for this review.

## Scope and evidence

Reviewed the complete diff and relevant surrounding implementations in:

- `native/deadpan-source/src/audio.rs`
- `native/deadpan-source/src/audio_decoder.c`
- `native/deadpan-source/src/audio_decoder.h`
- `native/deadpan-source/tests/audio_decode.rs`

SHA-256 bindings at review:

```text
082cf585372af195b72713ad8216cc40887c728306d05ff4888fcdf9e6cd20de  native/deadpan-source/src/audio.rs
946f2681d5006c326f0c2515e80f685c909c4735b55fb201f9fac5893eee4a91  native/deadpan-source/src/audio_decoder.c
bf6e4791ebbbed3b255ae3eb833d1ed0eeeca218e0f256ba2bedfd12e457eec8  native/deadpan-source/src/audio_decoder.h
6022db4d70213a2e09d56ab878f9a696db6f7c96e90750970a7ddee22f219669  native/deadpan-source/tests/audio_decode.rs
```

## Findings considered

- Manual compatibility: `open` and `open_first` explicitly select `AudioDecodeMode::Manual`; the enum default is also Manual. The native Manual branch retains `AV_CODEC_FLAG2_SKIP_MANUAL`. Existing stream and frame structures retain their fields and physical-sample behavior.
- Ordinary timing: the only native decoding-mode change clears FFmpeg's manual-skip flag before codec open. The adapter continues to report returned frame PTS, duration, samples, discard flag, and actual remaining skip side data. It does not introduce PCM alignment, packet dropping, endpoint synthesis, or tail trimming. Returned-sample budget documentation now correctly distinguishes Manual and Ordinary.
- FFI contract: the Rust and C mode/evidence fields, integer widths, array lengths, argument order, and `repr(C)` layout agree. Evidence outputs are initialized on both successful open and successful next/EOF. Strings are bounded in C and require a terminating NUL and valid UTF-8 in Rust. The native mode has a closed two-value check; Rust verifies the returned mode.
- Evidence: unknown profiles remain absent. Container and decoder profiles remain separate observations. AAC-LC validation continues in the native admission and per-frame paths, while the Rust evidence reader rejects unsupported AAC profile IDs and inconsistent profile names. Neither the mode nor reported profile claims sample-timing verification.
- Resource and lifetime behavior: descriptor ownership, callback lifetimes, runtime pinning, stream admission, packet counts/bytes, per-call I/O, allocation limits, decoded-sample bounds, poisoning, and preflight cancellation remain in the existing paths. Ordinary mode can consume skipped data inside FFmpeg, but submitted packet and per-frame progress bounds still constrain that work.
- Coverage: added tests compare default versus explicit Manual behavior, check Ordinary AAC PCM against the Manual post-priming frames, retain the fixture's observed unreported tail, inspect decoder evidence, and exercise both modes for PCM, cancellation, invalid configuration, stream selection, frame limits, and poisoned sessions. Malformed evidence receives unit coverage. Test execution remains the parent's responsibility.

## Remaining verification

Run the source crate's locked tests and the parent finished-file qualification matrix. This source review does not establish actual Ordinary-mode emitted-file timing, arbitrary-range padding behavior, sanitizer cleanliness, or decoder/container tolerance.
