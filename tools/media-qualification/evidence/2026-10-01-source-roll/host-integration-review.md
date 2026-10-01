# Source Roll host integration review

Scope: read-only review of the integrated store/CLI Roll host path, receipt checks, current test wiring, and core41/DB50 version gates. No checkout edits, Cargo, or native execution.

## Findings

No actionable findings.

## Review notes

- `ProjectStore::preview_source_roll` captures one revision-bound snapshot. Zero/clamped-zero previews still validate unused revision ID, timing allocation, wrapper absence, both source qualifications, asset records, and Original ownership. Nonzero previews use the shared prepared-command admission path; the same validator is wired into ordinary preview/commit preparation.
- Candidate validation checks both resolved sides against retained qualified Originals, exact full measured video spans, physical Source before/after values, literal parent slots, wrapper need/identity, and unchanged duration. Wrapper fields and editorial side flags are checked against the resolver result. The CLI `execute_short` dry-run branch returns that same preview, while commit uses the common store transaction.
- Store and CLI test modules are wired in their parent `source_registration.rs` test files. Store coverage supplies two independently retained MP4 Originals, asserts literal seam/output/clamp values (`[-2, 2]`, six output frames, seam 3→5 or 3→1), corrupts each side's qualification, ownership record, and asset row while the other side remains valid, and verifies exact database/history/redo/receipt rows are unchanged. An injected failure after history insertion verifies rollback and retry of the same request. Durable Undo/Redo is checked across reopen.
- CLI coverage compares cold and live preview JSON, exercises cold/live commit paths, pins the same numeric clamp/output expectations, verifies zero preview and invalid metadata are write-free, checks stale request rejection, and exercises durable Undo/Redo. These checks supplement the prior independent core/picture/audio Roll review; this report makes no runtime claim.
- Current versions are core document 41 and DB 50. DB schemas 39–49 are refused before acquiring a writer or creating backups; schemas 1–38 remain migration-required. The development-break test enumerates the refusal range and checks stored rows/package entries remain unchanged.
