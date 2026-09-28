# Independent AVFoundation audio timing

AVFoundation does not rescue the measured no-edit-list AAC export. For the same
120-frame, 60 fps file that FFmpeg reads 1,024 samples late, AVFoundation loses
the opening impulse and places the remaining impulses 1,088 samples early.
The default-edit-list reference is sample-aligned in both independent readers.
DP-17 remains open. This is an asset-reader measurement, not acoustic playback.

The [harness](../../tools/media-qualification/compatible/README.md#independent-avfoundation-audio-observation)
and [retained evidence](../../tools/media-qualification/evidence/2026-09-28-native-audio/README.md)
preserve the raw timing, formats, attachments, complete PCM, failed checks,
commands and identities. Inputs are the unchanged MP4 files from the
[encoder experiment](encoder-timing-2026-09-28.md). No new encode, manual trim,
event-based alignment, normalization or replacement of the earlier evidence
occurred.

## Measured result

The authored stereo events are at samples 100, 48,000 and 95,800. The authored
end is sample 96,000 at 48 kHz. One video frame is 800 samples; the declared
encoded tolerance remains 799 samples, with exact equality also reported.

| Observation | Default edit lists | Edit lists disabled |
| --- | --- | --- |
| Input MP4 SHA-256 | `200fd7eb55e7608fe2a266d0bc3e9703b574cbdb9260190f2ea80b7e6f8b7592` | `4aabc997d14e1649b2e40b6a6ad062c53d5d38f5357165e48cc4e1f58ab5d4eb` |
| AVFoundation track source segment start | 1,024 samples | 2,112 samples |
| AVFoundation track target start | 0 | 0 |
| Returned PCM and track end | 96,000 | 94,912 |
| Opening event, both channels | Exactly sample 100 | No qualifying event; the strongest small residual has the wrong sign and amplitude |
| Middle and final events, both channels | Exactly authored samples | 46,912 and 94,712, each 1,088 samples early |
| Endpoint error | 0 | −1,088 samples, or −22.667 ms |
| Previously measured FFmpeg events/end | Exact | +1,024 samples, or +21.333 ms |

Both stored-sample and decoded-PCM readers reach
`AVAssetReaderStatusCompleted`. Decoded buffers have matching raw/output PTS
and durations on the exact 48 kHz sample grid. Their trim, speed and reversal
attachments are absent; the adapter does not infer or apply another trim.
AVFoundation itself supplies the differing source-to-target segment mappings.
The disabled file's asset duration is still two seconds, while its audio track
and returned PCM are shorter. Complete reading uses the framework's default
asset range and does not claim retrieval of coded padding outside that range.

Apple's archived [TN2258](https://developer.apple.com/library/archive/technotes/tn2258/_index.html)
describes a historical 2,112-sample priming assumption when an explicit value
is unavailable. The measured mapping is consistent with that behavior; this is
an inference about this reader result, not a claim that the archived note
specifies every current AVFoundation implementation. The actual file and reader
observations establish the failure independently.

## Verification

Base commit: `3b6e8da5579fb9e0ddf6ef80396642f535afedfa`.
Host: Apple M5 Max, 128 GiB, macOS 26.5.2; Apple clang 21.0.0, SDK 26.5.
The probe targets macOS 15 and links Apple system frameworks without FFmpeg.
The sanitizer build separately records and rechecks its selected compiler
runtime, including the actual loader-path resolution.

- All 93 Python tests pass in 6.19 seconds including the outer runner. They
  cover the shared event oracle, native clock/attachment admission, failure
  retention, bounded files and process faults.
- Re-evaluating all 12 previously captured encoder cases takes 2.58 seconds.
  Every existing check and observation is unchanged after extracting the
  shared timing-only helper. The original 13th case remains the retained
  hardware B-frame encode rejection; no new encode or decode was necessary.
- Strict Objective-C compilation with `-Wall -Wextra -Werror`, ARC, ARC exception
  cleanup and blocks passes. The first actual read takes 4.91 seconds, but its
  oracle rejects a schema defect: CoreMedia's unsigned-byte Boolean boxed as
  numeric JSON `1`. The original source, full observations and PCM remain saved.
- Explicit C `bool` boxing fixes the three affected producer fields. The
  corrected run takes 4.18 seconds. The default file passes its scoped timing
  checks; the disabled file fails seven required checks. Its process exits are
  zero; the overall experiment correctly exits 1 for the timing failure.
- ASan/UBSan completes the same pair in 4.90 seconds with the same timing result,
  no sanitizer/process faults, unchanged native sources and passing final
  file-admission checks. Instrumentation covers the observer, not Apple
  frameworks; macOS leak detection is disabled.

Independent review covered the observer, adapter, public event helper and
runner. The runner correction retains a sanitizer candidate path so a symlink
retarget cannot hide behind unchanged target bytes. Its regression deliberately
uses identical bytes at a different target. Review confirmed both this fix and
the explicit Boolean boxing correction.

Only the parent ran compilers and tests. Concurrent Rust renderer authoring was
outside these tests; native dependency inventories stayed unchanged during each
run. The previous Rust/UI results remain valid for their recorded source, and
do not qualify the new renderer work.

## Export policy decision and remaining scope

The normative spec still requires no edit lists. A decision has been requested
on this concrete replacement for the muxing line in §22.3:

> Explicitly validated stream start and sync; edit lists permitted for encoder
> delay, padding, and frame reordering.

That proposal preserves emitted-file verification and the sub-frame encoded
tolerance. It is not approved by these measurements alone, and the specification
has not been changed. Independent color conversion work can proceed while this
decision remains open.

The native video stack, closed GOP independence, boundary-content/audio quality,
physical playback, other OS versions, full-resolution performance, HDR, shared
immutable revision rendering, verified publication and the native one-action
Render workflow remain required. A two-file reader result is not a complete
export qualification.
