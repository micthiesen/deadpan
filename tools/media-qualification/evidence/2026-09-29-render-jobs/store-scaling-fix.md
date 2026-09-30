# Render store scaling fix

Source-only changes complete. No builds, tests, formatters or native execution run by this worker.

## Runtime work

All ordinary render APIs now call `validate_runtime`, which reads only table counts, the two expected index definitions and the scalar active-attempt set. It does not read attempt JSON or use JSON extraction. Affected job head/ordinal checks are scoped to that job and read only scalar indexed columns.

Job/attempt/checkpoint readers use SQL `CASE` byte/type predicates before returning JSON or identifier strings to Rust. Invalid or oversized targeted fields become null and produce RenderJobInvalid before deserialization. Pages bound their selected identifiers and deserialize at most their selected rows, plus a checkpoint's directly relevant encoding owner. Checkpoint owner validation cannot recurse through arbitrary retries: the owner must directly own its own checkpoint.

Full `validate_metadata` remains in open/explicit store validation. It keeps all-table size and semantic checks, duplicates/head/ownership validation, and report validation. Writable recovery relies on that completed audit and re-reads only the active attempt.

## Full document audit

Full audit caches `(project ID, duration, SHA-256)` by retained RevisionId; only these small bindings are retained, not documents. Multiple jobs for one revision share one canonical hash. The previous aggregate 30-second deadline is removed.

New `deadpan_jobs::render::document_sha256_for_validation` uses the exact same private bounded writer as `document_sha256`. The control enum selects uninterrupted retained-document validation or the existing caller cancellation/deadline. Both retain `serde_json::to_writer(ProjectDocument)` and MAX_DOCUMENT_JSON_BYTES. Ordinary worker hashing keeps its caller controls.

## Regression coverage added

- Existing pure canonical-hash test compares the uninterrupted entrypoint to the controlled helper and preserves cancellation/deadline assertions.
- `targeted_operations_skip_unrelated_report_but_full_audit_rejects_it`: creates a large historical verifier report and a fresh attempt, corrupts only the historical report's type as a deterministic deserialization witness, then proves job reads/pages, selected attempt reads/pages, a selected transition and new job allocation succeed. Targeted access to the corrupt report, explicit full validation and both reopen modes still reject it.
- `targeted_json_bounds_reject_before_deserializing_the_row`: injects oversized invalid JSON, checks the stable RenderJobInvalid bounds error rather than serde parsing, and verifies unrelated job inspection still works while full audit rejects it.

No CLI, migration tests, schema or storage-media files edited during this follow-up. Parent's LowerHex fix in store tests retained; the analogous pure hash assertion now also uses explicit hexadecimal bytes.
