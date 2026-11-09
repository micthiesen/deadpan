# Performance suite, 2026-11-09

One full `cargo xtask perf --generate --ui` run on main at `1b254578f`, with
the `caminandes` and `interview` packages named as fixtures. Engineering
measurements on one machine, not release qualification.

## Environment and identity

| Item | Value |
| --- | --- |
| Hardware | Apple M5 Max, 18 cores, 128 GiB, AC power, Low Power Mode off, built-in speakers (512-frame callbacks) |
| Toolchain | rustc 1.97.1 release; FFmpeg prefix `ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3` |
| Binaries | `deadpan-cli` `4881c65beff8`, `deadpan-media-worker` `16d3d5227bfb`, `perf` `e9215266b092` |
| Load | 1-minute 1.21 at start; `--max-load 3` passed for every stage |
| Launch context | an agent shell. In this process tree a 2 ms sleep took 13 to 18 ms (p50 15.1, p95 18.0); a thread at user-interactive QoS and an `NSProcessInfo` user-interactive activity did not change that, so the throttling is imposed from outside the process |
| Output | `/tmp/deadpan-perf-20261109` (`summary.json`, per-stage JSON and logs) |

## Results

| Target | Workload | Result |
| --- | --- | --- |
| Cached ordinary edit p95 < 50 ms (headless commit + refresh) | gen-1080p60, gen-4k30 split, Repeat wrap, Undo (30 each) | PASS, p95 5.2 to 8.7 ms |
| Same | large-10000 split, wrap, Undo | PASS, p95 22.96 / 19.76 / 22.03 ms |
| Hold insertion committed < 100 ms | gen-1080p60, gen-4k30, large-10000 | PASS, p95 14.98 / 13.74 / 39.80 ms |
| Key event to command-state p95 < 8 ms | ui `rapid-input` (120) | PASS, 0.28 ms |
| Warm seek p95 < 80 ms, UI navigation to GPU completion | ui `rapid-input` (120) | PASS, 1.38 ms |
| Cached edit to visible preview p95 < 50 ms | ui `edit-latency` (40) | PASS, 8.74 ms |
| Hold fallback visible < 100 ms | ui `edit-latency` (40) | PASS, 12.86 ms |
| 10,000-beat navigation CPU p95 < 8 ms | ui `large-project` (160) | PASS, 1.00 ms |
| Warm seek p95 < 80 ms | gen-1080p60 (200) | PASS, 58.7 ms |
| Same | gen-4k30 Original (200) | FAIL, 235.6 ms (decode-bound, as on 2026-10-06) |
| Same, preview proxy | gen-4k30 (200) | PASS, 8.4 ms |
| Cold seek < 300 ms | gen-1080p60, gen-4k30 (10 each, page-cache-warm) | INFO, p95 150.9 / 277.2 ms |
| Audio: no callback underruns | every generated playback run | PASS, 0 starved, 0 faults |
| Playback sustained at source rate | gen-4k30 original, proxy, cuts, cuts+proxy | PASS |
| Same | gen-1080p60 (all five variants), gen-4k30 cuts without look-ahead | FAIL, see below |
| Export speed (informational) | gen-1080p60, gen-4k30 render, verify and publish | 17.6 and 5.1 encoded frames/s |

The two named packages were written by older development builds (database
schemas 68 and 59). Under the current development-format policy they are
refused without writes (`UnsupportedSchema`), so every stage on them reported
INFO "stage did not succeed". The generated fixtures cover the same targets.

## 1080p60 playback drops

Decode and Metal completion stayed at 5.2 ms mean per picture, yet 80 of
1,800 pictures were skipped (2026-10-06: none). An added diagnostic
(`heard_step_samples`, `heard_step_wall_ms` in `perf playback`) showed the
heard clock advancing 864 samples every 18.0 ms (p50 and p95), while device
callbacks were 512 frames every 10.67 ms. A standalone probe of the delivery
clock reproduced the 18 ms steps with 2 ms polling, and plain 2 ms sleeps took
13 to 18 ms in this shell. 864 samples exceeds a 60 fps picture's 800, so about
one picture in thirteen must be skipped whatever the decode cost; 30 fps
pictures (1,600 samples) are unaffected, which is why 4K30 passes.

So this FAIL measures the launch context, not decoding or the delivery clock.
It also exposed a product risk: nothing stopped App Nap from coalescing the
playback controller's 2 ms delivery checks and refills in the native app while
it plays behind other windows. `DeviceOutput` now holds an `NSProcessInfo`
user-interactive activity (`PlaybackActivity`) from `start_device` until
`pause_device` or drop. Its effect on an app actually under App Nap was not
measured here; that is in the owner verification list.

## Not established

- Page-cache-cold seeks (`cold-seek` needs a fresh disk image; not run).
- Real-package workloads, pending packages in the current format.
- 1080p60 playback from a foreground app context, and App Nap behavior with
  the new activity.
