# Compatible native media boundary

This developer-only harness builds signed FFmpeg 8.0.3 and runs the pinned
rsmpeg revision against the repository's original numbered-picture and audio
impulse fixtures. It does not change the app's dependencies. Gate A and export
acceptance remain open.

Prerequisites: Apple Silicon macOS, Python 3.12+, Xcode command-line tools,
`make`, `git`, `gpg`, `pkg-config`, and the repository's Rust 1.97.1 toolchain.
Python, GPG, Cargo, and `ffprobe` are developer tools, not end-user requirements.
No Homebrew FFmpeg libraries or external H.264 encoders are used by this build.

From the repository root:

```sh
python3 tools/media-qualification/compatible/build.py \
  --output /tmp/deadpan-media-compatible-build.json
python3 tools/media-qualification/compatible/qualify.py \
  --build-report /tmp/deadpan-media-compatible-build.json \
  --output /tmp/deadpan-media-compatible-qualification.json
python3 tools/media-qualification/compatible/qualify.py \
  --build-report /tmp/deadpan-media-compatible-build.json \
  --sanitizers --output /tmp/deadpan-media-compatible-sanitizers.json
```

The build creates a fresh `/tmp/deadpan-media-compatible-*` prefix, verifies the
archive, detached signature, release key bytes, and signer fingerprint against
`pins.json`, and installs only into that prefix. An optional `--download-cache`
directory may supply the three pinned download files; hashes and signature are
still verified. `--work` must name an empty directory. The default build uses
eight jobs, and the report retains configure arguments, command logs/hashes,
versions, license text, dylib hashes/linkage, and deployment load commands.
The release archive checksum is the authoritative source input; the recorded
Git tag/commit is a source reference, not a claimed archive/tree comparison.

The qualification script fetches the exact rsmpeg commit into a fresh scratch
Git repository and extracts a committed-source archive. Its sibling source
snapshot is the `../rsmpeg` dependency in the isolated probe manifest. The
checked-in `Cargo.lock` is copied as an input; every build, metadata query, and
Clippy invocation uses `--locked`. Existing upstream checkouts, untracked
additions, ignored artifacts, and root workspace dependencies are not used.

The C wrapper includes the existing original native fixture harness unchanged.
It adds the explicit MP4 `movie_timescale=240000` option to preserve rational
video ticks and audio offsets, and retains a default-timescale negative control.
The Rust probe independently reads authored picture numbers, checks exact PTS,
builds a frame/keyframe index, compares every seek with linear decode, and checks
a retained AVFrame after decoder/demuxer destruction. A swapped-picture fixture
must fail even though its timestamps are unchanged.

The process exits nonzero for unexpected failures. The known hardware CFR
B-frame mux failure, software VFR B-frame mux failure, and VFR terminal-duration
loss are explicit negative capability expectations. In the VFR case the Rust
probe itself emits its full seek evidence and exits nonzero on the authored
duration discrepancy. The orchestration succeeds only if these exact failures
remain visible and the required working configurations pass. A future fix that
changes one of these negative results intentionally requires updating the
expectation and requalifying it; success is not silently inferred.

`--sanitizers` instruments the C fixture/adapter with ASan and UBSan. It does
not rebuild FFmpeg, Rust, or Apple frameworks with sanitizer instrumentation.
Recorded timings are single-run observations on tiny warm-cache fixtures, not
performance targets or OS compatibility claims. Binary hashes identify the
measured build; fresh scratch paths and toolchain details mean byte-identical
binary reproducibility is not promised.

See [measured results and limits](../../../docs/qualification/media-compatible-2026-09-20.md).
