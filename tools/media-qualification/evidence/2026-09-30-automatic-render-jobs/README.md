# Durable automatic render evidence

See the [qualification report](../../../../docs/qualification/automatic-render-jobs-2026-09-30.md).

- [Summary](summary.json) and [review](review.md): exact coverage and remaining work.
- [Automatic workflow](automatic-workflow.json): fresh encode, reopened checkpoint retry,
  reconciliation and cold retry through the native app helper.
- [Initial independent readers](independent-full-file.json) and
  [cold retry readers](independent-cold-file.json): every picture plane and complete
  authored audio in both files at fixed PTS, without alignment or gain changes.
- [Publication identities](published-byte-identities.json): retry/reconciliation
  retain the initial movie bytes. The separately encoded cold file is also decoded.
- [Database preservation](database-preservation.json): every previous cell in 19 tables
  survives; two fresh encoding attempts own their respective immutable decisions.
- [Runtime files](host.json): independent post-encode match to all five observed images.
- [Native archive](native-files.tar.gz) and [inventory](native-files.json): exact movies,
  reports, canonical references, decoded bytes and consistent synthetic SQLite backups.
  Every archive member was rehashed. No helper or library binary is committed.

Source inventory: `b08c28b724091e2925872b0c55d35c478bf566e9f4ca2e858471265aa84de7fb`. Per-command journals retain commands, revisions,
source hashes, exits and elapsed times. The full workspace run exited 101 on one
outdated migration expectation; the focused correction passes. Final distinct
coverage is 2,439 tests in 171 groups, plus
310 app tests under the optional UI-harness feature. Failures and their
diagnoses remain retained. Passing unrelated suites were not repeated.

Scripts retain the original scratch paths. Remove `.txt` from saved script names
in scratch before reproduction. The canonical runner pins the FFmpeg prefix.
The five-second independent reader adapters are reused unchanged from the prior
runtime qualification; their original snapshots, edits and hashes remain included.
These results do not establish public Render or full-product acceptance.
[Manifest](manifest.json) hashes every other file here.
