# Source audio evidence

The source manifest binds this run to its pre-commit base and exact file hashes.
`gate/` contains the required repository checks and existing Python suites.
`sanitizer/` instruments selected C adapters and C dependencies, not Rust or
FFmpeg libraries. Raw logs are deterministic gzip files.

`probe/` retains the independent pinned-FFmpeg C observation probe. Build with:

```sh
clang -I"$DEADPAN_FFMPEG_PREFIX/include" probe/audio_side_probe.c \
  -L"$DEADPAN_FFMPEG_PREFIX/lib" -Wl,-rpath,"$DEADPAN_FFMPEG_PREFIX/lib" \
  -lavformat -lavcodec -lavutil -o /tmp/deadpan-audio-side-probe
```

Pass one fixture path as its argument. Raw outputs retain the probe's literal
line escapes; normalized `.txt` files make the same observations readable.
The integrated Rust tests exercise Deadpan's actual source decoder and host cache.
See [qualification](../../../../docs/qualification/source-audio-2026-09-21.md).
