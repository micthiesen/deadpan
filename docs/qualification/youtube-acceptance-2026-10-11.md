# YouTube import acceptance audit, 2026-10-11

This audit maps DP-14 to specification §15.2–15.4 and the one-Original
initialization it shares with DP-01. Completion uses §29.1's reference-Mac
scope. The whole product and its other requirements remain in scope.

| Requirement | Implementation and evidence |
| --- | --- |
| Bundled yt-dlp, compatible EJS and Deno; visible versions | [Helper bundle](../YOUTUBE_IMPORT.md#helper-bundle), actual baseline probes from a relocated app, bundled content/signature pins and tamper negatives. |
| App-controlled absolute executables, argument arrays, ignored configuration and plugins | `acquire::base_arguments`, `metadata_arguments`, the cleared `runner::environment`, per-launch helper rechecks and argument/environment tests. The only enabled JavaScript runtime is the pinned absolute Deno path. |
| Signed compatible updates, managed storage, preserved baseline and rollback | [Signed updates](../UPDATES.md#qualification-record-2026-10-06-m5-max): actual signed helper installation, activation, downgrade refusal and rollback through an ad hoc signed packaged app on this Mac. Unit and job tests cover corrupt state, replay floors, incompatible manifests and failed probes. Helpers never update themselves in the bundle. |
| Watch/share/short URLs and one explicitly indicated playlist video | Pure normalization/property tests and `--no-playlist`. Only the canonical eleven-character video ID reaches the helper. Metadata describing another video or multiple videos fails before transfer. |
| Title and available thumbnail before expensive transfer | [Confirmation qualification](youtube-thumbnail-2026-10-11.md): bounded actual CDN fetch, decoded image, accessible labeling, compact scrolling and explicit confirmation. Current real app-job verification requires a ready thumbnail and absence of a project before confirmation. |
| Best useful original quality and private provenance | [Quality qualification](youtube-quality-2026-10-09.md): qualified AV1/VP9/H.264 picture and original-language Opus/AAC sound, no per-word leveling, full-resolution stream-copy assembly, separate later proxies and persisted private author/title/URL/date/source/stream evidence. Both [size-bound](youtube-limits-2026-10-11.md) and [network](youtube-network-2026-10-11.md) milestones exercise real packaged 4K import. Unsupported interpretations fail explicitly. |
| One Original and full-source baseline, readiness, cancellation and recovery | Shared single-Original creation and [project acceptance](project-acceptance-2026-10-09.md). Job tests and replays cover confirmation, cancellation, name collisions, window shutdown, late completion, failure cleanup and opening a complete package. A remote import never inserts another picture source into an existing project. |
| Deleted, regional, rate-limit, extractor, format, age/auth and interruption errors | Typed classification tests, app guidance and worker/job tests. An explicitly chosen cookies file is copied owner-only, never read from a browser, omitted from stored provenance/events and removed after the attempt. Authenticated account behavior remains an owner check. |
| URL/redirect confinement, no file/private-network tricks, path or argument injection | [Network qualification](youtube-network-2026-10-11.md): authenticated per-connection public-host/IP admission, OS-enforced proxy confinement, real bypass/private-target probes and pinned-helper file-URL refusal. Format identifiers and extensions are bounded allowlists; the native library sanitizes remote titles to one 160-byte filename component, with traversal/collision tests and atomic no-replace publication. |
| Bounded subprocess work and metadata | Deadline/cancellation, bounded stdout/stderr, checked metadata/download limits, owned-group teardown and abandoned workspace cleanup. The gateway bounds connections, buffers, DNS, connection attempts and idle time. These are explicit application bounds, not a claim of complete containment of arbitrary malicious native code. |
| Rights notice, no DRM bypass and local footage | The notice is visible before confirmation, DRM-marked formats are excluded and no bypass flag is supplied. [Privacy audit](../PRIVACY.md) and network-denied local workflows cover the boundary; the import downloads footage and does not upload local projects or media. |

## Tracker reconciliation

The old DP-14 remainder mixed owner checks, adjacent requirements and optional
work. §15.3 requires actionable interrupted-transfer failures; it does not
require resumable video transfer. Model-pack resumption remains required under
§14.3 and is separate. §25.1 explicitly excludes Intel and Linux ports from
this macOS specification. A hosted update channel is not required by §15.2's
signed-manifest update contract; file manifests and explicit HTTPS inputs are
implemented. Broader source-format and playback performance work remains in
DP-02/DP-16/DP-24 rather than blocking the implemented qualified import paths.

Physical keyboard/IME delivery, VoiceOver speech, an authenticated account,
clean/second-machine installation and lower hardware tiers remain in
[To verify (owner)](../REQUIREMENTS.md#to-verify-owner), with concrete steps.
They do not substitute for missing implementation. This audit uses existing
offscreen workflow evidence and the current actual app job; no live native
window or browser was opened.

## Final result

DP-14 is Complete under §29.1. The final relocated packaged app imports the
3840×1608 AV1/Opus *CHARGE* Original in 38.023 seconds, reopens it and validates
the complete baseline. The [network qualification](youtube-network-2026-10-11.md)
records all regression, package and real-import results, including the
separately fixed AAC inspection-budget failure and the resolved UI-test
lingering-output warning. No other requirement or delivery gate is closed by
this audit.
