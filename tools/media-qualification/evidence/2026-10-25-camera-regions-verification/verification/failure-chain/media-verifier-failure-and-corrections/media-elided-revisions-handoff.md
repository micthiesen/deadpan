# Media supported elided-checkpoint correction

Scratch-only media.py updated after read-only inspection of native-regions-debug-third/report.json and its retained SQLite database. No Cargo, CLI command, detector/native replay, render, verifier or media.py execution occurred. The report is debug evidence for shape inspection only; the release-only build_profile assertion remains unchanged.

Observed storage:
All11 normal edit,2Redo and4Undo rows have revisions.document set to literal JSON null; only the initial checkpoint is materialized. This is valid Store elision. The prior driver incorrectly required a full raw head/from/to document and would falsely stop before media qualification. Raw elided bytes and all other columns remain included in the exact eight-table authority fingerprint.

Correction:
- authority returns an optional stored checkpoint, with no claim that it is the reconstructed head.
- The normal production CLI dump remains the authoritative reconstructed document and must exactly equal the native final durable actual snapshot and SQLite head revision.
- A non-elided head checkpoint must additionally match the dump.
- The active cursor's actual history patch must add exactly the native accepted target and change exactly the selected owner's framing, preserving full kind, all other content and zero duration delta.
- The native full durable snapshot immediately before atomic region:N is now explicitly required and bound to patch.from_revision. Active edit revision binds patch.to_revision. Its complete prior nodes/targets and final native/CLI content must match the patch, except Redo's fresh revision header.
- If either prior/committed raw checkpoint exists, it additionally must agree. Null checkpoints remain legitimate and byte-for-byte conserved. No Python history replay or replacement Store reconstruction was introduced.

Other current assumptions reviewed against the actual successful debug report:
All22 required rows exist/passed, all3 exclusions match exactly, final root is a single Source with expected Follow target/6/5 scale/no transition, actual atomic target matches final document, source picture span/mapping/timebase fit measured CFR manifest, separate complete nonzero WAV event and native audio support agree, eight authority tables exist, actual active patch is exactly one target plus one node and matches both full durable snapshots. None of this bypasses the runtime release-only guard. Fresh CLI detection/output-verifier assertions remain unexecuted.

Evidence:
media-elided-revisions-static-review.json records the read-only observations and report hash; media-elided-revisions.patch is the exact delta; frozen .py.txt and sourceApp .rs.txt snapshots are retained. Prior0abf5d55 driver and exclusion/preparation review artifacts are preserved verbatim in media-elided-revisions-before, in addition to all earlier original records.

This is a pre-runtime verification-driver correction, not a product failure. Root should review the new driver pin and then execute only with the successful release retry's retained project/report/receipt and matching frozen five-binary identities, using a fresh output directory. The15 operation, measured provider clock, readonly DB/WAL, full PCM zero-offset, portable/history/checkpoint and evidence-scope contracts are unchanged.
