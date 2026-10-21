# Source crop carrier evidence, 2026-10-20

Base production: d0f675a5. Progress-only commit fb70355c was pushed during verification; it changes no Rust or dependencies.

Authoritative final runs: core.log (18 focused core/semantic checks), pcm.log (9 actual PCM checks), check.log (format, strict Clippy, all 2375 core/plan/audio tests, none skipped), export.log/export-results.json (19+34 pictures and two complete zero-offset audio windows). source-identities.json binds the final source and Cargo.lock; test-binary-hashes.json and export-binaries.json pin executed binaries.

Authoritative old-code runs use the corrected natural-rate fixture: retained-natural-old.log and fresh-natural-old.log reach expected core refusal; neighbor-natural-old.log passes its negative control. pcm-old.log passes the first retained positive-offset bounded-silence prefix, then fails public admission; the neighbor renderer/control test passes. Do not infer that the failed test executed subsequent loop cases. reconstructed-old contains test sources verified against pre-execution hashes; old production source is d0f675a5.

pcm-support-boundaries-second.log and partial-trace.log expose the later real enclosing-read failure on the retained silent Hold. Final production fixes it with a narrow typed suppressed-silence return. Neighboring Source and RoomTone controls retain strict transfer admission.

Early *-old.log prototype runs, design.md, core-proposed.patch, pcm-proposed.patch and support-boundaries drafts are superseded fixture/proposal records. Some failed because fixture admission, natural mapping, allocation or disjoint-range assertions were wrong. They are retained to explain corrections, not to qualify final behavior. candidate-zero-review records a retracted suggestion: candidate allocation must remain the full physical allocation.

No million-frame full export, acoustic, new UI or performance claim. The next-repeat-family research is deliberately excluded from this increment. Binary files remain in private scratch; reproducible source identities and binary hashes are retained here.
