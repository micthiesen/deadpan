# Real process-crash qualification example

No build, formatter, tests or native execution run by this source worker.

Build the auto-discovered example `qualify_publication_recovery` and invoke:

```
qualify_publication_recovery PACKAGE JOB_ID ENCODING_ATTEMPT WORKER NEW_OUTPUT_DIRECTORY
```

Arguments for each fresh copied schema-40 fixture:

- source: `JOB_ID=durable-structural`, `ENCODING_ATTEMPT=structural-encode-1`
- generated: `JOB_ID=durable-generated`, `ENCODING_ATTEMPT=generated-encode-1`
- `WORKER` is the current `deadpan` worker executable.
- `PACKAGE` must be a caller-prepared clone under canonical `/tmp`, and output must be a new sibling directory under `/tmp`. Do not pass the original retained source evidence packages.

The parent mode captures all exact authoring/history cells, performs a real backed-up schema 40 to 41 migration, verifies no authored cell changed, and launches itself as ten child processes through `deadpan_native_process::spawn`. Every child freshly verifies the same retained checkpoint under a new attempt, then executes the actual staged publication APIs. It writes a bounded child-state JSON and pause marker only after its selected actual stage has completed, and blocks on parent-owned stdin. Parent checked group teardown sends real SIGKILL, confirms cleanup, and reaps once. Early child exit, excess output, and pause timeout follow the same owned cleanup path. No PID from persisted metadata is signalled.

Cases:

1. intent recorded: NotPublished
2. sealed files recorded Prepared: NotPublished
3. report authorization recorded: NotPublished
4. report renamed before recording ReportCommitted: NotPublished
5. report recorded: NotPublished
6. movie authorization recorded, before rename: Unresolved
7. movie commit returned, before terminal journal record: Published
8. terminal Published recorded: Published
9. movie commit returned, then parent moves report aside after kill: PublishedUnconfirmed
10. movie commit returned, then parent preserves original and creates an identical-byte/mtime replacement: Unresolved

After each kill, a read-only store reopen must exactly preserve the pre-crash record. Writer reopen interrupts an active record or preserves terminal Published. The parent freshly verifies the checkpoint again and calls explicit reconciliation, retaining recovered locks through the final journal transaction. Before/after file inventories establish that reconciliation did not rename, remove, replace, or change any publication remnant. A second writer reopen confirms the terminal state. Every case compares exact authoring/history cells.

Expected passing corpus per package: 10 SIGKILL cases; 20 fresh verifications; zero encodes; four movies renamed before kill. Classifications: 5 NotPublished, 2 Unresolved, 2 Published, 1 PublishedUnconfirmed. Counts are reported only after every case passes.

Outputs include `report.json`, `case-<name>.json`, `authored-before.json`, `authored-after.json`; each case subdirectory retains child stdout/stderr, child-state JSON, every movie/report/partial, and deliberately displaced artifacts. JSON contains full persisted identity evidence, reports, attempts, phase/sequence values, operation histories, artifact paths/metadata/SHA-256/length, and original exact authored rows. Parent can independently decode retained output paths. Report writes are bounded to 32 MiB; authored cells to 16 MiB; artifact count to 16 and each file to 512 MiB; verification progress to 4096; child pause wait to 240 s and verification work gets 120 s deadlines.

Limits: this qualifies process death between completed host API calls, not death inside SQLite/rename/fsync nor a power cut. Nested verifier processes have completed teardown before the child pause signal. Groups do not contain processes that escape their group. If checked group and leader cleanup both fail, the harness reports failure rather than waiting without a bound or using an unchecked PID. Independent media content comparison remains external to this harness. Caller clones must preserve immutable media modes and use SQLite backup for any live database copy.
