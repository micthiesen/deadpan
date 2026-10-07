# Release audit, 2026-10-06

This record covers the resumed changes after `ae79d614`. It is evidence for
the tested boundaries, not a release qualification by itself. The full
[requirements tracker](REQUIREMENTS.md) remains authoritative.

## Verified on this Mac

- The pinned FFmpeg 8.0.3 source/signature build passed under
  `~/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix`. Cargo native adapters and
  workers subsequently compiled and ran against that durable prefix.
- Nine beat-object core tests and five native grammar unit tests passed.
  These cover `ib`/`ab` selection and first-play mark/caption/cutaway handling.
  Sound-bearing Repeat transforms remain unfinished.
- Ten cancellation-related jobs tests passed, including the real worker
  whose cancellation truncates a frame and the preservation of unrelated
  reader failures.
- The focused CLI, worker and privacy run passed 68 tests in 36.462 seconds.
  It covered malformed frames, output limits, deadlines, cancellation,
  artifact admission, provenance, downloader redaction, local diagnostic
  export and network-denied local workflows. The latter completed in
  30.714 seconds. Detailed output is retained locally at
  `/tmp/deadpan-resume-20261006/security-checks.log`.
- Four UI replays passed 681 checks: groups, keyboard layouts, model packs
  and diagnostics. They used private copies of the built executables;
  `deadpan-app` SHA-256 was
  `8703a6fff1a97219f8ecda1f2b4b726e4f599f33d79ff7ce6350f6935acc8235`.
  The reports at `/tmp/deadpan-resume-20261006/replays` retain checks, images,
  timing and helper hashes. The diagnostic export/collision panel was visually
  inspected. Layout retries settled; the reports retain their warnings.
  The live Kestrel source SHA-256 matches the reservation fixture:
  `368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
- The Models replay subsequently passed 28 checks, including keyboard
  activation of a license link through the production `OpenUrl` output.
  Its executable SHA-256 was
  `bcd2c9c490efc7288984c8ffee765de29edbb687d45e9582011e92231580f387`;
  evidence is in `/tmp/deadpan-resume-20261006/license-link-replay`.
  This verifies dispatch; the replay does not launch the configured browser.
- Deno source attribution extraction retained 275 source-file comment sets.
  The complete notice inventory now contains 553 distinct texts totaling
  1,687,073 bytes. Each text's SHA-256 matches its filename. The manifest
  SHA-256 is
  `86478938d62f160320aa4163daf59b80365cfba28821feca5647136cc563a481`.
  A complete regeneration from a clean checkout of pinned Deno commit
  `0c071246a412575e07423263404a5d13e7ed6aa2` reproduced the manifest and
  every text byte for byte. It covers 768 dependency crates, 59 Deno workspace
  crates and 13 native/standard-library source components. The regeneration
  evidence is in `/tmp/deadpan-resume-20261006/notices-regenerated`.
- The instrumented libFuzzer campaign passed all 27 targets, each run for
  60 seconds with four concurrent processes: 41,346,130 executions in
  431 seconds of campaign wall time, with no failure artifacts or failed
  exits. Every target reported coverage, ranging from 160 to 13,518 edges.
  `/tmp/deadpan-resume-20261006/fuzz-campaign-3/summary.json` retains per-target
  counts, coverage, logs and executable hashes. Its complete source manifest
  SHA-256 is
  `c47b430b6724844150c347e94c76e0214399a1b5e428213403eb2aaea0352f2d`.
  The corpus was not merged. This is a bounded campaign, not proof that all
  malformed inputs have been covered.
- A fresh ad hoc signed bundle passed `bundle-verify` from a relocated copy
  with a fresh HOME and scrubbed environment. The positive checks include
  compiled helper pins, Deno/V8 notices, creation and verified MP4 render,
  bundled Python/MLX/LTX and Metal execution. Negative checks refuse tampered
  or missing helpers, a modified AI worker and missing Deno notices.
  The 721.9 MiB bundle contains 74 audited Mach-O files. Evidence is retained
  in `/tmp/deadpan-resume-20261006/bundle.log` and `bundle-verify.log`, with
  the copied test bundle at the location printed in the latter.
- The packaged native window smoke test initialized Metal on the Apple M5
  Max, passed and completed its shutdown callback. It left no test window
  running. The executable SHA-256 is
  `a46d2b662ad13161c6495d7953bab7d46fc7936da26900db2076956c60a41556`.
- The follow-up run passed 68 tests in 220.113 seconds, including the shared
  AI network launcher, hostile inference workers, the corrected beat grammar,
  the full custom-keymap compatibility audit and all xtask tests. TCP, UDP
  and Unix-domain connections were denied in Python and its descendant;
  local files, protocol traffic and owned process teardown worked.
  Two unrelated read/serde tests had nextest pipe-close warnings. Both passed
  a serial rerun in 0.016 seconds without warnings; their cause was not
  established. Logs: `followup-checks.log` and `pipe-warning-recheck.log` in
  the same scratch directory.
- The rebuilt packaged app exercised the real AI path with a fresh HOME,
  scrubbed environment and the production network-denied launcher. Offline
  model import and the model check passed in 13.458 seconds. A 30-frame Hold
  in the committed `cfr-bframes.mp4` fixture generated in 78.166 seconds.
  Its fallback document remained unchanged until explicit acceptance, then
  the accepted revision rendered a verified MP4 in 2.413 seconds. The app
  SHA-256 was
  `6c4926e00aab2aa22ae15d7fdd7bdf382d7a0d1b03af06b888a9dca0c34ae9e7`.
  `/tmp/deadpan-resume-20261006/packaged-ai-network` retains the summary,
  commands and reports. No native window was opened.
- This rebuilt bundle then passed every positive and negative `bundle-verify`
  check from another relocated, scrubbed copy. Its native smoke test
  initialized Metal, passed and completed shutdown. Logs are
  `bundle-network-verify.log` and `bundle-network-smoke.log` under the same
  scratch directory. No test window was left running.
- [Manual application replacement and rollback](UPDATES.md#application-versions-and-rollback)
  passed using the two complete signed bundles. Each installed copy was
  checked against all 10,818 files in its published checksum inventory and
  its code signature. Moving from the previous build to the candidate and
  restoring the previous build preserved both the project dump and SQLite
  bytes. Both archived bundles remained available. Partial and duplicate
  checksum inventories were rejected by the qualification script. Evidence:
  `/tmp/deadpan-resume-20261006/app-rollback/summary.json`.

## Failures retained and corrected

The first fuzz attempt found that cargo-fuzz 0.13.2 does not accept
`--locked`. The runner now performs a locked offline Cargo preflight, builds
offline and checks the lockfile hash. The next build found an adapter calling
a test-only keyboard helper; it now calls the production event router.
Both attempts stopped before a campaign ran. Independent review also fixed
partial target enumeration and the failed-build lock check. The first full
gate stopped at Clippy's `while_let_loop` lint in the runner; the loop was
corrected. The second gate found a redundant closure in one generation
protocol test; the direct function passes its focused strict lint check.
The third gate passed strict lint and 4,693 of 4,696 workspace tests, with
10 explicitly ignored qualification tests. Its three failures were stale
expectations for the new beat grammar: a custom key path collided with `rab`,
an operator test still rejected `rib`, and a wire-format test still rejected
beat objects. Those expectations were corrected and passed the focused run.
The fourth gate exposed a pre-existing crash-test report bug: SIGKILL could
interrupt `writeln!` before its newline, leaving a partial revision identity
that the parent treated as a complete record. The harness now admits only
newline-terminated reports. Its preceding attempt record still proves at most
one unreported commit; all database/history checks remain unchanged. An
every-byte truncation regression and all three real process-kill tests passed
in 5.110 seconds (`chaos-report-checks.log`). Independent review found no issue
with the fix.
These logs remain under `/tmp/deadpan-resume-20261006`.

## Final gate and disposition

`cargo xtask gate` passed on the reviewed source after the crash-harness fix:
formatting, strict workspace and UI harness Clippy, all 4,700 workspace tests
in 448.456 seconds, all 1,023 UI harness tests in 255.801 seconds, and both
doc tests. The gate retained 10 explicitly ignored workspace qualification
tests and 2 UI harness qualification tests; it does not count those as passes.
Log: `/tmp/deadpan-resume-20261006/gate-5.log`.

The workspace run had no pipe-close warnings. The UI harness run reported one
for `headless_errors_keep_the_same_structured_protocol`, which passed a serial
rerun in 0.032 seconds without a warning (`final-pipe-recheck.log`). The
earlier fourth gate had two such warnings in unrelated store tests. Their
cause remains unestablished; no worker-lifecycle test reported one.

Independent review found no remaining implementation or normative gap in the
audited DP-23 scope. With this gate and the packaged runtime evidence above,
DP-23 is complete under §29.1. Other product requirements and release gates
remain governed by their own evidence. Current-bundle native online and
offline-distribution acceptance for DP-22 is still in progress.

No campaign result is inferred from the presence of a target or corpus.
Clean-machine and physical/human checks follow the owner scope in spec §29.1.

## Native installation follow-up

[Native workflow evidence](qualification/native-install-2026-10-06.md) records
local and permitted YouTube import, native model downloads and archive installs,
generation with unchanged fallback until explicit acceptance, and restart/render
with IP networking denied. The archive-installed project's movie was confirmed
published. The Documents export exposed an asynchronous macOS `UF_TRACKED`
change in its report; the corrected bundle recovered that exact movie with
full byte verification and published a fresh native Documents render. The
whole-app offline test also retains the macOS nested-sandbox limitation and
the exact nonblocking owner check. DP-22 is complete within §29.1's scope;
the broader import/export and release gates retain their own open work.

The [2026-10-07 follow-up](qualification/followup-2026-10-07.md) records
descendant Repeat clocks, backup settings, signed bridge updates, publication
recovery and their focused checks. It also retains the full-gate failures
and fixes. Its full gate passes 4,734 workspace tests, 1,027 UI-harness tests,
strict lint and doc tests. The corrected packaged update also passes real
generation, explicit acceptance, rollback and verified Render with the selected
provider retained. Native backup settings survive Save and restart. The real
generation's initial missing receipt-field failure remains recorded, with its
regression and corrected run. All test windows and workers are closed.
