# Independent automation through shared audio stages

Gain and mute now retain each contribution's own clock before and after shared
saturation/EQ, including ancestor treatments following a child's shared effect.
A fractional-sample move no longer forces the Original and its attached sound
to use one automation clock or refuse a valid treatment order.

## Processing contract

`shared_components` carries each permitted contribution as f64 stereo PCM
through owner programs from children to parents. Shared saturation uses the
complete channel sum `s`, distributing the common factor `tanh(drive*s)/s`.
At exact zero it uses the continuous limit `drive`; a single permitted input
keeps direct shaping. This preserves the combined drive even when a later
independently timed mute removes one contribution.

EQ is linear. Selected-output reads recursively evaluate the complete jointly
driven prefix and return only that contribution. Selecting an output never
changes the inputs driving saturation. Tile identity includes selected output
and the full permission set, so an allowed sound cannot revive forbidden
Original or peer history. Each shared EQ uses one active input context; gains
continue using their separate clocks.

An immutable plan-wide flag chooses contribution processing. Request-local
clock comparisons cannot switch arithmetic between full and cold partial
reads. Exact Q32 gain accumulation spans linear stages; shared nonlinear or
temporal stages flush it. Event gain and Repeat escalation enter once.
Contribution PCM and maximum pending gain programs are admitted before media
access under the existing work, cancellation, depth and residency limits.

## Decoded-media checks

The verified stereo WAV fixture inserts one frame at 30000/1001 before an
Original with an attached sound. At consumer sample `t`, the Original's clock
is `(5*t - 24026)/8008` frames and the sound's is `(5*t - 24024)/8008`.

- Post-saturation mute over `[975/8008, 977/8008)` removes only the sound at
  sample 5000. Its 6406-sample output and four cold reads exactly match an
  independent decoded-input/common-drive/own-clock oracle.
- A +3 dB trim with a −12 to +6 dB envelope over four frames matches the same
  oracle exactly. All six gain/EQ/saturation orders pass direct f64 convolution
  over complete 2048-sample halos and exact shuffled cold partitions. The
  inspected inputs are f32; the convolution comparison permits four f32
  epsilons. No existing tolerance changed.
- Opposing decoded constant WAVs cancel exactly. Muting one contribution after
  saturation exposes the correct zero-limit contribution at precisely one
  sample. Nested parent gain/saturation and activation/deactivation boundaries
  retain the same behavior across cold reads.
- A one-context EQ limit catches duplicate contexts caused by independent
  clocks. PCM-budget refusal occurs before ordinary or retained provider calls;
  cancellation and missing-source recovery preserve subsequent output.
- Silent-Hold permission isolation runs through both aggregate and contribution
  processing. A supported root-event pause edit keeps full and cold PCM equal.

The proposed pause inside an attached sound owner initially failed in core,
before PCM. That is an existing unsupported partial processing-scope edit.
The final test preserves atomic refusal and separately exercises the supported
root-event route. No processor-scope guard was weakened or timing route faked.
This structural limitation remains open in DP-09 and the sound-event contract.

## Verification and review

All 1,069 locked audio, planning and playback tests pass, with zero ignored
tests. The final focused run passes all 15 tests. Strict workspace all-target
Clippy with `ui-harness`, rustfmt and whitespace checks pass.

Archived `90910379d4e0dc35372083ce1acbaf3bcecf2880`, with only the final test
file overlaid and a separate target directory, passes eight and fails seven.
Six positive regressions hit the old unsupported guards; the seventh expects
the new contribution-residency refusal. Actual baseline and current executables
are separately hashed. Earlier failures, including duplicate preflight work
in two existing Repeat/Preserve cases and the one-context EQ failure, are kept
with the fixes and final results. No resource limit was increased.

The initial release fixture attached the effect to its neutral crop wrapper,
whose input clocks agree. Its pre-render assertion rejected that scenario.
The corrected fixture attaches to the underlying Source and places the WAV
inside the visible Source range; its clock/gain assertion and export pass.

The primary agent implemented, reviewed and verified this increment alone.
The retained review describes numerical, scope, permission, cache and resource
checks. It is self-review, not independent-review evidence.

## Real release preview and export

The new fixture uses real CLI commands to attach a WAV, author saturation then
EQ then a changing gain, and insert a leading NTSC frame. Before rendering it
asserts that both owner clocks and their evaluated post-EQ gains differ.
The exact coordinates are retained in `export-results.json` notes.

Public Render emits H.264/AAC and passes preview/emitted-file verification:
31 pictures, 36 compared audio blocks and one verified zero-offset signal
window. The maximum block-level difference is 0.6367388991 dB and minimum block
SNR is 6.381142788 dB. No packet dropping, event alignment or timing
tolerance change was used. The exact movie, authoritative package, render
receipt, report and executable hashes are retained.
The file declares 2,048 priming samples and verifies 49,650 presented samples.

## Evidence and remaining scope

[Evidence](../../tools/audio-qualification/evidence/2026-10-12-shared-automation)
contains full compressed command logs, source/binary identities, the complete
implementation patch, environment, review and verified export artifacts.
Run `python3 check-evidence.py --source` there to check current source; omit
`--source` to validate historical evidence.

Measured on Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84),
Rust 1.97.1 and qualified FFmpeg 8.0.3. No native controls changed; no live GUI,
device, listening, other-hardware or interactive-latency claim is included.
Arbitrary sends, incoming-tail policy, partial attached-sound scope routes,
broader binding/Repeat/Preserve qualification and persistent filtered caching
remain agent work. DP-09/DP-16 and the full product goal remain open.
