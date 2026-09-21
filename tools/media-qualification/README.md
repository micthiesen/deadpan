# Native media qualification harness

This is a developer diagnostic, independent of the Rust app. It generates only
original geometric video and impulse audio. It calls libavformat/libavcodec
directly for encoding, muxing, demuxing, persistent decoding, seek/flush, and AAC
decode. It does not spawn FFmpeg per frame. `ffprobe` independently inspects the
emitted files; the FFmpeg CLI is used only for its build inventory.

On the qualifying Mac, with developer installations of Python 3.12+, clang,
pkg-config, FFmpeg 9 libraries, ffmpeg, and ffprobe:

```sh
python3 tools/media-qualification/run.py --output /tmp/deadpan-media-report.json
python3 tools/media-qualification/run.py --sanitizers --output /tmp/deadpan-media-sanitizers.json
python3 tools/media-qualification/audit_upstream.py --media-report /tmp/deadpan-media-report.json --output /tmp/deadpan-media-upstream-report.json
PYTHONPYCACHEPREFIX=/tmp/deadpan-media-pycache python3 -m unittest discover -s tools/media-qualification -p test_audit.py -v
```

Reports and fixtures are separate. The report includes the scratch directory,
source and binary hashes, library paths/hashes/versions/build flags/license,
commands, exact PTS, decoded frame hashes, seeks, audio events, metadata, MP4 atom
order, observed GOP/B-frame settings, elapsed times, and failures. Scratch
directories default to `/tmp/deadpan-media-*` and remain available for inspection.
No private media is read, no system software is installed, and no app dependency is
changed. `audit_upstream.py` fetches only pinned public source revisions and
builds them in scratch. Its optional `--existing-clones` argument requires the
same exact revisions and rejects tracked modifications and untracked files.
Ignored build artifacts are reported and excluded: builds consume clean
`git archive` snapshots. The downstream probe and rsmpeg use the checked-in
Cargo locks with `--locked`; the report records the manifests, locks, Python
orchestrator, and Rust probe hashes. The three checkout regression tests require
no upstream download or dependency build.

The native suite covers 120-frame 320×180 H.264/AAC MP4s: `30000/1001` CFR,
alternating one/two/three-period VFR, and a `60060/30000`-second source start.
Each linear decoded frame's visible number must match its authored index.
Every decoded frame is sought in reverse order, plus 12 nonmonotonic/repeated
requests on the same demuxer/decoder. Each result must hash-match linear decode.
Retained frame ownership must survive decoder reuse and flush. Timed impulses
in both channels must peak at their exact authored absolute sample positions;
AAC leading priming and terminal padding are recorded separately. A real
negative-control video swaps the pixels of frames 10 and 11; ffprobe independently
confirms its 120 original PTS, then the native identity assertion must reject
frame 10 as displaying number 11. A decoder cannot pass by consistently assigning
correct timestamps to the wrong picture.

VFR indexed presentation intervals use adjacent PTS plus the final decoded
frame's duration. Raw frame durations and stream-duration disagreement remain
visible in the report; the index is not evidence those raw metadata are correct.
The decoded YUV patch test covers code values, not a complete display color path.

The suite continues after a configuration fails, writes the failure, and exits
nonzero. **The recorded 2026-09-20 native run intentionally exits 1:** the host's
hardware VideoToolbox B-frame path emits invalid DTS. Six other configurations
pass the scoped assertions, including explicit VideoToolbox OS software encoding
and explicit hardware encoding with B-frames disabled. The upstream comparison
also exits 1 because pinned rsmpeg does not build against FFmpeg 9 and Cutlass
does not preserve the offset fixture's first PTS under the tested contract.
These are qualification findings, not ignored tests or completed product gates.

[Measured report](../../docs/qualification/media-2026-09-20.md) records the
configuration decisions and remaining work. Neither these developer tools nor
the Homebrew GPL FFmpeg build is an end-user runtime or approved distribution.
