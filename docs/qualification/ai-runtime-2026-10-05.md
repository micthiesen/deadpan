# Bundled AI runtime and model pack qualification (2026-10-05)

Apple M5 Max, 128 GB, macOS 26.5.2, Xcode 26.6, Rust 1.97.1. Ad hoc signed
bundles built by `cargo xtask bundle` from a working tree with uncommitted
changes (recorded in `build-provenance.json`). Other agents were building and
testing on the same Mac throughout; load averages are noted where they matter.

This qualifies the packaged runtime path and the model pack manager on the
build Mac. It is not the clean-machine test of §26.6, a Developer ID or
notarized run, or the §13.4 quality corpus.

## Runtime assembly

`tools/ai-runtime/build.py build` assembled the runtime from verified inputs:
python-build-standalone 20260325 CPython 3.12.13 (archive SHA-256 pinned), 36
wheels with the `uv.lock` SHA-256 of the qualified ltx-2-mlx checkout, the 139
pinned LTX source files, and a static GPL `ffmpeg`/`ffprobe` (FFmpeg 8.0.3, x264
r3222 at the commit Homebrew's x264 uses). A first assembly took 53.7 s, 52 s
of it the codec build; reassembly with a cached codec takes 4–6 s. The
build's own check imported MLX, NumPy, Pillow, transformers, `mlx_lm`,
safetensors and the LTX pipeline and computed on `Device(gpu, 0)`.

The first bundle attempt failed its audit: Pillow's `libjpeg.62.4.0.dylib`
carries an `LC_RPATH` of `/Users/runner/work/Pillow/Pillow/build/deps/darwin/lib`.
The builder now deletes absolute run paths from wheel binaries and records
them in the notices. The second attempt (with the macOS 26 MLX build) failed
because `libmlx.dylib` loads `@rpath/libjaccl.dylib` through its loader's run
path; the audit now follows dyld and retries unresolved `@rpath` references
against the run paths of the images that load the library (unit-tested with a
three-image clang chain and its negative).

## Bundle

| Measure | Result |
| --- | --- |
| Bundle on disk | 710.1 MiB (206.0 MiB without the runtime on 2026-10-04) |
| `ditto` ZIP | 288.5 MB (115.3 MB without) |
| Runtime | 494 MiB: Python 447 MiB (site-packages 408 MiB, of which MLX 203 MiB and transformers 98 MiB; 94 MiB precompiled bytecode), `ffmpeg`/`ffprobe` 43 MiB, LTX source 3.2 MiB |
| Signing | 59 libraries and 3 executables in the runtime, hardened runtime, no entitlements |
| Audit | 74 Mach-O files, no problems |
| Staged check | The staged Python resolved its own prefix, imported MLX and the LTX pipeline and computed on the GPU in a cleared environment; `ffmpeg` lists `libx264rgb` |
| Build time | 160–172 s with a release compile, 44 s with `--no-build` |

## MLX build selection

`mlx` and `mlx-metal` publish macOS 14, 15 and 26 builds. Same copied
project, 30-frame pause at frame 300, seed 1, otherwise identical bundled
runtime, launched through the development CLI with explicit `DEADPAN_BRIDGE_*`:

| MLX build | Worker launch to exit | Load average during |
| --- | --- | --- |
| `macosx_26_0` | 113.2 s | about 25 (heavy concurrent builds) |
| `macosx_15_0` | 254.6 s | about 5–7 |

The first packaged run with the macOS 15 build took 178.5 s in the worker
(backend 166.2 s). The development environment (macOS 26 build, Homebrew
FFmpeg) took 106.1 s. The runtime now pins the macOS 26 builds, exactly the
tags `uv sync` chose for the qualified environment, and refuses AI pauses on
older macOS (`minimum_macos` 26.0 in `runtime.json`). The rest of the app
keeps its macOS 15 floor; no macOS 15 machine was available to test either
build there.

Decoded RGB of the generated masters (the worker's `native_rgb` and
`candidate_rgb` hashes) was identical across the development environment,
the bundled runtime driven by the CLI and the packaged app, all with the
macOS 26 build (`2807515f…` native, `59216fe2…` sampled). The two macOS 15
runs were identical to each other (`917969f9…`) and differ from the macOS 26
output. FFV1 master files differ in container bytes between runs; their
pixels do not.

## End-to-end run through the packaged app

`e2e.sh` copied the bundle with `ditto` and ran everything with `env -i
HOME=<scratch> PATH=/usr/bin:/bin` from a scratch directory:

1. `deadpan-cli doctor`: the bundled runtime inside the copy, identity
   `ltx-mlx 0.15.8+deadpan1`, not ready, missing only "install the AI model
   pack (36.2 GB) from Models… or with `deadpan-cli models install
   ltx-2.3-q4-bridge --accept-license`".
2. `deadpan-cli models import ltx-2.3-q4-bridge
   ~/Library/Caches/Deadpan/ltx-qualification --accept-license`: 13.3 s wall
   for 31 files and 36,152,862,913 bytes (APFS clones into the scratch
   models root, every file hashed) plus the smoke test through the bundled
   Python (pinned sources, 100 imported LTX modules, Metal, 31 size-checked
   files, 14,347 tensor headers, 0.2 s). Maximum RSS 66 MB. The source cache
   was only read.
3. `doctor` again: ready, with Python, `ffmpeg` and the worker inside the copy
   and the model data in the scratch models root.
4. `project create-original` from the interview test pattern (640×360, 30 fps,
   595 frames), then `command` `insert_time` of a 30-frame Background Hold at
   frame 300.
5. `deadpan-app --headless generate-hold --seed 1`: Ready in 98.9 s wall
   (conditioning 0.2 s, workspace 0.02 s, worker 96.4 s with backend 84.0 s,
   qualification 1.5 s, publication 0.7 s). Worker peak RSS 14.2 GB. Native
   master 25 frames at 24 fps (4,910,509 bytes), sampled master 30 frames at
   30 fps (5,933,395 bytes), provenance 44,820 bytes. Load average about 4–6.
6. `deadpan-app --headless accept-hold`: one committed revision.
7. `deadpan-cli render`: published and verified a 625-frame 640×360 30 fps
   MP4 in 13.3 s. Frames 300–329 show the generated bridge filling the canvas
   between the unchanged neighbours.

A final bundle built from the finished tree (including the Models panel and
the review fixes) repeated the run at a load average of about 2–4: import
12.2 s, generation 76.8 s wall (worker 75.3 s, backend 63.8 s, peak RSS
14.3 GB), render published and verified in 10.9 s, with the same decoded
pixels (`2807515f…`, `59216fe2…`). The earlier run with the macOS 15 build
passed the same steps (import 13.4 s, generation 180.1 s, render 11 s).

## bundle-verify

`cargo xtask bundle-verify <bundle> --ai-models-from
~/Library/Caches/Deadpan/ltx-qualification` passed every check on a scrubbed
relocated copy: codesign and audit, smoke test, both doctors, helpers and
probe, `create-original` and `render`, the bundled runtime located with its
identity, the copied Python importing MLX and LTX and computing on the GPU,
the offline import with the bundled smoke test (12.4 s; 12.3 s on the final
bundle, 43 checks), doctor reporting AI
pauses ready with every path inside the copy, and the negative cases
(tampered yt-dlp, deleted helpers, and a changed byte in the AI worker that
`codesign --verify --deep --strict` refuses).

## Not covered

Developer ID signing and library validation of the nested Python code,
notarization, a quarantined download, a second Mac or clean account, macOS
15–25, an interrupted real HTTPS download of the 36 GB pack (resume is
unit-tested with an in-memory transport and was exercised live for the
whisper pack earlier), and the native Models panel on a physical display.
