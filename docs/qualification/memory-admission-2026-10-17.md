# Model memory admission, 2026-10-17

This advances DP-18; it does not close the full requirement. Base revision
`88ee08e6`, Mac17,7 / Apple M5 Max, 128 GiB RAM, macOS 26.5.2,
Rust 1.97.1. Native qualification helpers use the qualified FFmpeg 8.0.3
prefix; the Python MLX adapter's configured Homebrew FFmpeg/ffprobe resolve to
9.0.1 (Cellar package `9.0.1_1`). This is a development-runtime measurement,
not a newly qualified packaged runtime.

## Admission behavior

App-owned AI generation and both transcription paths carry their selected
pack's memory estimate into the shared job coordinator. Before loading a model,
the estimate must fit in observed headroom after reserving the greater of
8 GiB or one eighth of physical RAM. This reserve is an explicit admission
policy for the OS, editor and other applications, not a measured bound on their
future allocations. Conditioning and publication do not hold the model slot.

Bounded `sysctl hw.memsize` and `vm_stat` observations run on waiting job threads,
outside the board lock and project writer. Headroom is the sum of the printed
free, speculative and inactive pages. Apple's printed free count excludes
speculative pages; adding them once matches the underlying free-plus-inactive
approximation. Compressed, purgeable and file-backed counts are not added again.
See [Apple's counter implementation](https://github.com/apple-oss-distributions/system_cmds/blob/main/vm_stat/vm_stat.c)
and [psutil's macOS available-memory calculation](https://github.com/giampaolo/psutil/blob/master/psutil/arch/osx/mem.c).
This is estimated reclaimable memory, not guaranteed allocation capacity.

Missing estimates, missing readings, malformed counters and arithmetic overflow
hold the model. Memory pressure remains an independent condition. The byte
budget cannot be bypassed by Run anyway; that action still overrides the
existing power, heat, playback and pressure conditions when the byte budget
fits. Jobs explains the held condition and retains Cancel. No memory hold
mutates the authored edit or evicts an already running model.

Observations expire after 15 seconds. Model release invalidates the old sample;
an observation epoch also rejects a probe that began before release and
completed afterward. Resampling never runs on release or the UI thread. There
is still one inference slot, including while paused or draining cancellation.
Resident model estimates are not subtracted from observed headroom again.

## Verification

The live macOS observation returned 137,438,953,472 bytes physical RAM,
86,401,826,816 bytes estimated available memory, a 17,179,869,184-byte reserve
and normal kernel pressure. The explicit opt-in observation test passed.

Initial checks caught a production dead-code error after the probe refactor;
the injected-condition setter is now compiled only for tests and UI replay.
The initial scoped gate then passed 198 tests and both strict Clippy modes.
Independent review found no parser, arithmetic, estimate-propagation or
observation-epoch defect. A subsequent specification review tightened the
byte-budget override policy to protect smaller-memory machines.

The tightened policy passed all 60 selected jobs and system-observation tests,
including a 16 GiB machine refusing the 21 GiB estimate, exact reserve
boundaries, unknown readings, overflow, stale probes, pressure overrides and
model-slot ownership. A real service test records the attempt as Queued,
commits two ordinary edits and durably cancels it while its scripted process
PID remains zero throughout. A final UI-status race correction passed three
Jobs presentation tests. Both strict normal and UI-harness Clippy modes passed
after each correction. Independent review of the final policy found no further
concrete issue. The final status message rereads the row before explaining a
refused override, so a completed job is not described as still waiting.

The first release visual replay caught a stale assertion for the earlier
refusal wording. Both jobs remained held, O was absent, Cancel and the complete
memory reasons were visibly painted. The assertion now requires the current
reason and explicit non-overridable explanation, preserving the two-held-job
checks. No production behavior or timing threshold was relaxed.

## Maximum-shape Extension measurement

A complete FromLeft Extension used the installed approved pack and current
MLX adapter: 768×320, nine context frames and 72 generated frames at 24 fps,
30 inference steps, seed 42107. The 81-frame native tensor is the largest
supported Extension shape. The supervising process sampled the owned worker's
`ri_phys_footprint` every 500 ms through the existing native process adapter.
Its report retains the same diagnostic fields as doctor; another process cannot
read these process-local counters.

The worker exited cleanly after 618.919 seconds, with one live model at most,
1,221 samples and zero sampling failures. The sampled physical-footprint peak
was **18,348,993,696 bytes (17.089 GiB)**, below the pack's 21 GiB estimate.
This includes complete inference and decoding. It is an observed sampled peak,
not an instantaneous maximum or proof of a bound across every input. Cargo
verification and compilation ran concurrently; the elapsed time is not an
isolated performance benchmark. Cache state was not controlled.

The supervisor hash-verified the native movie and provenance; its own
`media_validated` field correctly remains false. Separate retained-conditioning
and media qualification then passed in 6.861 seconds, checking the 81 native
frames and producing exactly 72 sampled output frames. The conditioning receipt
was retained after inference, with inputs compared against hashes recorded
during the run. This is not proof of pre-launch retained-input admission and
does not replace the earlier normal service/CLI qualification. No candidate was
accepted or published into an authored edit.

[Evidence](../../tools/model-qualification/evidence/2026-10-17-memory-admission/)
retains the fixture/setup scripts, host and runtime configuration, source and
executed-binary hashes, conditioning PNGs, events, reports, compressed native
and sampled movies and provenance. Only Extension is installed on this Mac;
the Bridge maximum of 97 native frames remains unmeasured here. Whisper's
manifest estimate is wired into admission, but this run does not qualify its
memory envelope.

## Final release UI and native verification

Final app SHA-256:
`5b7670c5081ea1c51c829d4d9627506dadf5039c9a54003a1fb9dc28653c64b8`.
Visual and isolated performance Jobs replays each pass 41 checks plus the
production key-routing audit. The final `jobs-125.png` was inspected: both
complete memory reasons and Cancel buttons fit, the selected row has a clear
focus outline, and no Run anyway hint or button is shown. Pressing O preserves
both held jobs and explains the refusal. Returning to battery conditions
restores the existing override workflow.

Performance mode recorded 2,273 UI frames, CPU p95 0.464 ms; 99 input-to-state
samples, p95 1.387 ms; and seven commits, maximum/p95 84.370 ms. This is the
scripted-model replay after real inference and compilation finished, not a
real-inference interference benchmark. Existing footer retry warnings and the
visual intermediate-screenshot allowance warning are retained in the reports;
no assertion failed in the final runs. Native Metal initialization on the
Apple M5 Max, window startup and the shutdown callback passed. No app bundle
was rebuilt or newly qualified for release in this slice.

## Scope retained

Pack estimates are estimates, not hard runtime allocation caps. Available
memory can change after admission. Further model/input footprint qualification,
inference/playback interference measurements, other workers' scheduling and
sleep policies, cross-process coordination and historical ETA remain open.
Lower-memory hardware execution belongs to the existing owner verification
scope; its admission behavior is covered with injected observations here.
