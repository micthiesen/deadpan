# Independently timed input gain and mute

The shared audio bus now evaluates each contribution's linear input prefix in
its own clock before shared saturation or EQ. This fixes the default treatment
order after a fractional-sample move: the Original keeps its retained clock,
and its attached sound keeps the current owner clock. Saturation receives the
combined sum. No clock, authored recipe, policy permission or tolerance was
changed to make the old refusal disappear.

## Implementation and boundary

`group_mix` retains a stage range and Repeat-escalation flag per owner layer.
Each voice applies the prefix before the first saturation; the bus applies its
suffix once and passes that result to its parent. `shared_mix` uses the same
split at the first saturation or non-unity EQ. Recursive EQ evaluates its
complete prefix with the participating voices' own clocks. Only the delivered
suffix must agree across overlapping inputs.

Differing automation after a shared stage remains explicitly unsupported,
including an ancestor gain after a child's shared nonlinear stage. The negative
post-saturation regression retains refusal before provider access. Completing
that path, arbitrary sends, incoming tails, wider binding/Repeat/Preserve
qualification and persistent filtered caching remain agent work. DP-09/DP-16
and the full product goal remain open.

## Decoded-media checks

The fixture uses the real verified `pcm-stereo-48000.wav` decoder and inserts
one frame at 30000/1001 before an Original with an attached sound. At consumer
sample `t`, the Original's local clock is `(5*t - 24026)/8008` frames; the
sound's is `(5*t - 24024)/8008`. Neither is substituted for the other.

- A mute interval `[975/8008, 977/8008)` mutes only the sound at sample 5000.
  Its full 6406-sample output and four cold partial reads exactly match the
  independently clocked gain, sum and saturation oracle.
- A +3 dB trim with a −12 to +6 dB envelope over four frames matches the same
  decoded-input oracle exactly, including cold reads.
- Gain → EQ → saturation and gain → saturation → EQ both match direct f64
  convolution over full 2048-sample halos. The oracle starts with f32 inspection
  PCM, while temporal processing retains f64 intermediates; the new assertion
  allows four f32 epsilons at unity. Three shuffled cold partitions match the
  complete output bit for bit. No existing tolerance was relaxed.
- Gain placed after saturation still refuses the differing mute clocks before
  either ordinary or retained-context provider access. Muted-source admission
  and the previous constant-treatment regressions continue to pass.

All 973 locked audio and planning tests pass with zero ignored tests. The final
focused run passes all seven tests. Only comments changed between the complete
suite and final focused run. Strict workspace Clippy with `ui-harness`, rustfmt
and whitespace checks pass.

Archived `2d8c874cb1dcc2c0408a3fea71ac43907ee2df61`, with only the new test file
overlaid and a separate Cargo target directory, passes four tests and fails the
three new positive tests at the old unsupported-automation guards. The actual
baseline and current executables are separately hashed in the evidence.

## Release preview and export

The existing shared-EQ release fixture passes real H.264/AAC Render and emitted
file verification after this change: 54 pictures, 83 compared audio blocks and
two signal windows, both with measured zero offset. The largest block-level
difference is 0.3172504593 dB; the lowest block SNR is 18.91011398 dB. The file
declares 2048 priming samples and verifies 86486 presented samples. No packet
dropping, event alignment or tolerance changes were used.

This is a regression check of the shared preview/export path. The distinct
fractional input-automation behavior is established by the decoded-PCM tests
above, not by claiming this existing export fixture contains that scenario.
The exact movie, package, render receipt, verification report and release
executable hashes are retained.

## Evidence and scope

[Evidence](../../tools/audio-qualification/evidence/2026-10-12-input-automation)
contains complete compressed logs, command receipts, the expected baseline
failures, source and binary hashes, environment, review and export artifacts.
Run `python3 check-evidence.py --source` there to verify the retained measurements
against the current source; omit `--source` for historical evidence.

Measured on Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84),
Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix. The primary agent implemented,
reviewed and verified this change alone. No native controls changed; no live
GUI, device run, listening check or new interactive-latency claim is included.
