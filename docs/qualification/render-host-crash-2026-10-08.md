# Render host crash recovery, 2026-10-08

The real automatic SDR Render workflow recovered after its host was killed
during both encoding and full-file picture verification. The helper process
groups exited, authored state and history stayed unchanged, and the verified
checkpoint could be retried without encoding again.

## Method

[`qualify_render_host_crash`](../../crates/deadpan-cli/examples/qualify_render_host_crash.rs)
is an explicit opt-in qualification executable. It runs the production
`RenderWorkflow`, automatic encoder admission, encoder and verifier processes
on a current-format private project. The retained synthetic Original has 100
frames at 30000/1001 and stereo 48 kHz audio.

The `qualification-render-host-crash` feature adds a pause only after a real
worker flushes intermediate progress. The coordinator must independently
observe matching public progress and request/attempt identities. Before
SIGKILL, the harness checks that the recorded worker is live, belongs to that
host and has the expected process group and start identity. It kills only the
owned host leader, reaps signal 9, and observes the helper groups without
signalling them. Their normal control-pipe EOF ends the work.

Each case uses a byte-verified executable copy and a bounded adjacent config
bound to that initial attempt. The automatic runtime has no argument or
environment overrides. The pause and config reader are absent from ordinary
builds. The config and executable hashes must remain unchanged through the run.

## Results

| Case | Observed interruption | Recovery |
| --- | --- | --- |
| Encoding | Real progress 1/100, host exited by SIGKILL; recorded groups absent after 0.201 s | Attempt became Interrupted; no checkpoint was invented |
| Verification | Real picture progress 1/100, host exited by SIGKILL; recorded groups absent after 0.198 s | Attempt became Interrupted; retained movie/checkpoint hashes matched |
| Verification retry | One new verifier, zero new encoders | Full verification and report/movie publication passed; published bytes matched the retained checkpoint |

Before/after authored revision, history, state and redo rows are byte-identical.
The observed group-exit intervals are evidence from these two runs, not a
latency distribution or performance qualification.

Run on Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1, pinned FFmpeg 8.0.3,
debug profile. Strict example Clippy, three gate unit tests, five example tests
and both real crash cases passed. Executed binary SHA-256:
`4715b63f7461fcc55e42cc9ffd592b0b81c75e6fbf7140d473675e49f8a7a2c1`.
[Evidence](../../tools/media-qualification/evidence/2026-10-08-render-host-crash/)
retains source/fixture hashes, exact commands and exits, process observations,
full recovery/retry reports and the published MP4.

The first fixture attempt was rejected before launch because automatic admission
forbids runtime environment overrides. Two following checks exposed SQLite
integer-type errors in the harness and its tests. Both were corrected using
checked conversions. Evidence includes these failures. No production admission
rule or recovery behavior was relaxed.

## Reproduce

```sh
DEADPAN_FFMPEG_PREFIX=/path/to/pinned/prefix cargo build --locked \
  -p deadpan-cli --features qualification-render-host-crash \
  --example qualify_render_host_crash
deadpan-cli project create-original /tmp/crash-source.deadpan \
  tools/media-qualification/evidence/2026-10-08-render-host-crash/original.mp4
target/debug/examples/qualify_render_host_crash \
  /tmp/crash-source.deadpan /tmp/new-crash-evidence
```

Use a fresh package and evidence directory. The harness deliberately refuses
reused crash job identities. It requires an existing current-format project
under `/tmp`, at least eight frames, at most 4,096 authored/history rows and
16 MiB of captured authored data. It renders at most 120 frames. Worker and
host deadlines, log sizes and process observations are bounded.

## Limits

This exercises the production coordinator directly, not native window or
authenticated IPC delivery. It covers known intermediate encode/verification
windows, not every instruction or filesystem call. Process-group observation
does not contain escaped descendants. Physical power loss remains on the owner
verification list. Existing publication crash tests cover separate publication
windows; this test does not replace them.
