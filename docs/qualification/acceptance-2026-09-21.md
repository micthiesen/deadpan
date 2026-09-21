# Durable acceptance qualification, 2026-09-21

The dedicated store API turns explicit acceptance of a selected Ready bundle
into one atomic asset/provider/history edit. Qualification and Ready alone leave
the document unchanged. [The contract](../GENERATION_ACCEPTANCE.md) describes
selection, revision, relevance, occurrence and dependency checks.

[Recorded evidence](../../tools/model-qualification/evidence/2026-09-21-durable-acceptance/)
contains configuration, reports, source hashes, gate logs and sanitizer results.
The implementation starts from `a9555c6442c44a3e36b516bea442234329871606`; the source
manifest identifies the tested changes. No images, weights, generated footage or
project database are committed as qualification evidence.

## Real-media acceptance and readback

The probe reuses the actual protocol-2 LTX-2.3 q4 result from
[retained conditioning qualification](conditioning-2026-09-21.md). It reloads the
pre-launch input archive, qualifies native footage/provenance, derives both FFV1
masters and records envelope 3 with measured source spans. No inference was
repeated. Hardware is Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1 and the
qualified LGPL FFmpeg 8.0.3 developer prefix.

`qualify_generated_acceptance` creates a synthetic Hold project, publishes all six
objects, records Ready, previews, explicitly accepts, relocates the package,
reopens, undoes/redoes acceptance, reverts to Background and reads all six objects.
Ready and preview preserve the original document. Acceptance matches preview;
undo removes newly registered assets and redo restores them. Final revision is
`revert-to-fallback`. This took 7.902 seconds, including conversion/storage checks.
One uncontrolled developer run is not an interactive latency benchmark.

| Master | Frames/rate | Last PTS/duration | Measured source span |
| --- | --- | --- | --- |
| Native | 25 at 24 fps | 1000/41 ms | `[0, 1041)` ms |
| Sampled | 30 at 30000/1001 fps | 968/33 ms | `[0, 1001)` ms |

Report 2 records the actual final decoded duration. Exact frame counts/rates and
sampling remain separate from these container bounds. Object byte lengths and
BLAKE3 identities are retained in `acceptance-report.json`.

Independent developer FFmpeg decoding, with no model/runtime paths in its
environment, recovered 18,432,000 native RGB bytes and 22,118,400 sampled RGB bytes.
Their SHA-256 values match host qualification:

- Native: `be4ffce12b048457bcab1779eefe8683f41c242d013be7f488c25416491efe1a`.
- Sampled: `eda362044e57353435b56756be4143419a3bb6ed52bb2f849102b3eff962c71a`.

This proves readable retained media through the tested history operations. The
external decoder is QA tooling, not an integrated Deadpan renderer. The native
integration test separately removes worker input/output files after acceptance.

## Automated verification

The exact repository gate passed: formatting, workspace Clippy with warnings
denied, 339 Rust tests with no failures or ignored tests, locked workspace build,
and CLI doctor reporting database schema 9/core schema 5. Audio/model harnesses
passed 20/54 Python tests. The initial gate found a fixture-function name shadowed
by a test variable; the corrected final run is recorded alongside that failure.

ASan/UBSan passed all 18 native-worker tests in 42.156 seconds, including real
bundle acceptance. Instrumentation covers the C adapter and target C dependencies;
Rust and separately built FFmpeg libraries are not instrumented.

Store tests exercise missing/corrupt objects, changed selection/receipt/revision,
context mismatch, incomplete reconciliation, alias conflicts, read-only preview,
transaction rollback, undo/redo/revert/new branches, stale/detached non-revival
and occurrence isolation. A genuine schema-8 fixture from the base revision
preserves JSON, operational metadata and pending redo. Old receipts stay
unqualified; new fields reject migration without promotion and preserve backups.

Independent general, storage/migration and media-timing reviews reported no
findings. Root reviewed the complete diff and verified the tested source hashes
still match the checkout. [Review scope and conclusions](../../tools/model-qualification/evidence/2026-09-21-durable-acceptance/review.json)
are retained with the evidence.

## Remaining qualification

The fixture context resolver echoes an unchanged captured context because this
probe changes only provider/history. Application source-clock resolution, decoded
conditioning/color meaning, installed-model attestation, useful motion, joins,
audition and speech preservation remain open. Request bindings require one
effective Hold occurrence and reject Retime ancestors pending crop resolution.

History dependency inventory, cleanup, portable copy, app scheduling/rendering,
clean-machine distribution, physical power loss and Linux runtime behavior remain
unqualified. No native lifecycle or UI changed, so startup, GUI aesthetics,
focus/IME, accessibility and keyboard navigation were not repeated. These remain
required for the interactive workspace. All product requirements and delivery
gates retain their existing open/partial status.
