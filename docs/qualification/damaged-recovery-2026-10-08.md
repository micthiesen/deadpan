# Native recovery of an unreadable project

Source base: `167300e4`, with the damaged-recovery changes. Host: Apple M5
Max, macOS 26.5.2 (25F84), Rust 1.97.1 and pinned FFmpeg 8.0.3. Core format
47 and SQLite schema 75 are unchanged. This advances DP-01; wrapped raw I/O
classification remains separate work.

## Behavior and review

A failed native Open offers the package's listed backups. Checking a backup
fully validates it without replacing the database. Restore is a separate
action, bound to that failed Open, current session/head, package identity
and observed database/sidecar/manifest/backup files. Changed files, a new
Open or an intervening edit refuse. A now-readable project must use ordinary
restore, which first preserves its current state. An unreadable manifest
requires typing the backup's project identity explicitly.

Replacement waits for active backup copies, keeps the old main database,
WAL, SHM and rollback journal in a named quarantine, and installs a verified
copy. Closing a session releases its idle read-only backup monitor. Failed
opening after replacement preserves the replacement receipt and the previous
workspace. Interrupted moves report their retained location; a final sync
failure reports an installed database with uncertain crash durability.
No old media is removed. The complete contract is in [Backups](../BACKUPS.md).

The sole working agent reviewed the store, service and UI changes; no
additional reviewer was spawned under the user's sole-agent instruction.
Review corrected rollback-journal preservation, idle monitor ownership,
active-backup exclusion, post-install failure reporting and modal keyboard
priority. A regression blocks host-discovery publication after replacement
to test the durable receipt independently of successful reopening.

## Verification record

The first UI fixture corrupted only the main database while leaving a valid
WAL. SQLite recovered it normally, so the expected failed-Open offer never
appeared. The disposable fixture now closes the project, drains owned backup
workers, removes its sidecars, and then corrupts its database. A separate
service-test timeout came from waiting for an already-consumed idle update;
the test now waits only when the Close reply still reports active workers.
These failures and corrections are retained in
`/tmp/deadpan-damaged-followup-20261008.log`,
`/tmp/deadpan-damaged-followup2-20261008.log` and
`/tmp/deadpan-damaged-followup3-20261008.log`.

The corrected focused run passed three native service tests and all sixteen
store backup tests. One passing store test was marked leaky by nextest;
the full milestone gate is the follow-up check. Coverage includes exact
quarantined bytes, replacement of the same package after Close, changed
project/backup refusal, explicit missing-manifest identity, and preserved
current workspace and stale-request refusal.

The debug replay at `/tmp/deadpan-damaged-visual2-20261008` passed 23 recovery
checks and the production shortcut audit's 21,884,016 routing cases against
62 Kestrel reservations. Executable SHA-256:
`0208b025d154d5fac0a45324716d5e28725bf321d2aee01dd13ebbfe6c4923b0`.
It exercises actual corrupt SQLite files and production service commands,
with only package selection scripted. Visual inspection then found that
the outer scroll region retained its initial short height after inspection,
hiding the heading while tabbing to Restore. Reserving height only on the
scroll area was insufficient: the first release run, at
`/tmp/deadpan-damaged-release-20261008/damaged-recovery`, caught the heading
still scrolling out when the second recovery had four backup rows. Its
executable SHA-256 is
`523ba4d70519b50de5ebdcfcb193117438ef42329c05466855fb749a3c83346b`.
The dialog now sets its own bounded height and keeps the heading, keyboard
help and Close outside the scrolling body. The replay checks the heading
and final actions together.

The full milestone gate passed formatting, strict workspace and UI-feature
Clippy, all 5,473 workspace tests (ten existing skips), all 1,092 UI-feature
tests (two existing skips), and both compile-fail documentation tests.
Workspace tests took 453.676 seconds; UI-feature tests took 234.230 seconds.
The log is `/tmp/deadpan-damaged-gate-20261008.log`. The target hygiene step
reclaimed old artifacts and incremental state, reducing the measured target
from 91.9 to 53.6 GiB; it still exceeds the configured 40 GiB threshold.

Two passing tests were marked leaky by nextest: the CLI's
`generation::runtime::launch::tests::ai_network_launcher_refuses_missing_isolation_and_invalid_python`
and the UI-feature app's
`transport::tests::original::offset_original_subrange_stops_before_out_and_keeps_leading_audio`.
Both passed serial isolated reruns without leak marks in
`/tmp/deadpan-damaged-release-20261008.log`.

The layout-corrected release at `/tmp/deadpan-damaged-release2-20261008`
passed damaged recovery (29 checks), ordinary backups (49), crash recovery
(13), and named takes (40), each with its separate shortcut audit. Its
executable SHA-256 is
`f5ee77130f1534e9e944b55b8877ad5028507c8904d53238a0723455542385b0`.
Inspected offscreen captures show the whole dialog at 960×640 and the
four-backup, typed-identity case at 1280×820, with its heading and final
actions visible. Strict UI-feature Clippy passed after that layout change.

Its separate performance run passed the same 29 recovery checks without
PNG capture. Input-frame CPU p95 was 1.026 ms across 26 inputs (maximum
1.234 ms); full UI-frame CPU p95 was 0.603 ms across 326 frames. Two
inspection waits completed within 4.053 ms, and two restore/open waits
within 49.300 ms. Those waits start after dispatch and are not end-to-end
input latency; two samples cannot qualify general backup performance. The
single general commit sample, 82.316 ms, is also insufficient to qualify
cached-edit latency. No timing sample failed or timed out.

## Native observations

The developer wrapper at
`/tmp/deadpan-damaged-native-20261008/Deadpan.app` used that release binary;
it is not a packaging qualification. Its disposable `recovery.deadpan`
retains real `cfr-bframes.mp4` media (120 frames, 30000/1001 fps, 320×180)
and a two-part split. The database was deliberately corrupted only after
closing and validating the copied project and creating a verified backup.

The native Command-O picker opened the fixture. Accessibility exposed the
failed-Open offer with Check selected backup focused. Enter verified it and
reported revision `79f52e08-0c90-4bcf-9376-d3862a312b2a`. Tab navigation and
Enter restored it; Accessibility showed Saved and the exact quarantine
path. Escape re-enabled the editor, and `l` advanced Edit boundary 0 to 1.
A separate read-only validation confirmed that exact revision, all 120
frames and a valid history. The retained quarantine's main file matched
the deliberate corruption bytes exactly. Command-Q closed the owned app,
confirmed by process inspection. JSON evidence is in that scratch root.

Native inspection also exposed a focus improvement: disabling Check during
verification removed it from the focus chain, so the next Tab started over
at the first backup. The reply now restores focus to Check; regression
checks require one Tab to reach Restore, or the typed-identity field when
needed. An initial compile of the new assertion held
its iterator borrow across a mutable harness check; capturing the result
first fixed it (`/tmp/deadpan-damaged-release3-20261008.log`).

The final release passed all 33 recovery checks and the shortcut audit in
both visual and performance modes at
`/tmp/deadpan-damaged-release3-20261008`. Formatting and strict UI-feature
Clippy passed. Executable SHA-256:
`9942617acc56a246d7e104971bbafd3e9c7ad2d6c8616ad503a01dcd7b36be3d`.
Input-frame CPU p95 was 1.191 ms across 18 inputs; UI-frame CPU p95 was
0.617 ms across 304 frames. Inspection waits were at most 3.904 ms and
restore/open waits at most 48.101 ms (two samples each). No timing sample
failed or timed out. One command-entry frame needed a second layout retry;
no discarded frame was painted. The preceding sample-size and dispatch
latency limitations still apply.

The developer wrapper then ran that final binary against
`focus-recovery.deadpan`, with both database and manifest deliberately
corrupted. Native Accessibility confirmed Check retained focus after Enter,
one Tab reached Confirm project ID, and typing
`fec5da87-ff71-4334-9774-04775c23884f` enabled Restore. Tab and Enter restored
the backup. Escape returned to the editor and `l` advanced boundary 0 to 1.
Read-only validation again confirmed revision
`79f52e08-0c90-4bcf-9376-d3862a312b2a`; the new quarantine retained the exact
damaged database bytes. Evidence is `validated-focus-after.json` in the
native scratch root. Command-Q closed the app, confirmed by process inspection.

The 18 changed/new Rust files have canonical path-to-SHA-256 map digest
`854e445fd82a16ff3753718bd316e5c18282f43acaa9f881d66ef186c15d5d3a`;
the map is `/tmp/deadpan-damaged-source-20261008.json` and its base is
`167300e4060f1da8e55054c6c06572a470db62ed`.

Physical keyboard/IME behavior, VoiceOver speech and physical power loss
remain owner checks under specification §29.1. The replay does not establish
them, and its offscreen images are not native screenshots.
