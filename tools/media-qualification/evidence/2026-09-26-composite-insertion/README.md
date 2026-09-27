# Composite insertion evidence

See `docs/qualification/composite-insertion-2026-09-26.md` for scope and results.
`verification.json` and `sha256.json` identify retained commands, source manifests
and reviews. Failed checks remain failures; this is not release qualification.

The public-command script uses only disposable Background/Silence projects.
It exercises the built app's shared headless entrypoint without seeded snapshots
or rewritten history. PCM tests independently exercise decoded fixture audio.
The actual older-binary fixture and its producer/provenance remain under
`crates/deadpan-store/tests/fixtures/v29-composite-insert-history.*`.

The attempted UI replay retains its missing-Metal failure and passing shortcut
audit. It produced no editing image or visual assertion. The earlier completed
concurrent harness, host-run images and performance misses remain in
`tools/ui-feedback/evidence/2026-09-26` under their original build identities.

Git metadata is read-only in this session. No commit or push was made. A complete
tracked patch, untracked archive and hash manifest are checkpointed in task
scratch after validation, including the completed harness and ImageGen boards.
