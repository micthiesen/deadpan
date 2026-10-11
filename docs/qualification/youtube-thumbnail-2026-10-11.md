# YouTube confirmation thumbnails, 2026-10-11

Status: implemented and locally verified. Final regression follow-ups, offscreen
visual checks, real CDN/app-job checks and release bundle checks pass. The initial
workspace gate's intermittent proxy failure remains recorded below. DP-14 stays
partial for its other requirements.

## Behavior and boundaries

The native import job now fetches an available thumbnail after inspecting the
video and before asking to download its picture/audio streams. The worker
produces at most 480 × 270 owned RGBA8 pixels. The UI preserves the aspect,
names the image for accessibility and keeps one texture for the active preview.
Cancellation, a different preview, or leaving confirmation releases that
texture. Missing or unavailable images show an explicit status and still allow
the user to confirm a valid import.

The thumbnail client accepts only canonical HTTPS ytimg hosts and the inspected
video's ID. It has no cookies, proxy or redirects, validates TLS through the
platform verifier, filters non-public addresses before connection and applies
four-second network deadlines. A single DNS worker retains its occupied slot
after a timeout until the OS lookup finishes. Input is limited to 4 MiB,
4,096 pixels per dimension and 4,194,304 pixels total; the decoded raster is at
most 32 MiB. See [YouTube import](../YOUTUBE_IMPORT.md#confirmation-thumbnail)
for the precise limits and cancellation behavior.

Review of image 0.25.10 found that its WebP adapter does not forward the
best-effort allocation budget to metadata reads. The app therefore validates
WebP chunk ranges against the actual bounded input before invoking that decoder,
checks coded frame dimensions independently of the extended canvas, refuses
conflicting dimensions and duplicate coded pictures, and rejects animation.
The allocation setting is not claimed as a total-process memory cap or a hard
decoding deadline. JPEG orientation is applied before sizing.

The thumbnail never becomes project media, a receipt, proxy, AI input, playback
picture or export input. It does not change authored history or source timing.
The only added Cargo package is image-webp 0.2.4, MIT OR Apache-2.0; JPEG uses
the already locked zune dependencies. All versions remain locked.

## Deterministic checks and review findings

Sixteen loader tests pass for the admitted URL/IP ranges, response size and
cancellation, unavailable states, bounded DNS ownership, PNG/JPEG/WebP pixels,
JPEG EXIF orientation, dimension/area limits and malformed WebP structures.
The lossy WebP fixture separately exercises VP8 pixels; the image crate's own
encoder exercises lossless WebP.
Sixteen app tests pass for import confirmation, optional images, cancellation,
expiry, recovery, naming and explicit texture release. The real-import test
returns early in ordinary runs; the separate opt-in run below actually performs
the network import.

One nextest run reported a lingering-output warning on the pure cancellation
test while all fifteen tests passed. Its isolated follow-up passed in 0.011 s
without that warning; the initial result is retained. Initial compile failures
in the new test harness and qualification example, and an assertion for
unapplied texture deltas in the first cache test, are also retained with their corrected
follow-ups.

The first offscreen replay found a real layout failure: the start card used the
previous frame's content height, so newly arrived metadata moved its button
after the first paint. A pointer click targeted the old location and never
started the transfer. Import stages now use one fixed top edge. The idle form
settles measured height before paint. A subsequent replay exposed a modal
ScrollArea constrained by the shorter entry sheet's previous size; an explicit
bounded minimum scrolling height lets it grow before shrinking to its content.
These failures were preserved, not converted into waiting in the click test.

The third replay passes 111 workflow checks plus the separate production
shortcut audit, over 131 recorded steps. It uses an owned procedural image
decoded through the production loader, not an image of the named remote video.
It covers default/compact images, immediate pointer and keyboard confirmation,
decline, transfer cancellation, project opening, optional-image states and
texture teardown. All frame-layout assertions pass; informational warnings
about second retries and consecutive frames with different retry causes remain.
The later visual follow-up keeps the rights notice beside the decision at compact
size and adds a long title that actually needs scrolling. Its 124 checks pass,
but inspection of the first frame of a repeated long-title import found a
further defect: the centered sheet retained the earlier short form's top edge,
placing the new actions below the window for one frame. Subsequent settled
frames were correct. The sheet now anchors to a fixed top edge, and optional
images, long titles and repeated confirmation are checked on their first frame.
The final replay passes 125 workflow checks plus the shortcut audit, over 146
steps. The first painted sheet button stays at exactly the same rectangle on
the following frame. Long-title confirmation and a repeat after scrolling both
keep the rights notice and buttons visible immediately. Final default, compact,
long-title, scrolled and repeated-confirmation PNGs were visually inspected.
Its executed app has SHA-256
`322812a234e2b61a07cb01031597b9940dd4ce5de6a53ed0b7dbd3ea3e131beb`.
Informational multi-retry warnings remain for consecutive frames with distinct
input/state changes; all ignored-retry and repeated-cause assertions pass.

## Regression gate

The initial full gate is retained as a failure: 5,664 of 5,667 workspace tests
passed, with three failures and twelve skipped tests. Two privacy-audit failures
identified the new client and dependency owner. The client was reviewed against
§27.4, documented in [Privacy](../PRIVACY.md), and added to exact source and
dependency allowlists. All six privacy checks, sixteen thumbnail tests and the
network-denied workflow test pass in the follow-up.

The third failure is the unchanged proxy worker's
`a_stalled_worker_is_torn_down_and_retried_once_in_a_fresh_output` test:
`invalid_packet`, packet 20 had PTS 21021/DTS 20020 instead of PTS 20020.
Its isolated follow-up passes in 1.364 s. This remains an intermittent failure;
the rerun does not establish a fix or prove that machine load caused it.

Strict workspace and UI-harness Clippy checks pass. All 1,105 UI tests and
workspace doctests pass in the gate continuation. The workspace and UI runs
each report one lingering-output warning after passing assertions, in
`intrinsic_policy_window_preserves_bound_support_outside_the_visible_run` and
`empty_selected_beat_yanks_and_pastes_at_its_sibling_slot_without_cursor_inference`
respectively. Both isolated follow-ups pass without lingering-output warnings;
the app follow-up also reruns all sixteen YouTube tests. The affected
`linked-original` and `layouts` replays pass 19 and 150 workflow checks,
respectively, plus their production shortcut audits. No keyboard mapping changed.

## Real network and app job

On the M5 Max / 128 GiB Mac, macOS 26.5.2, Rust 1.97.1, the production client
fetched Blender's *Caminandes 2: Gran Dillama* thumbnail from the existing CC-BY
[video fixture](https://youtu.be/Z4C82eyhwgU). Fetch, decode and resize took
2.455 s. Its 480 × 270 RGBA raster contains 518,400 bytes with SHA-256
`a777095af114074ddacec3a5949887eb9269f2d0d9ab4c5087a90ae457764c9f`.
The retained PNG was visually inspected and has SHA-256
`3a574272712cdc17360c630a8dec96321e3980a8e21ad054c65aebae3b5baf91`.

The reproducible `qualify_youtube_thumbnail` example's executed binary has
SHA-256 `39a5e6c48d4a05076d60f69cf3b21bfa8190494442fba5aab2a7bc23ebb0685a`.
The separate normal-app job test required a Ready thumbnail before confirming,
asserted that no package existed then, and created and validated a Ready
AV1/Opus Original in 14.2 s. Its test binary has SHA-256
`d342b93e3da51937b407f9e05a7ee7d1f1a2fc233ac93d34c1a27ac9bfec4935`.
The actual media worker has SHA-256
`f4c180fe23e384a0feeefbac0d76bf99dbe4e72cee06adffa46f1ddefe674542`.
All three binaries are retained in local qualification scratch.

The release example independently fetches and decodes the same image in
0.263 s and produces exactly the same RGBA and PNG hashes. Its executed binary
has SHA-256 `b427036fa66cb66e9fa0d6ca169aa8942d918c88ade3faaed194398bb612fdba`.
These single requests are observations, not a CDN latency guarantee. The
debug example under `sandbox-exec` with network access denied returns
`Unavailable(Network)` in 0.021 s, exits with failure and leaves its output
directory empty.

No live browser or native-window inspection was used. Offscreen Metal replay,
accessible labels, actual paint clips and exported PNG inspection provide the
UI evidence here. Physical keyboard, OS IME delivery, VoiceOver speech and other
hardware remain in the owner's verification scope.

## Final release and retained evidence

The final 742.8 MiB ad-hoc-signed bundle passes `cargo xtask bundle` and
`bundle-verify`: relocated import/render, pinned-helper checks under hardened
runtime, bundled AI runtime/Metal and negative tamper/removal checks. Its app
executable has SHA-256
`2f461f9fca51e855a62c0f167ae82cb4f2ccd7f8c659ed9108e4aeaf3d122b00`.
The SBOM includes image-webp 0.2.4 with its MIT OR Apache-2.0 license expression.
These checks do not exercise a native packaged thumbnail window or a clean Mac.

Final formatting, normal-app and UI-harness strict Clippy checks pass. The
final app check reruns sixteen YouTube tests and the earlier lingering-output
case without a warning. The source changed after the large UI suite only to
reset confirmation scrolling and stabilize the sheet position; the final
replay exercises both changes. No native unsafe adapter changed.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-11-youtube-thumbnail/README.md)
contains the failed gate, initial layout/compiler failures, passing follow-ups,
real client reports, selected PNGs, final source hashes and executable identities.
Large JSON/log files are losslessly compressed with their original hashes.
Full intermediate captures and large binaries remain in the scratch locations
listed there. The earlier bundle predates the last sheet-position fix and is
explicitly superseded by the final bundle.
