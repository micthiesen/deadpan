# Typed storage failures and project scope

Source base: `62978a6f`, with the storage-error changes. Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg 8.0.3. Core format 47
and SQLite schema 75 are unchanged.

## Behavior

CLI, generation and Render workflow error wrappers now retain the actual
StoreError in their source chain while preserving their existing Display
text. The app explains those errors before crossing string-only service
mailboxes. Disk full, quota exceeded, read-only filesystems and denied
permissions keep their stable classification, original details and recovery
action. Typed live-command and public Render codes use the same explanation.
Standalone I/O and raw SQLite text do not establish a project save failure;
export-destination failures remain separate.

Editor feedback carries the affected project session. Failed Open or Create
of another package, and a failed reopen after damaged-database replacement,
do not label the retained workspace unsaved. Late operation receipts are
checked against their captured session before affecting the storage alert.
Generation recording, finishing and acceptance preserve storage failures.
The last durable revision remains current after a refused transaction, and
a later successful save clears the warning.

The sole working agent reviewed the complete change; no additional agent
was spawned under the user's sole-agent instruction.

## Verification

Ten focused UI-feature tests passed in
`/tmp/deadpan-storage-focused2-20261008.log`, including actual kernel EACCES
from another package's writer lock and real ENOSPC on a private APFS image.
The denied Open retains the current session/revision without a storage alert;
a later refused edit still raises its alert, a successful retry clears it,
and both packages validate. Existing recovery tests retain missing-media,
relink, crash-report and unchanged-history coverage.

The wrapper matrix checks ENOSPC, EDQUOT, EROFS and EACCES through CLI,
generation, Render, backup, live-command, Original media and migration
errors. These errno-construction cases qualify propagation, not a physical
quota or filesystem failure. Render tests distinguish project journal I/O
from external I/O. Two identity `.map_err` calls returned String rather
than Error and were removed; an initial test used rustix's nonexistent
`ACCES` spelling, corrected to `ACCESS`. Logs retain both compile failures.

The workspace gate passed 5,476 tests with 10 existing skips. The full
UI-feature run passed 1,092 tests and failed three exact-message assertions
because failed Open now includes the candidate path. Those expectations
were corrected without changing their no-write, no-backup or retained-work
assertions. The final affected set passed all 38 tests in each configuration,
including Open, recovery, damaged recovery, backups, Render and storage
classification. Final formatting, workspace and UI-feature strict Clippy,
and workspace doc tests passed. One analysis test in the full run and one
backup test in the affected run passed with nextest's leak warning; each
passed cleanly in an isolated serial rerun. The debug linker still reports
the existing large `__eh_frame` warning.

The gate was completed in stages. After the full workspace run, failed-Open
path context and clearing an old Render service failure before starting a
new workflow were added; the full UI-feature run and final affected tests
cover that source. Initial gate attempts also caught a UI-only test helper,
formatting, octal permission spelling and a test-module placement lint;
all were corrected. Logs retain those failures:
`/tmp/deadpan-storage-verify3-20261008.log`,
`/tmp/deadpan-storage-final-20261008.log` and
`/tmp/deadpan-storage-leak-rerun-20261008.log`.

The final release binary passed `storage-failure` (13 checks),
`damaged-recovery` (33) and `recovery` (13), each with the production Kestrel
shortcut audit. Reports are under `/tmp/deadpan-storage-release-20261008`.
The storage replay uses a real permission-denied candidate package, checks
the retained Saved header, then runs the refused-save/retry and unsaved
Camera close flow. The rendered Saved, Not saved and saved-again captures
were inspected. Reports retain layout warnings for command-footer second
retries and, in recovery, three consecutive input frames with different
retry causes; there are no failed checks or ignored-retry paint failures.
Native picker selection, physical presentation, VoiceOver, OS IME delivery
and device audio are outside these replays.

The executed app SHA-256 is
`cd6209a9584f7c710d118e51924b0d8de277f9e1ea61a69ea2e9ec62f7c5bcf4`.
Helper hashes are retained in `binaries.json` beside the reports. The
22 changed Rust source files are recorded in
`/tmp/deadpan-storage-source-20261008.json`; its sorted compact file-hash map
has SHA-256
`ff50796d16d9875d6f737580cc038b89ba17f479f39b8268b745dca9ddab2f78`.
This is local release-binary evidence, not packaged or clean-machine
qualification. DP-01 remains partial while system Quit and final recovery
acceptance are addressed.
