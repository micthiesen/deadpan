# Continuous owner returns, 2026-10-12

Stored beat and group recipes now produce continuous reverb and delay through
the canonical authored bus. `:wet-send reverb|delay -6dB|off` captures its target,
preserves other treatments and changes no picture time. Whole-beat edits support
dot and macros; individual Repeat plays isolate their recipes. Inspector rows
show the current levels and command.

The [processing contract](../FINITE_WET_EFFECTS.md#owner-returns) places each
return after the owner's complete ordered chain and before parent processing.
An owner's return cannot feed itself. Child returns can feed an ancestor's
effect. Allocated-Tail sends remain separate and tap before the owner's own
continuous return. Both features retain exact authored allocation.

The shared graph preserves per-contribution clocks and nonlinear drive.
Consuming-sample permissions apply to complete wet history. Silent Holds suppress
Original returns while independent sounds retain their own allowances. An
explicit Tail provider, including a Repeat default gap, suppresses other
incoming ancestor Original returns and retains its selected effect and input.
The initial Tail regression failed at its first sample; its fixed test compares
both Tail effects with independently rendered selected providers.

One native workspace per effect serves nested owners within each preparation
request. Construction happens after recursive input preparation and its complete
bound is admitted against shared PCM-equivalent residency. History, output and
cached arrays have separate reservations. Existing deadline, work, frame, node
and depth limits remain active. Failures cannot publish partially prepared PCM.
This is bounded preparation, not a realtime or persistent-cache claim.

Core 51, store 79 and frozen audio context 12 retain the new owner levels.
Older unused development packages are refused without writes under the existing
owner authorization. No migration, dependency, native ABI, limiter or emitted
audio verification tolerance changed.

## Verification

All 3,950 unit, integration and documentation tests pass across audio, core,
planning, playback, storage and the app. The app retains nine existing opt-in
ignores. Ten new decoded-PCM checks cover both effects against independently
composed dry/finite-kernel output, cold partitions, parent saturation and EQ,
nested returns, split/Repeat history, silence permissions, explicit Tail Holds
and default gaps, cancellation and bounded refusal. Core checks authenticate the
new frozen recipe and preserve sends through other treatment edits.

The initial full app run found stale generated command documentation and routing
baselines. Their regeneration deliberately exits with failure for diff review;
the final run passes. Two general storage tests had redundant assertions naming
schema 78. Those constants were removed; both tests still compare the actual
persisted schema with the current version, and the dedicated migration test
asserts version 79 and rejection of all earlier development versions. The full
storage suite passes after that repair. Original failures are retained.

Strict Clippy also identified that the larger recipe pushed `PrepareGain` over
the request enum's size threshold. Its proposal is now boxed at the service
boundary, with ordinary owned lifetime and no lint suppression.
The final 320 service tests pass with one existing opt-in ignore, followed by
strict workspace/all-target Clippy with `ui-harness`.

The first export fixture incorrectly selected the project root for a macro.
The existing direct-child guard rejected it without an audio edit. The corrected
fixture wraps the unchanged edit in an ordinary Sequence group and applies the
same semantic command to that selectable owner.
The next fixture run used the generic peak-above-0.5 assertion for a -6 dB
return. Independent dry preview measures 0.749076783657074, predicting an echo
of 0.3754277209755036; the measured echo is 0.3754277229309082. The corrected
fixture uses an explicit 0.37..0.39 peak range. No production DSP or emitted-file
verification threshold changed.

The release CLI fixture now passes limited preview and public Render followed
by full emitted-file verification. Its ordinary group has a -6 dB delay: the
Original click at sample 28781 produces its first echo at 43181, within the
unchanged 30-frame edit. All 30 H.264 pictures, two AAC audio blocks and one
signal window verify; measured offset is zero. Maximum block-level difference
is 0.05685868115 dB and minimum block SNR is 33.63830809 dB. The output declares
2048 samples of priming and 48048 presented audio ticks. The actual movie,
receipt and authoritative project package are retained.

Final offscreen Metal replays pass 350 scenario checks: 65 continuous-return,
79 allocated-tail, 124 audio-treatment and 82 EQ checks, plus four Kestrel
shortcut audits. They cover saved levels, Undo, dot/macros, stale/absent captures,
single-play isolation, invalid levels, preserved gain edits and unchanged time.
The final 1280×820 and 960×640 PNGs were inspected against the stored workspace
target. Both return levels and command labels remain readable after inspector
scrolling; the explanatory text needs more scrolling at minimum height. No live
window, physical keyboard, audio device or listening test was performed.

The primary agent implemented and reviewed this change alone. Review covered
the complete substantive diff, wire defaults and preservation, version gates,
captured native targets, scoped edits, shared drive, owner ordering, permission
identity and resource cleanup. No independent review or live GUI is claimed.

Independent sound-event continuous sends, wider copy/retained/Preserve/loop
qualification, partial attached-sound processing scopes, persistent filtered
caching and remaining mastering/performance work stay open. Existing atomic
scope refusals remain. DP-09, DP-16 and the full product goal remain open.

[Evidence](../../tools/audio-qualification/evidence/2026-10-12-wet-owners)
retains the exact source patch, source/executable hashes, compressed logs,
failures, dry-click oracle, actual export/project and inspected native captures.
Run `python3 check-evidence.py --source` there to check current source; omit
`--source` for historical evidence. Cargo sequences and replays were serial on
Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and
qualified FFmpeg 8.0.3. Broader platform and listening checks remain in the
[owner verification list](../REQUIREMENTS.md#to-verify-owner).
