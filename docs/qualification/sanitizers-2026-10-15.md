# Native adapter sanitizers, 2026-10-15

Gate G sanitizer coverage beyond the source adapters, using
`cargo xtask sanitizers` ([Adversarial suite](../ADVERSARIAL.md#native-adapter-sanitizers)).
Earlier sanitizer evidence already covered `deadpan-dsp`, the media worker,
the source adapters and the native encoder (C instrumentation through
`tools/media-qualification/host/build_sanitized.py` and `chaos --sanitize`)
and nightly libFuzzer ASan for Rust parsers. This run adds the remaining
native adapters. Compact evidence (flags, every binary's SHA-256 and
instrumentation, per-run counts and harness results) is in
[sanitizers-2026-10-15.json](sanitizers-2026-10-15.json).

## Environment

- Apple M5 Max, macOS 26.5.2, SDK 26.5.
- `rustc 1.101.0-nightly (ea137335b 2026-10-05)`, LLVM 23.1.3;
  `cargo 1.101.0-nightly (f3865b2a4 2026-09-29)`; `rust-src` installed for
  `-Zbuild-std`. The product toolchain (1.97.1) is unchanged.
- Apple clang 21.0.0 (clang-2100.1.1.101) for native objects.
- Pinned LGPL FFmpeg 8.0.3 developer prefix
  (`ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3`), not instrumented.
- Source: `main` at `723cb9dd` with uncommitted concurrent work by other
  agents (tracking/render/CLI tests; the run includes their new
  `deadpan-track` `geometry` test) plus this change.

## Command

```sh
export DEADPAN_FFMPEG_PREFIX="$HOME/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix"
cargo xtask sanitizers --target-root target/agent-sanitizers \
  --output target/agent-sanitizers/runs/2026-10-15 --runs 3
```

Wall time 619 s with warm sanitized target directories; both directories
total 18 GB.

Flags: Rust `-Zsanitizer=address` or `-Zsanitizer=thread` with
`-Zbuild-std`; native target objects `-fsanitize=address,undefined
-fno-omit-frame-pointer -fno-sanitize-recover=all -mllvm
-asan-guard-against-version-mismatch=0` or `-fsanitize=thread
-fno-omit-frame-pointer`. Runtime options are listed in the
[suite](../ADVERSARIAL.md#native-adapter-sanitizers).

## Results

All positive controls behaved as required before adapter results were
trusted: ASan reported the Rust heap overflow and use-after-free and the C
heap overflow, UBSan the C signed overflow, TSan the Rust and C data races,
and both clean runs exited 0 without a report.

Every listed test, worker, device-probe and CLI binary imports
`___asan_init` or `___tsan_init`. Counts are totals over three runs of each
binary; no run failed, timed out or printed a sanitizer report.

| Entry | ASan/UBSan | TSan | Notes |
| --- | --- | --- | --- |
| `fileclone` | 12 passed | n/a | `fclonefileat` exclusive/independent clone, symlink and existing destinations |
| `filesystem` | 15 passed | n/a | `fstatfs`/`fgetattrlist` volume UUID, NSURL bookmark create/resolve/garbage |
| `sqlite-status` | 3 passed | n/a | Bundled SQLite instrumented (11 UBSan handler imports) |
| `process` | 36 passed | 36 passed | libproc group listing and rusage, spawn guard, teardown |
| `output` | 123 passed | 123 passed | Device probe 3/3 per sanitizer on the default output device, digital silence: 295–304 (ASan) and 269–287 (TSan) real CoreAudio callbacks, all probe checks passed |
| `track` | 87 passed | not run | Instrumented `deadpan-track` worker (11 UBSan imports from deadpan-source C and BLAKE3) under the real Vision tracking, face, landmark and geometry fixtures |
| `transcribe` | 12 passed, 3/3 harness | 12 passed, 3/3 harness | Harness recognized all 21 words of the speech fixture each run with whisper.cpp 1.8.3 on Metal, 16–20 s (ASan) and 74–84 s (TSan) per run |

The transcription harness exercises the worker's raw whisper.cpp abort
callback on every encode as well as model loading, the Metal backend and
decoding. A full run of an earlier iteration (`runs/full1`) gave the same
counts, and a separate ten-run TSan repetition of `transcribe` passed 40
tests and 10 harness runs.

## Findings

No defect was found in a Deadpan adapter; no adapter code changed.

Two third-party findings, each confirmed from its report and source and
handled narrowly:

1. UBSan `pointer-overflow` in vendored ggml (whisper-rs-sys 0.15.0,
   whisper.cpp 1.8.3): `ggml_graph_nbytes` computes a layout size by
   offsetting a null pointer through `incr_ptr_aligned`
   (`ggml/src/ggml.c:6898-6921`), reached from `whisper_init_state`. This is
   undefined behaviour in C but accesses no memory; it aborted every sanitized
   transcription before inference. The compiler wrapper now builds sources
   under `*/whisper.cpp/ggml/*` with `-fno-sanitize=pointer-overflow`; every
   other UBSan check and ASan still apply there. Fixing it belongs upstream;
   it is outside the adapter and the vendored crate is not modified.
2. TSan data race on one errno word, in one of three runs of the
   transcription worker tests: a libtest thread's `TempDir` cleanup wrote
   errno, and a later deadpan-jobs stderr pump thread read the same address
   through `io::Error::last_os_error`. Darwin keeps errno in the pthread
   structure at the top of each thread's stack allocation; libsystem is not
   instrumented, so reuse of a finished thread's allocation has no
   happens-before edge in TSan. errno is thread-local, so this is a false
   positive in host test code, not in the worker. It is suppressed only for
   std's `errno`/`set_errno` frames
   (`tools/sanitizer-qualification/tsan.supp`); it did not recur in the
   final run or the ten-run repetition, so the suppression's match is
   unobserved (a non-matching suppression would fail visibly).

## Build issues resolved in the harness

- Apple clang emits `__asan_version_mismatch_check_apple_clang_2100`, which
  the nightly runtime does not define; the guard is disabled and the mixed
  runtime is proven by the controls above.
- Host build scripts share the target triple on Apple Silicon, so
  `CFLAGS_aarch64_apple_darwin` instrumented BLAKE3 inside
  `deadpan-audio`'s build script, which links no runtime. Wrappers now
  instrument only units whose `OUT_DIR` is below the target triple.
- whisper-rs-sys does not rerun its CMake build when `CC` changes and
  compiles through ccache when present, so the wrapper hash names the target
  directory and `CCACHE_DISABLE=1` is set.

## CoreVideo allocation probe

`deadpan-track` copies decoded pictures into `CVPixelBufferCreate` BGRA
buffers through a raw slice. Vision and CoreVideo are not instrumented, but
`tools/sanitizer-qualification/corevideo_allocation_probe.c` (built with
`clang -fsanitize=address`) printed:

```text
64x36 r=0 stride=256 stride*h=9216 data=9216 asan_owned=1 alloc=10304 past_end_poisoned=0
320x180 r=0 stride=1280 stride*h=230400 data=230400 asan_owned=1 alloc=235584 past_end_poisoned=0
1920x1080 r=0 stride=7680 stride*h=8294400 data=8294400 asan_owned=1 alloc=8294464 past_end_poisoned=0
3840x2160 r=0 stride=15360 stride*h=33177600 data=33177600 asan_owned=1 alloc=33177664 past_end_poisoned=0
```

The buffers are ASan-owned heap chunks whose data size equals the
`bytesPerRow * height` the adapter writes. Trailing bytes of the same chunk
(64 bytes at 1080p and 4K, more at small sizes) are not poisoned, so an
overrun shorter than that slack would go unseen; longer overruns reach a
redzone.

## Not covered

- System frameworks (Vision, CoreVideo, CoreAudio, AppKit, Foundation,
  Metal and its GPU kernels) and FFmpeg are not instrumented; only accesses
  by instrumented code are checked.
- `track` has no TSan run: its threads are Vision-internal and the adapter
  calls Vision synchronously from one thread.
- Sanitizers see only exercised paths. These are the adapters' tests and
  focused harnesses, not hostile inputs; hostile media inside the Vision and
  whisper.cpp workers remains open.
- LeakSanitizer is not used (unreliable on Darwin arm64).
- `deadpan-lifecycle`'s AppKit termination hook has no test reaching its
  unsafe code.
- Physical audio devices other than the default output, and sleep/wake
  delivery by the OS, are not exercised.
