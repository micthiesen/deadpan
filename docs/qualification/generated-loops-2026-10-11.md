# Generated selection loops and Original startup, 2026-10-11

Starting playback on accepted Generated footage now prepares the missing
Original decoder before its first appearance. The previous 4K loop took
63.574 ms on that first Original request and skipped the following Generated
picture. The fixed request takes 8.731 ms. Thirty selection-loop laps present
every picture at both 1080p60 and 4K30, with zero underruns or device faults.
DP-16 remains partial.

## Workload

Both projects have two real accepted LTX Holds separated by one Original
picture. The 1080p fixture is the unchanged
[closely spaced provider fixture](generated-transitions-2026-10-11.md).
The 4K fixture starts from a verified portable copy of the
[real export fixture](real-generated-export-2026-10-11.md); production InsertPause
at Edit 46 adds 500 ms, real generation with seed 816 reaches Ready, and
`accept-hold` accepts it without changing the first accepted Hold. Generation
ran alongside a compiler while constructing the fixture; its 81.214-second
wall time is not a model performance qualification.

The new explicit `generated-loop` replay uses normal keyboard selection and
Shift+Space, zero audition context and 5% monitor gain. The half-open ranges
are Edit 60–121 at 60 fps and 30–61 at 30 fps. They contain 60/30 Generated
pictures and one Original picture. Both ends are Generated, so every seam
switches between different accepted providers. Stopped preview first shows the
included start, releasing the previous Original under the normal one-slot
stopped policy. Each run completes 30 laps: 30.5 and 31 seconds, respectively.

The authored Holds are silent; the Original picture is from the interval
between the fixture's chirps. Device delivery, picture clocks and callback
health are measured, not acoustic alignment or listening. Both fixtures retain
their normal conditioning/reconciliation rules; no artificial acceptance or
structural edit after generation is used to create the seam.

## Implementation and bounds

`ProviderAhead` shares the existing bounded admission job, horizon/loop scan,
complete media key, cancellation and foreground adoption rules between Generated
and Original preparation. The Original job runs only when playback is on
Generated and no Original is retained. It uses the ordinary receipt-verified,
progressively admitted Original reader and decodes its first needed picture.
The foreground still rechecks authority, liveness and the exact requested
picture before serving it.

The Original job occupies the missing Original slot; it cannot coexist with
a retained Original. Playback remains bounded to one Original, its one return
companion, one retained Generated decoder and one upcoming Generated admission.
Each provider scan examines at most 1,024 new frames per request, within the
captured horizon/window. Stopped requests, cancellation, changed output
generation, non-project requests and shutdown signal both admission jobs.
Teardown and adoption stay on the worker, outside the mailbox lock.

## Measured result

These are release builds on the M5 Max / 128 GiB / macOS 26.5.2 reference Mac,
Rust 1.97.1 and qualified FFmpeg 8.0.3 libraries. Each run opens a fresh private
portable copy. File caches are warm. No compiler or inference runs alongside
the measured playthroughs. The app uses the real preview worker and device
engine, proxies disabled, a requested 120 Hz replay cadence and complete
offscreen Metal UI submissions at 1280×820 points, 1× scale. Original decoding
is full resolution; the native Generated masters are 768×320.

| Run | Missing pictures | First Original worker ms | First Original request-to-composition ms | Composition p95 / max ms | Whole-replay peak MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Previous 4K30 | 1 | 63.574 | 74.328 | 13.026 / 74.328 | 2132.7 |
| Fixed 1080p60 | 0 | 2.182 | 13.591 | 12.855 / 13.936 | 894.8 |
| Fixed 4K30 | 0 | 8.731 | 16.834 | 13.019 / 28.875 | 2156.6 |

The missing old picture is Edit 46 on lap zero, immediately after Original
45. Later laps already reuse the Original correctly. The final runs retain
all 1,831/931 worker/composition timing pairs: the complete 1,830/930-picture
windows plus the first picture of the next lap. No proxy pictures, skips,
silent starvation padding, underruns or device faults occur. Sampled clock
lag is at most one frame at 1080p and zero at 4K. One Original companion job
starts in each loop; no companion swaps are needed for the repeated identical
Original ordinal. Footprint includes copying, opening and reporting, not just
decoders, and does not establish the maximum-size Generated memory envelope.
An earlier pair of fixed runs also presents every picture. The final pair
above uses the strengthened completion assertion requiring both displayed and
device laps to reach 30; both clocks do so in both runs.

The initial 1080p replay fails its cleanup assertion after successfully
completing all 30 laps: the helper demanded a newly sought stopped picture at
the cursor. Production Stop correctly retains the last submitted picture,
which can be one frame behind the heard cursor. The corrected replay checks
that retention instead. Completed GPU tickets in the initial trace independently
recover all 1,831 pictures; its first Original request was 22.485 ms. The failed
report is retained and is not counted as a successful whole replay.

## Verification and limits

Six real-media Generated tests pass, including a new test that proves the first
Original is already decoded before its request, matches an independent reader
byte-for-byte, rejects cancelled/stale admissions and releases pending work.
That test also passes separately against the 4K fixture. All 45 ordinary
worker tests, six option tests, the new independent-lap coverage regression,
strict workspace/all-target Clippy with `ui-harness`, release build and rustfmt
pass. The ordinary nonloop Generated playback control loses Edit frame 63
early in the first Hold, while both provider transitions and audio remain
correct. An old-binary control and a repeat with the same fixed binary both
present all 180 pictures. The startup timing gap is intermittent in these runs;
they do not establish that this change caused it. It remains open, and the loop
result does not qualify every ordinary playthrough. No additional agent was
used; the implementing agent reviewed the complete diff, ownership/bounds,
cancellation, keys, clocks and retained evidence.

Coverage is tracked per displayed lap independently of the heard clock; a
picture from an earlier lap cannot fill a later gap. Completion also requires
the device clock to reach the requested lap count. A full lost delivery lap
is rejected. No native window, screenshots, physical scanout, acoustic
synchronization, larger Generated masters or other hardware tiers are measured.
The whole-product objective and remaining format/effects/mastering acceptance
remain open.

The [evidence manifest](../../tools/media-qualification/evidence/2026-10-11-generated-loops/manifest.json)
retains the initial failures, complete traces, generation/acceptance receipts,
commands, tests, source and executable identities, fixture pins and resource
observations. Scratch packages and copied executables remain under
`/tmp/deadpan-generated-loops-20261011`.

Executed final app SHA-256:
`adb05f788a5b6380fa334db2578a67d50c930b55953de6dea8e93fd422ab53e9`.
