# Independent publication durability review

Scope reviewed: new publication journal/store modules and durability hook, schema-40-to-41 migration hooks, `deadpan-jobs` publication declarations, the CLI journal/staging/recovery paths, and the native APFS identity adapter. Review was read-only; no builds, tests, formatters, or native programs were run.

## Findings

No concrete correctness findings in the assigned lens.

## Checks and evidence

- A publication phase transition commits its SQLite transaction, updates the live epoch, and issues a `PublicationPermit` only after the package/database/WAL identity checks and durability barrier succeed (`crates/deadpan-store/src/publication.rs`, `publication_ack`; `crates/deadpan-store/src/publication_durability.rs`, `barrier`). A barrier failure revokes all epochs and fences further writes until reopen.
- The CLI requires the exact durable phase permit before report and movie renames (`crates/deadpan-cli/src/encoded_render/publication/journal.rs`, `PreparedPublication::commit_report` / `commit_movie`). `commit_guarded` checks the permit immediately before `RENAME_NOREPLACE`; after a successful rename, later sync/readback failures are classified as published-but-unconfirmed (`staging.rs`, `commit_movie`; `filesystem.rs`, `commit_inner`).
- Process loss before a terminal SQLite transition leaves an active operation. Writable reopen records it as `Interrupted`; a fresh reconciliation verifier must refer to a later verification attempt for the same retained checkpoint (`crates/deadpan-store/src/publication.rs`, `recover_nonterminal` / `begin_publication_reconciliation`). For `MovieCommitting`, recovery cannot claim NotPublished (`finish_publication_reconciliation`).
- Recovery hashes and syncs only entries whose persisted directory and file identities match; it opens with no-follow semantics and does not rename, unlink, recreate, or adopt arbitrary names (`crates/deadpan-cli/src/encoded_render/publication/filesystem/recovery.rs`; `journal.rs`, `reconcile`). Initial creation is exclusive, and publication uses no-replace rename.
- Permits are invalidated by sequence/epoch changes, cancellation, failed durability barriers, and store closure. Migration creates only the new publication tables for schema 41; schema-40 project cells remain in the consistent candidate/backup path, with a dedicated all-cells preservation test present in `crates/deadpan-store/tests/migration/publication.rs`.
- I initially considered whether `finish_publication(Failed/Cancelled)` could falsely close a `MovieCommitting` record. I dropped it: the journal host's `commit_movie` returns `Err` only before a successful rename and maps every post-rename failure to `PublishedUnconfirmed`; a lost process/result remains active and reopens as `Interrupted`, where MovieCommitting cannot be declared NotPublished.

## Limits

SIGKILL/restart tests establish process-death behavior, not power-loss behavior. The source explicitly disclaims resistance to malicious same-user filesystem interference. I did not infer either guarantee from the code or tests.
