# Durable automatic render qualification

The [shared render workflow](../RENDER_JOBS.md#automatic-admission-and-recovery)
now connects fresh automatic encoder admission to durable jobs, retained
checkpoints and verified publication. Database 42 stores the algorithm policy
with the job and a bounded immutable decision with each original encoding
attempt. Qualification stays Queued. Only an atomic decision/Encoding transaction
authorizes the worker to consume its live admission.

Cold encoding retries qualify again. Checkpoint retries and destination
reconciliation load their original decision and freshly verify the retained
movie. Stored evidence cannot recreate a live encoder capability. Automatic
manifest and inner publication-provenance schema 2 bind the exact intent,
decision, rejected probes, frozen controls and observed runtime. Engineering
schema 1 retains its original closed grammar and serialized shape.

## Native workflow and independent files

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and pinned LGPL
FFmpeg 8.0.3. A fresh SQLite backup of the synthetic project renders immutable
revision `workflow-live-restored`, document SHA-256
`564af137ff203f7a10344a9e50db7a4167cee0e1dfbbecf44ed3b39a3c6ef638`.
The complete range is `[0,128)` at 320x180 and 30000/1001, with 205,005 authored
audio sample frames. The runtime is the native app executable, with no argument
or environment overrides.

The real run passes two fresh automatic encodes and four fresh verifications:
initial publication, checkpoint retry after reopening, reconciliation of the
initial destination, and cold re-encoding. Retry and reconciliation do not enter
qualification or encoding. Cold retry uses fresh probe identities and leaves the
original decision unchanged. Both encodes retain the typed hardware B-frame
PTS-before-DTS rejection and select hardware without B-frames. Each selected
probe checks 46 pictures and 73,674 audio sample frames before project encoding.

The two 32,553-byte published files have SHA-256:

- Initial: `9c1d0c7a27a6db4aa7b3a0c9c3a3920db2dd22a1759f811e7d56674b6e9340e1`.
- Cold retry: `07a7e5b8674253eeb804c9b15e137a9d53875934ee89c774fe410c82586fe4ae`.

Checkpoint retry and reconciliation retain the initial movie identity and
original decision. Both distinct files pass independent FFmpeg picture decoding
and ordinary/manual FFmpeg plus AVFoundation audio decoding. Every one of the
768 complete picture planes passes the unchanged fidelity bounds; the largest
code error is 42 against the 48-code maximum. Each audio reader passes all
205,005 authored samples per file at observed absolute PTS. Physical sample counts
are 205,824, 206,848 and 205,005 respectively, with priming/padding retained.
Maximum audio error is 0.064974 and maximum RMS error is 0.000283, below the
unchanged 0.25 and 0.02 limits. No PCM alignment or gain adjustment is applied.
The previously reviewed five-second reader adapters are reused byte for byte;
all 14 retained source/evidence files match the prior qualification.

An independent host read matches all five runtime backing objects to the captured
observations. The native helper SHA-256 is
`fc61e7d42d3aad15e59f6e9f662d9b7c458c203d315c368886914aeec57c999b`.
A consistent post-run SQLite backup preserves every existing cell in all 19 old
tables. Authoring/history are unchanged; only new render operational rows and
the two original-owner decisions are added.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-30-automatic-render-jobs/README.md)
includes both files, fresh canonical references, decoded bytes, published reports,
consistent project backups, failed runs, reviews, scripts and per-command source
inventories. Every archive member was rehashed. The final qualified inventory is
`b08c28b724091e2925872b0c55d35c478bf566e9f4ca2e858471265aa84de7fb`.

## Persistence and failure checks

Migration from an authentic schema-41 database preserves every prior cell,
including 3 jobs, 26 attempts, 2 checkpoints, 12 publications and 23 publication
operations. Its consistent SQLite backup has SHA-256
`4d1d4ccb20e499132ac5160e52a162c5bfeb199d1748a91332fb8699b16dd30a`.
The retained SQL fixture is produced without rewritten cells. Tests cover a
live WAL reader, preserved backup, new-table collisions, bounded records and
automatic vocabulary injected into old jobs or nested publication intents.

Store tests cover atomic rollback, stale transition identities, immutable
decisions, output/document mismatch, failed/cancelled admission, reopen and
checkpoint ownership. Full audit reuses validated job heads and compact revision
summaries; deterministic counters cover histories growing from 6 to 96 decisions.
Targeted reads still reject corrupted allocation heads and output bases.

Workflow tests require the decision commit before Encode is sent. A failed SQL
insert cannot start encoding. Cancellation after selection discards the unused
admission; a captured failed qualification retains its actual terminal reason
when cancellation races. Unconfirmed cleanup keeps the owner slot fenced.
Store-owner revocation also reaches a real controlled child: the host sends
cancel, drains its terminal protocol and confirms cleanup before returning.

The native executable dispatches direct private picture, encode, verify and
probe worker calls before creating a window. Tests cover all four entrypoints.
The normal headless wrapper remains available. No visible editor layout changes.

## Workspace verification

The locked full workspace invocation completed every target and doctest with
2,438 passing tests and one failed historical migration expectation. The latter
compared the new empty decision table's name as though it were an old cell.
The failure reproduced; removing that single marker made both comparison arrays
identical in order. The corrected test separately asserts the new table is empty
and preserves exact comparisons for every old cell and backup.

The affected publication checks pass after that test-only change. Final distinct
workspace coverage is 2,439 passing tests in 171 result groups, with no remaining
failure or ignored test. The original exit-101 log remains retained; unrelated
passing targets were not repeated. Earlier compilation failures in new test
bindings and the qualification example, plus a reproduced retry-fixture lifecycle
failure, are also retained. Independent review found and corrected an audit cost
regression and a serialization sink that could grow past its bound. No review
findings remain.

Strict workspace Clippy, formatting, all 310 optional UI-harness app tests and
their strict Clippy pass. Native Metal startup, window creation and the shutdown
callback pass. A subsequent qualification-example correction changes only its
report comparison, as described below; production sources and these passing
tests remain unchanged.

The first real automatic workflow verified and published its movie, then the
example rejected two JSON representations of the same f32 values. The stored and
published intent/decision JSON match exactly. Seven peak values use different
decimal spellings but identical f32 bits. The example now decodes the closed
typed DTOs and compares exact fields after checking the report against its
receipt's size and SHA-256. There is no tolerance. That fix passed independent
review and strict all-target Clippy; the failed native evidence remains retained.

## Scope

This implements the shared durable automatic SDR boundary. Native Render
controls, public headless rendering, complete mastering/effects, HDR, expanded
raster/content/runtime qualification and release acceptance remain open. No DP
requirement or delivery gate is promoted. The runtime evidence identifies mapped
backing objects under trusted installed code; it does not attest resident memory,
OS frameworks, drivers or hardware.

Sanitizers, physical listening, painted UI/accessibility and GUI performance were
not rerun. These measurements cover one synthetic SDR project on one host. Tiny
raster failures from the prior admission qualification remain unresolved.
