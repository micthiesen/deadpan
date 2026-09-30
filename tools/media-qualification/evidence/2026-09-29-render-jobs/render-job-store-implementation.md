# Render job store implementation

Source-only implementation complete. No builds, tests, formatters, native runs or commits were run by this worker; the parent owns them.

## API

`deadpan_jobs::render` exports:

- `RenderIntent { schema_version:1, job_id:RequestId, project_id:ProjectId, revision_id:RevisionId, document_sha256:Sha256, range:FrameRange, policy:RenderEngineeringPolicy }`.
- Policy has `schema_version:1`, `selection:RenderSelection::ExplicitEngineering`, `encoder:RenderEncoder::{Hardware,Software}`, `b_frames:RenderBFrames::{None,TargetTwo}`. Native contract reconstruction remains in the host.
- `document_sha256(&ProjectDocument,&AtomicBool,Instant)->Result<Sha256,RenderError>` preserves `serde_json::to_writer(ProjectDocument)` and `MAX_DOCUMENT_JSON_BYTES`. Error variants: Invalid, Cancelled, Deadline, Json.
- `RenderAttemptIdentity { job_id, attempt_id, cancellation_token, expected_sequence }`.
- `RenderAttemptState`: Queued, Encoding, EncodedRetained, Verifying, Verified, Cancelling, Cancelled, Failed, Interrupted. `is_terminal`, `as_str`, pure `validate_transition`.
- `RenderDiagnostic { code:String, detail:String }`.
- `RenderVerificationObservation { schema_version:1, validator_id:String, validator_version:String, movie_sha256:Sha256, movie_byte_length:u64, report:serde_json::Value }`. Report is bounded at 256 KiB, object only; this is historical evidence and grants no live verifier authority.

`deadpan_store::render_jobs` exports:

- `BeginRenderAttempt { job_id, attempt_id, cancellation_token, checkpoint_attempt_id:Option<AttemptId> }`.
- `StoredRenderAttempt { job_id, attempt_id, cancellation_token, ordinal, transition_sequence, state, checkpoint_attempt_id, cancellation_requested, diagnostic, verification }`. `identity()` binds the next exact mutation. Selected checkpoint always names the original encoding attempt.
- `StoredRenderCheckpoint { job_id, encoding_attempt_id, media:RenderCandidateMedia }`.
- `RenderAttemptTransition::{Encoding,Verifying,RequestCancellation,FinishCancelled,Failed(RenderDiagnostic)}`. FinishCancelled and Failed are host terminal observations after owned work has stopped. Unresolved cleanup remains Cancelling; Cancelling->Failed retains cancellation intent.
- `ProjectStore::create_render_job(intent,cancelled,deadline)`, `begin_render_attempt(input)`, `transition_render_attempt(&identity,transition)`.
- `retain_render_checkpoint(&identity,&PreparedRenderRetention,cancelled,deadline)` checks exact token identity plus session/descriptor freshness before transaction and before commit. No file hashing on writer.
- `record_render_verification(&identity,observation)` compares exact checkpoint movie identity, requires Verifying, records terminal historical Verified.
- Inspection: `render_job(&RequestId)`, `render_jobs(after:Option<&RequestId>,limit:u32)`, `render_attempt(&RequestId,&AttemptId)`, `render_attempts(&RequestId,after_ordinal:u64,limit:u32)`, `render_checkpoint(&RequestId,&AttemptId)`.

Capacities: 4096 jobs, 65536 total attempts/checkpoints, pages 1..256; job JSON 8 KiB, attempt JSON 272 KiB, checkpoint references 8 KiB. IDs/tokens are unique across all render attempts in the project. One active attempt per project enforced by an explicit unique index and independent checks. SQLite counters are checked; active attempts retain one final sequence for recovery. Every public operation validates bounded operational metadata. Full open validation also rehashes captured documents under a shared 30-second metadata validation deadline.

## Wiring and migration

Store schema 40 adds only `render_jobs`, `render_job_heads`, `render_attempts`, `render_candidate_checkpoints`; core remains schema 33. Schema 39 uses `validation::validate_history`, with no authored replay or JSON rewrite. Earlier versions use unchanged migrate_history adapters, then receive empty render tables. CREATE collisions reject the candidate before promotion.

Store owns render_storage/render_closed Arcs, closes on Drop, initializes `Media/RenderCandidates` for new packages. Render media errors map through StoreError::RenderMedia; metadata errors use RenderJobInvalid. `ProjectStore::open` validates first, then writer-only recovery marks nonterminal render attempts Interrupted without touching checkpoints/cancel intent or authored state. Read-only opens do not recover.

## Added checks for parent to run

- jobs src/render/tests.rs: exact canonical hash, cancellation/deadline, strict versions/fields, lifecycle/checkpoint and cancellation rules, report bounds.
- store tests/render_jobs.rs: captured historical intent through edits/undo/redo; authored history and pending redo unchanged by operational writes; stale tokens/sequences/foreign identity; one active project attempt; fresh IDs/tokens/high-water; read-only vs writer recovery; unsupported/wrong intent; paging; missing indexes/malformed states/reports; durable checkpoint interruption/retry; exact report identity; foreign/stale/modified retention guards; oversized evidence; checked transition exhaustion; cancelling failure retains checkpoint/cancel intent.
- migration/render_jobs.rs: all three authentic v39 fixtures preserve every cell of every old table byte-for-byte; backup stays v39; render tables empty; vocabulary collision rejects migration.

No fixture SQL or object_storage/render_media code edited by this worker. No publication table, authored command, or native UI was added.
