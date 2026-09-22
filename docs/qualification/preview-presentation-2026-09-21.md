# Preview presentation qualification, 2026-09-21

The native viewer now separates requested, decoded and displayed picture
identities. A new request no longer clears accepted work waiting for the GPU.
The picture caption and accessibility name follow the submitted image rather
than the requested cursor. A freeze can therefore show original frame 20 at
sequence frame 121 without confusing those clocks. UI captions use one-based
frame numbers; document coordinates remain zero-based.

This work starts from `21d760e46a3bb2815a6b4bb60f08431639158cc3`, whose
[CI run 35673725010](https://github.com/micthiesen/deadpan/actions/runs/35673725010)
passed. Hardware is Apple M5 Max with 128 GiB, macOS 26.5.2, Rust 1.97.1, Metal
and the pinned LGPL FFmpeg 8.0.3 prefix at
`/tmp/deadpan-media-compatible-xyhilms4/prefix`. Schemas and dependencies are
unchanged. Logs and source hashes are retained in the
[evidence directory](../../tools/media-qualification/evidence/2026-09-21-preview-presentation/).

## Implemented behavior

`presentation::Presentation` owns immutable request identity, accepted picture,
displayed identity, decode progress and picture errors. Project identity includes
the session, project, revision and explicit Source/Sequence location. Displayed
original-source identity remains separate. Late replies, including failures,
cannot change the current picture, metadata, progress or errors.

An already accepted image remains a correctly labelled fallback while a newer
request decodes, including when the accepted image has not reached the GPU yet.
The latest stopped-frame reply replaces the accepted image. This does not change
the worker's current cancellation policy or implement continuous playback.

The viewer caption is drawn after rendering so it names the image painted in that
UI pass. The bottom bar retains the requested boundary and pending-update state.
A successfully submitted resized target replaces the prior registered texture;
allocation/render failure retains the old texture and displayed identity.
Backgrounds explicitly remove old textures. Empty sequences do not claim to show
source frame zero. Current decode failure clears the old picture and reports
unavailability. Render errors stop automatic retries until a fresh request or
decode; successful recovery clears the picture error without clearing unrelated
project or action errors.

GPU submission ordering on the shared queue establishes what the subsequent
egui draw samples. It does not measure the physical display timestamp or qualify
audio/video synchronization.

## Headless and real-media checks

Ten new tests cover accepted-but-unrendered work surviving a new request,
caption/source provenance advancing only on presentation, stale success/error
rejection, clear/reopen epochs, current decode failure, render failure/retry,
error recovery while a later decode is pending, repeated source pixels at
different sequence positions and revisions, Source context, backgrounds and empty
sequences.

The actual-media integration opens the registered `cfr-bframes.mp4` fixture on
the real preview worker, retains an old revision's completed reply, and adds an
ordinary Freeze Hold through the store command path. After the revision change,
the old reply is rejected. Sequence frames 121 and 122 both decode original frame
20 with identical bytes, while their captions advance to 122 and 123 respectively.
These tests exercise presentation transitions without claiming actual GPU failure
injection or physical display timing.

## Native appearance and keyboard checks

A disposable development `.app` wrapper opened the existing test-only
`Keyboard review.deadpan` package. It contains the debug executable and uses
developer libraries, so this is not a portable distribution test. No authored
edits were made during this review.

Observed results:

- `12l` in Source context reached boundary 12/120, displayed fixture number 012
  and caption/accessibility name `Showing source frame 13`.
- `:sequence`, Return and G reached boundary 120/120, displayed fixture number
  119 and caption/accessibility name `Showing sequence frame 120`.
- Tab, forward/reverse pane cycling and `/` entered source search. Typing `cfr`
  and pressing Left kept Sequence boundary 120/120 and its displayed image.
- Escape and Tab returned to viewer navigation. Two `h` presses reached boundary
  118/120, fixture number 118 and `Showing sequence frame 119`.
- At the observed 1234×768 screenshot size, the large picture remained dominant,
  the caption fit directly beneath it, controls stayed visible, and the requested
  boundary remained separate in the bottom bar. Native Command-Q exited; a
  process check found no remaining `deadpan-app`.

Dragging the observed window corner and invoking its exposed zoom action did not
change the observed dimensions. Small-window layout and native resize success
are therefore not established by this run. Full VoiceOver, CJK IME, non-US
physical keyboards, light appearance, physical display calibration and GPU/device
loss remain untested. Existing headless text/IME, pane focus and target geometry
tests still run in the gate.

The interactive binary was captured before the final picture-error ownership
follow-up. That follow-up leaves the observed geometry, caption and key paths
unchanged; its recovery behavior is checked headlessly. The final binary passed
the native startup/shutdown smoke test.

## Review and repository gate

An independent reviewer examined request/decoded/displayed identity, stale
replies, repeated-frame captions, background transitions, texture retention and
renderer error/submission ordering. A follow-up reviewed presentation-owned
errors and recovery while decoding. Neither pass reported a finding.

The first gate stopped on a test-only `field_reassign_with_default` Clippy warning.
The initializer was corrected without suppression. The final gate passed
formatting, strict workspace Clippy, **763 tests with zero failures or ignored
tests**, locked workspace build, doctor and native Metal startup/shutdown smoke.
The app contributes 53 unit/integration tests. The earlier failed gate is retained
alongside the final logs. Native decoder/DSP code and shaders are unchanged, so
sanitizer and GPU color/pixel matrices were not repeated.

The project remains incomplete. In particular, the worker currently cancels
superseded stopped-frame requests; continuous picture scheduling, application
audio transport, the complete voice/master graph, preview/export equivalence,
full editing, generated-provider preview and release qualification remain open.
All DP requirements and Gates A through G remain open or partial.
