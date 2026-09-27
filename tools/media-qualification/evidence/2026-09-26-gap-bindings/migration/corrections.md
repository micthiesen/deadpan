# Migration check notes

- `core-initial.log`: 12 frozen binding adapter tests passed.
- Fixture was extended after initial production to exercise ordinary legacy WrapRepeat/SetRepeat adding positive gaps after the last InsertTime. Current SQL/provenance are from `producer-2/commands.json`, 9 revisions and 5 authored edits. The first producer output was superseded, and is retained only in scratch.
- `store-gap-initial.log`: actual v27 replay and 100 adversarial migration cases passed. Durable history test compiled its earlier body (which wrapped a new Repeat) while its included fixture already contained the added legacy Repeat; therefore captured 2 gaps rather than expected 1. The test now captures the fixture's existing `old-gap-owner` directly. This was a concurrent fixture/test edit during compilation, not a core or store behavior failure. Full store migration/persistence rerun follows after writers settle.
- `core-final.log`: all 12 frozen adapter tests passed after the fully streaming tagged-map guard and final projection whitelist.
- `store-final.log`: no tests started; `--locked` rejected temporarily unsynchronized concurrent app UI-harness manifest/lockfile. Parent synchronized existing lock metadata offline; `store-final-2.log` retries with `--locked`.
