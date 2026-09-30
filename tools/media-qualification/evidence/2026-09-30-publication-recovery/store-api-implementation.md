# Exact implementation API

Protocol path `deadpan_jobs::render::publication`:
- `PublicationIntent { schema_version:u32, publication_id:RequestId, job_id:RequestId, verified_attempt_id:AttemptId, destination:PathBuf }`; `.validate()`, `.movie_name()->Result<&str,RenderError>`, `.report_name()->String`, `.movie_partial_name()->String`, `.report_partial_name()->String`. Report name `deadpan-render-<publication_id>.json`; partial names `.deadpan-<publication_id>.movie.partial` and `.deadpan-<publication_id>.report.partial`.
- `PreparedPublicationEvidence { schema_version:u32, movie_sha256:Sha256, movie_bytes:u64, report_sha256:Sha256, report_bytes:u64, contains_generated_pictures:bool, filesystem:serde_json::Value }`; `.validate()`.
- `PublicationPhase::{Intent,Prepared,ReportCommitting,ReportCommitted,MovieCommitting}`.
- `PublicationOutcome::{InProgress,Interrupted,Unresolved,Failed,Cancelled,Published,PublishedUnconfirmed}`.
- `PublicationOperationKind::{Publish,Reconcile}`.
- `PublicationIdentity { publication_id:RequestId, operation_id:AttemptId, cancellation_token:CancellationToken, expected_sequence:u64 }`.
- `PublicationOperation { publication_id, operation_id, cancellation_token, ordinal:u64, kind, verified_attempt_id:AttemptId, active:bool, outcome, diagnostic:Option<RenderDiagnostic> }`.
- `StoredPublication { intent:PublicationIntent, render_intent:RenderIntent, encoding_attempt_id:AttemptId, movie_sha256:Sha256, movie_bytes:u64, prepared:Option<PreparedPublicationEvidence>, phase, outcome, sequence:u64, observed_movie_commit:bool, cancellation_requested:bool, operation:PublicationOperation }`; `.identity()->PublicationIdentity`.
- `PublicationCompletion::{Failed(RenderDiagnostic),Cancelled,Published,PublishedUnconfirmed(RenderDiagnostic),Unresolved(RenderDiagnostic)}`.
- `PublicationReconciliation::{Confirmed,CommittedUnconfirmed(RenderDiagnostic),NotPublished(RenderDiagnostic),Unresolved(RenderDiagnostic)}`.

Store path `deadpan_store::publication`:
- opaque `PublicationPermit` (not Clone/Deserialize): `.record()->&StoredPublication`, `.identity()->PublicationIdentity`, `.check_live()->Result<(),StoreError>`. Identity/phase/kind available from record. One permit may be shared by borrow for its exact stage; any committed transition or owner closure revokes it.
- `ProjectStore::begin_render_publication(intent:PublicationIntent, operation_id:AttemptId, cancellation_token:CancellationToken)->Result<PublicationPermit,StoreError>`
- `record_prepared_publication(&PublicationIdentity,PreparedPublicationEvidence)->Result<PublicationPermit,StoreError>`
- `advance_publication(&PublicationIdentity,PublicationPhase)->Result<PublicationPermit,StoreError>`
- `request_publication_cancellation(&PublicationIdentity)->Result<StoredPublication,StoreError>`
- `finish_publication(&PublicationIdentity,PublicationCompletion)->Result<StoredPublication,StoreError>`
- `begin_publication_reconciliation(&RequestId, verified_attempt_id:AttemptId, operation_id:AttemptId, cancellation_token:CancellationToken)->Result<PublicationPermit,StoreError>`
- `finish_publication_reconciliation(&PublicationIdentity,PublicationReconciliation)->Result<StoredPublication,StoreError>`
- `render_publication(&RequestId)->Result<StoredPublication,StoreError>`
- `render_publications(after:Option<&RequestId>,limit:u32)->Result<Vec<StoredPublication>,StoreError>`
- `publication_operations(&RequestId,after_ordinal:u64,limit:u32)->Result<Vec<PublicationOperation>,StoreError>`

Every mutation commits, revokes previous permit, performs strict DB/WAL/package barrier. A barrier error leaves committed metadata observable, denies the permit, and revokes any old one. Cancellation has no rename permit. Finish methods require exact current identity even when cancellation was requested. `Published`/`PublishedUnconfirmed` require MovieCommitting phase and set observed commit regardless of cancellation. Failed/Cancelled are definite no-movie-commit host declarations and reject existing observed commit. Unresolved preserves uncertainty/observed commit. Reconciliation requires a newer terminal Verified render attempt and same checkpoint/movie. Confirmed and CommittedUnconfirmed require prepared evidence and prior MovieCommitting phase. NotPublished permits pre-movie phases only. Definite-unpublished completion cannot be adopted. A fresh live VerifiedCandidate remains the CLI host's independent requirement.
