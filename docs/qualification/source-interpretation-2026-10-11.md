# Correctable source interpretation, 2026-10-11

Status: implemented and locally verified. The workspace gate, affected replay
continuation, release exports, performance replay and rebuilt bundle checks pass.
DP-02, DP-14 and DP-16 remain partial.

## Authored choice and immutable evidence

The Original pane exposes **Interpret picture…**, backed by
`:source-interpret range=full chroma=center` and the shared typed
`SetSourceInterpretation` command. Omitted parameters retain their current
values; `auto` resets one field and `reset` resets both. An unchanged command
creates no native history entry. Entry captures the source, session and revision,
including absence, so a late import or intervening edit cannot supply a target.

Core schema 49 stores the per-asset choice outside the immutable asset and
qualification receipt. One reversible transaction changes picture interpretation
without changing bytes, source identities, picture/sample clocks, geometry,
audio, the Original baseline or structural beats. Historical revisions retain
their choices. Existing destination assets keep their current interpretation
when pasting a historical slice; restoring an absent asset brings its captured
choice. Database schema 77 also includes the choice in shot-analysis and
checkpoint keys. Obsolete, unused development formats are refused before writes;
no migration is provided under the owner's development-format decision.

An absent decoded range uses full for RGB and limited for YUV. The receipt's
`range_assumed` distinguishes that heuristic from a declaration. The native
decoder checks subsequent raw declarations without modifying them. Other
missing or unsupported color metadata is still refused. Explicit chroma choices
require subsampled YUV; RGB remains full-range and the qualified HDR path remains
limited-range. Store preview and commit refuse unsupported choices atomically.
Raw I420 and ten-bit inspection refuse overrides or assumed range, preserving
the independence of emitted-file inspection.

The shared RGBA8 and RGBA64 decoders apply the choice before composition.
Companion decoders, thread changes, seek/recovery, Original/Edit playback
pictures and immutable export sessions retain it. The displayed notice belongs
to the successfully submitted picture, including while an older picture remains
visible during preparation.

## Consumers and stale results

AI conditioning retains corrected PNG pixels and an explicit interpretation in
boundary and complete-support identities. Changing a relevant source invalidates
captured inputs; an unrelated asset does not. Generated masters keep their own
interpretation. This run verifies conditioning and protocol behavior, not a new
model generation or subjective AI quality.

Proxy cache names, sidecars, partial journals and remembered failures include
the choice. Real eight- and ten-bit corrected proxies preserve source PTS and
durations, pass corrected-reference fidelity and fail automatic-reference
fidelity. Native preview drops the old proxy and Undo can reopen its matching
variant. A changed choice cancels a pending build and drops its event receiver.

Shot scans measure corrected pixels and use separate stored analyses/checkpoints.
Foreign checkpoint tails fail without erasing existing progress. Native saves
reject stale results, and Undo restores the matching analysis. Tracking and face
detection carry the choice into the real Apple Vision worker. The tracking
fixture still follows its square and stops at the measured cut; the face fixture
still finds both drawn faces. Picture scanning retains its existing RGBA8
analysis representation; the ten-bit viewer uses RGBA64, so their byte hashes
are not an equality contract.

## Pixel, history and native workflow checks

Independent retained HEVC code values verify full/limited conversion at eight
and ten bits, including threaded decode and repeated seeks. Retained AV1 planar
codes and a scalar bilinear reference verify centered chroma. A stream-copy
fixture removes only the FFV1 range tag: its automatic pixels equal the declared
limited fixture and its corrected pixels equal the declared full fixture.
The 645-byte fixture has SHA-256
`d92f1d92cbbef19048dc42105082e488469d6c58dd61848bc5290fb913582eeb`.
Its reproducible generator uses the FFmpeg 9.0.1 CLI; runtime decoding uses the
separate pinned LGPL FFmpeg 8.0.3 libraries with Opus 1.6.1 and dav1d 1.5.4.

The offscreen Metal `source-interpretation` replay passes 24 checks over 55
steps, plus the harness's separate Kestrel shortcut check. It exercises current
value inspection, correction, the visible control, partial parameter changes,
Visual selection preservation, stale entry, reset and late project opening.
The Original fingerprint before correction and after reset is
`ef8e6a12a44fc605564d8b536cba1f9e4f56ba143e547481b6effe3afc361447`.
The corrected fingerprint differs. The corrected Original capture was visually
inspected for hierarchy, wrapping, picture area and footer placement. No live
GUI/browser inspection was run.

The full `cargo xtask gate` passes 5,650 workspace tests, 1,103 UI-feature tests,
two doctests and both strict Clippy configurations. Twelve workspace tests and
two UI-feature tests remain explicitly skipped. The prior isolated normal-app
run passed 1,060 tests with two ignored. Source-adapter ASan/UBSan passes all 177 tests
in 184.03 seconds. Instrumentation covers native C adapters and target C
dependencies, not Rust or the separately built FFmpeg libraries. Its report
retains flags, runtime identity and all 21 executed test-binary hashes.

The final affected visual scenarios pass in a bounded continuation: source
interpretation (26 checks), transcript (50), Original layout (304), long
keybindings (323), and general layouts (151). These totals include each run's
separate shortcut audit. Both app lint configurations pass; seven selected
transcript/interpretation tests pass in each of the normal and harness builds.
The first selected test command used the library target and ran zero tests;
the corrected commands select the application binary. That unsuccessful
invocation remains in the evidence.

The final release performance replay passes 22 workflow checks plus the shortcut
audit, with no screenshot readback. Its app SHA-256 is
`995772f4a3e7c24c2919232b90729430169580d57438939cffff0358e6f65160`.
Across 262 UI frames, CPU time is 0.0757 ms median, 0.9310 ms p95 and 5.7998 ms
maximum. Three committed-revision waits finish in at most 7.4719 ms. The five
input-to-picture completions have a 2.3238 ms median and 90.8005 ms maximum;
the initial Original admission/display alone takes 90.1710 ms. These are the
small retained fixture's measurements on this Mac, with uncontrolled power,
thermal and OS cache state, not a large-project or physical-display claim.

## Real exports

The public Render path exports a 42-frame black-pause recipe before and after
Full/Center correction. Both movies pass all 42 picture comparisons and their
signal window at exactly zero measured sample offset. Minimum luma PSNR is
55.6236 dB for automatic interpretation and 56.4680 dB for the correction.
Audio SNR is 40.3391 dB and identical for both.

Verifying the automatic movie against the corrected revision fails all 30
source pictures, with minimum luma PSNR 27.2796 dB; the twelve authored black
pictures and audio remain correct. No threshold or timing allowance changed.
After Undo, the automatic movie passes again, and the immutable historical
corrected revision still verifies its corrected movie.

Fresh-home release and packaged probes each create four real Original projects,
dry-run and commit the typed correction, export, Undo, and verify the corrected
historical revision again. Dry-run changes nothing; commit preserves the asset
and node inventories. Each build passes 152 picture comparisons and five signal
windows at zero sample offset. Their measured results are identical:

| Original and correction | Pictures | Signal windows | Minimum luma/chroma PSNR |
| --- | ---: | ---: | --- |
| H.264 B-frames, Full/Center | 120 | 3 | 56.2071 / 54.1683 dB |
| Ten-bit SDR HEVC, Full/Center | 12 | 1 | 51.8974 / 60.3099 dB |
| Eight-bit AV1, Full/Top-left | 12 | 1 | 53.9403 / 50.8938 dB |
| PQ HEVC, automatic range/Center | 8 | 0 | 69.9663 / 69.4766 dB |

The release CLI SHA-256 is
`251761df1436d6ee424d51b2f451bf3947e443a4135418ada342bcd9d5acd544`;
the packaged CLI is
`adc0d4d75f12f5887b3639a1ba395664aab539a7f4cc0e37cacdb5b16a124a81`.
The standalone release binaries are retained under `production-release/bin` in
the scratch root. The ad-hoc signed app is retained at
`/tmp/deadpan-source-interpretation-bundle-20261011/Deadpan.app`.
`bundle-verify` passes relocated, scrubbed-environment startup, helper/runtime
checks and the deliberate tamper refusals. No clean-machine claim is made.

The final UI bundle is
`/tmp/deadpan-source-interpretation-final-bundle-20261011/Deadpan.app`, with app
SHA-256 `8b397733ea2ef3c5a9bdadd8b26750551d8faaa7c745f5eadbeb85a2ab920209`.
Its rebuilt, relocated and tamper checks pass. The CLI, media worker, tracker
and transcriber are byte-identical to the first packaged export qualification;
those passing media checks therefore retain their exact executable identities.
Both bundles are ad-hoc signed, 741.9 MiB, with 75 audited Mach-O files and
276 Rust dependencies in their notices. The source review and all follow-ups
were performed by this session without subagents.

## Retained failures and evidence

Development checks exposed stale exhaustive constructors, a too-short generated
Hold in a new test, and an invalid RGBA8-versus-RGBA64 byte comparison. They were
fixed without weakening pixel or timing assertions. One existing model-lock test
reported `Busy`; its isolated and full-library follow-ups passed. The original
logs remain retained rather than replaced. Four schema-version assertions in
existing tests still expected core 48/store 76; they now assert 49/77. The first
core/store run passed all 1,045 core tests before the storage assertion failure;
the corrected complete storage run passes 668 tests.

The source review was performed by this session without subagents, under the
owner's sole-agent instruction. Its 127 source/test hashes identify the first
release qualification; the subsequent rail-only correction is recorded below.

The complete replay run finished 74 scenarios successfully, with one failure
and two explicit-project skips. It evaluated 7,406 checks in 3,742.2 seconds
wall with two jobs. Its retained app SHA-256 is
`5cd718ce7ec43e8feb548e9136e6161817647bd5f8284fa889fa7803130c80cb`.
The skipped `ai-pause-ready` and `generated-picture` scenarios need their own
accepted-AI project fixtures. Per-frame layout retry warnings remain in the
reports; the harness's repeated-cause and ignored-retry assertions did not fail.
The debug link also reported that its large `__eh_frame` section could affect
exception-handling performance; no warning was suppressed.

That run exposed a real sidebar regression: adding the
interpretation control pushed the transcript model's install button below the
paint clip. The correction compacts the model's size/license caption and keeps
English language, local transcription and the one-time download in its tooltip.
The interpretation control also keeps current values in its tooltip and command
entry rather than adding another rail paragraph after correction. The transcript
and source-interpretation replays now require the complete install label,
clickable button and license to be visible, including with an active correction.
The first release performance follow-up caught a smaller remaining defect: the
label and license were fully visible, but the button extended 9.3125 pixels
below its clip. Removing the language from the inline caption prevents its
second line. The full-button assertion remains unchanged. The final button
occupies y=644.3125 through 672.3125 inside the rail's y=677 paint boundary.

The next visual run passes source interpretation and general layouts. It also
reaches transcript and compact catalog checks that had relied on below-fold
accessibility rectangles without scrolling. At 960×640 the sound row's click
landed in the footer, so it correctly selected no sound. The transcript's
remaining-word summary was below the rail clip. Their updated replays send
bounded wheel input through the existing Original scroller, require fully
painted text and the complete button before interaction, and keep the revision,
both clocks and retained-picture assertions. All three follow-ups pass.
The corrected Original, complete model offer and scrolled word-cut captures
were visually inspected. No production scrolling behavior was changed.

Scratch root: `/tmp/deadpan-source-interpretation-20261010`. The initial native
replay and its PNGs are under `native-replay-1`; `source-sanitizers/report.json`
records sanitizer provenance. The debug export package, both movies and five
verification reports are retained under
`/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpHIMBJc`.
The [retained evidence manifest](../../tools/media-qualification/evidence/2026-10-11-source-interpretation/manifest.json)
indexes source and binary identities, original failures, completed checks,
replay reports/captures, performance data, sanitizer provenance and emitted-file
verification. Large binaries and movies remain at the recorded local paths.
