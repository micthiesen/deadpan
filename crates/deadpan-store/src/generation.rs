//! Persistent generation intent and monotonic relevance outside document history.
//!
//! Media/context resolution remains host-owned. The store persists the resolved
//! context hash and independently checks the target Hold, duration, project rate,
//! request identity, and complete relevance coverage.

use std::collections::BTreeMap;

use deadpan_core::{
    MAX_DOCUMENT_NODES, MAX_IDENTITY_BYTES, NodeId, NodeKind, ProjectDocument, RevisionId,
    ScopedNodeTarget,
};
use deadpan_jobs::{
    BridgeGenerationPlan, GenerationPlan, HoldConstraints, ProviderSelection, Relevance, RequestId,
    RequestVersion, Sha256, TargetBinding,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::generation_inputs::{
    GenerationCaptureSpec, GenerationInputBinding, InputCaptureBudget, MAX_INPUT_BINDING_BYTES,
};
use crate::generation_pictures::QualifiedGenerationPictures;
pub use crate::generation_scope::GenerationScopeId;
use crate::{ProjectStore, StoreError, read_snapshot, validation};

const MAX_REQUEST_JSON_BYTES: usize = 16 * 1024;

const CREATE_TABLES: &str = "
CREATE TABLE generation_requests (
    request_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    hold_id TEXT NOT NULL,
    scope_id TEXT NOT NULL REFERENCES generation_scopes(scope_id),
    origin_target TEXT NOT NULL CHECK (json_valid(origin_target)),
    request_version INTEGER NOT NULL
        CHECK (request_version BETWEEN 1 AND 9223372036854775807),
    origin_revision TEXT NOT NULL REFERENCES revisions(id),
    context_sha256 TEXT NOT NULL,
    constraints TEXT NOT NULL CHECK (json_valid(constraints)),
    provider TEXT NOT NULL CHECK (json_valid(provider)),
    plan TEXT CHECK (plan IS NULL OR json_valid(plan)),
    input_binding TEXT CHECK (input_binding IS NULL OR json_valid(input_binding)),
    relevance TEXT NOT NULL CHECK (relevance IN ('current','stale','detached')),
    UNIQUE (scope_id, request_version),
    CHECK ((plan IS NULL) = (input_binding IS NULL))
) STRICT;
CREATE UNIQUE INDEX one_current_generation_per_scope
    ON generation_requests(scope_id) WHERE relevance='current';
CREATE INDEX generation_request_origin ON generation_requests(origin_revision,scope_id,request_version);";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationRequestInput {
    pub request_id: RequestId,
    pub expected_revision: RevisionId,
    pub hold_id: NodeId,
    pub context_sha256: Sha256,
    pub constraints: HoldConstraints,
    pub provider: ProviderSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredGenerationRequest {
    pub request_id: RequestId,
    pub origin_revision: RevisionId,
    pub binding: TargetBinding,
    pub scope_id: GenerationScopeId,
    /// Immutable authoring address captured alongside the worker binding.
    pub origin_target: ScopedNodeTarget,
    /// Current address after proven isolation. Worker artifacts never use it
    /// as a replacement for their original binding.
    pub target: ScopedNodeTarget,
    pub constraints: HoldConstraints,
    pub provider: ProviderSelection,
    pub plan: Option<GenerationPlan>,
    /// Independently captured immutable origin inputs. Legacy V1 has neither
    /// an operation plan nor this qualified structural descriptor.
    pub input_binding: Option<GenerationInputBinding>,
    pub relevance: Relevance,
}

impl StoredGenerationRequest {
    pub fn bridge_plan(&self) -> Option<&BridgeGenerationPlan> {
        match &self.plan {
            Some(GenerationPlan::Bridge(plan)) => Some(plan),
            Some(GenerationPlan::Extension(_)) | None => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextObservation {
    Resolved(Sha256),
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelevanceObservation {
    pub request_id: RequestId,
    pub binding: TargetBinding,
    /// Exact mapped authoring address whose context the host observed.
    pub target: ScopedNodeTarget,
    pub after_context: ContextObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelevancePlan {
    pub from_revision: RevisionId,
    pub to_revision: RevisionId,
    pub observations: Vec<RelevanceObservation>,
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    crate::generation_scope::create_tables(connection)?;
    connection.execute_batch(CREATE_TABLES)?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    crate::generation_scope::check_stored_sizes(connection)?;
    let invalid_requests: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_requests WHERE
            typeof(request_id)!='text' OR length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(project_id)!='text' OR length(CAST(project_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(hold_id)!='text' OR length(CAST(hold_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(scope_id)!='text' OR length(CAST(scope_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(origin_target)!='text' OR length(CAST(origin_target AS BLOB)) NOT BETWEEN 1 AND ?3 OR
            typeof(request_version)!='integer' OR request_version<1 OR
            typeof(origin_revision)!='text' OR length(CAST(origin_revision AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(context_sha256)!='text' OR length(CAST(context_sha256 AS BLOB))!=64 OR
            typeof(constraints)!='text' OR length(CAST(constraints AS BLOB))>?2 OR
            typeof(provider)!='text' OR length(CAST(provider AS BLOB))>?2 OR
            (plan IS NOT NULL AND
             (typeof(plan)!='text' OR length(CAST(plan AS BLOB))>?2)) OR
            (input_binding IS NOT NULL AND
             (typeof(input_binding)!='text' OR length(CAST(input_binding AS BLOB))>?4)) OR
            ((plan IS NULL) != (input_binding IS NULL)) OR
            typeof(relevance)!='text' OR length(CAST(relevance AS BLOB)) NOT BETWEEN 5 AND 8",
        params![MAX_IDENTITY_BYTES as i64, MAX_REQUEST_JSON_BYTES as i64, crate::generation_scope::MAX_TARGET_BYTES as i64, MAX_INPUT_BINDING_BYTES as i64],
        |row| row.get(0),
    )?;
    if invalid_requests != 0 {
        return Err(integrity(
            "stored generation metadata exceeds its bounds or has the wrong type",
        ));
    }
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    let document = read_snapshot(connection)?;
    let mut identifiers = connection.prepare(
        "SELECT CASE WHEN typeof(request_id)='text'
                     AND length(CAST(request_id AS BLOB)) BETWEEN 1 AND ?1
                     THEN request_id END
         FROM generation_requests ORDER BY request_id",
    )?;
    let mut identifier_rows = identifiers.query([MAX_IDENTITY_BYTES as i64])?;
    let mut prior_request: Option<RequestId> = None;
    while let Some(row) = identifier_rows.next()? {
        let request: Option<String> = row.get(0)?;
        let request =
            RequestId::new(request.ok_or_else(|| integrity("invalid generation request ID"))?)
                .map_err(|_| integrity("invalid generation request ID"))?;
        if prior_request.as_ref() == Some(&request) {
            return Err(integrity("duplicate generation request ID"));
        }
        prior_request = Some(request);
    }

    let mut statement = connection.prepare(&format!(
        "{BOUNDED_REQUEST_SELECT} ORDER BY scope_id,request_version,request_id"
    ))?;
    let mut rows = statement.query([MAX_IDENTITY_BYTES as i64])?;
    let mut prior_pair: Option<(GenerationScopeId, RequestVersion)> = None;
    let mut prior_current_scope: Option<GenerationScopeId> = None;
    while let Some(row) = rows.next()? {
        let request = parse_request_row(connection, row)?;
        let pair = (request.scope_id.clone(), request.binding.request_version);
        if prior_pair.as_ref() == Some(&pair) {
            return Err(integrity("duplicate scoped Hold request version"));
        }
        prior_pair = Some(pair);
        if request.relevance == Relevance::Current {
            if prior_current_scope.as_ref() == Some(&request.scope_id) {
                return Err(integrity("multiple current requests target one Hold scope"));
            }
            prior_current_scope = Some(request.scope_id.clone());
        }
        if let Some(plan) = request.plan.as_ref() {
            validate_plan_binding(&request.constraints, plan)?;
        }
        let origin =
            validation::read_revision(connection, request.origin_revision.as_str())?.document;
        if request.binding.project_id != *document.project_id()
            || request.binding.project_id != *origin.project_id()
            || request.origin_target.node != request.binding.hold_id
            || request.origin_target.validate(&origin).is_err()
            || validate_new_target(&origin, &request.binding.hold_id, &request.constraints).is_err()
        {
            return Err(integrity(
                "generation request is incompatible with its origin revision",
            ));
        }
        if let Some(plan) = &request.plan {
            let captured = capture_request_inputs(
                connection,
                &origin,
                &request.origin_target,
                &request.constraints,
                plan,
            )?;
            if request.input_binding.as_ref() != Some(&captured) {
                return Err(integrity(
                    "generation input binding differs from its immutable origin",
                ));
            }
        }
        let high_water = crate::generation_scope::read(connection, &request.scope_id)?.high_water;
        let request_version = request_version_i64(request.binding.request_version)?;
        if high_water < request_version {
            return Err(integrity(
                "scope request clock is below an allocated version",
            ));
        }
        if request.relevance == Relevance::Current {
            if high_water != request_version {
                return Err(integrity(
                    "current generation request is not the latest allocated scope version",
                ));
            }
            validate_current_target(&document, &request)?;
        }
    }

    let mut clocks = connection.prepare(
        "SELECT
            CASE WHEN typeof(scope_id)='text'
                 AND length(CAST(scope_id AS BLOB)) BETWEEN 1 AND ?1 THEN scope_id END
         FROM generation_scopes ORDER BY current_target",
    )?;
    let mut rows = clocks.query([MAX_IDENTITY_BYTES as i64])?;
    let mut prior_target: Option<String> = None;
    while let Some(row) = rows.next()? {
        let id: Option<String> = row.get(0)?;
        let id = crate::generation_scope::parse_id(
            id.ok_or_else(|| integrity("invalid generation scope identity"))?,
        )?;
        let scope = crate::generation_scope::read(connection, &id)?;
        let target = crate::generation_scope::target_json(&scope.target)?;
        if prior_target.as_ref() == Some(&target) {
            return Err(integrity("duplicate generation scope target"));
        }
        prior_target = Some(target);
        let origin =
            validation::read_revision(connection, scope.origin_revision.as_str())?.document;
        scope
            .origin_target
            .validate(&origin)
            .map_err(|_| integrity("invalid generation scope origin"))?;
        let first_request_matches: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM generation_requests WHERE request_id=?1 AND scope_id=?1
                AND origin_revision=?2 AND origin_target=?3)",
            params![
                scope.id.as_str(),
                scope.origin_revision.as_str(),
                crate::generation_scope::target_json(&scope.origin_target)?
            ],
            |row| row.get(0),
        )?;
        if !first_request_matches {
            return Err(integrity(
                "generation scope disagrees with its first request",
            ));
        }
        let maximum: Option<i64> = connection.query_row(
            "SELECT MAX(request_version) FROM generation_requests WHERE scope_id=?1",
            [scope.id.as_str()],
            |row| row.get(0),
        )?;
        if maximum != Some(scope.high_water) {
            return Err(integrity(
                "scope request clock does not equal its latest allocated version",
            ));
        }
    }
    Ok(())
}

impl ProjectStore {
    pub fn allocate_generation_request(
        &mut self,
        input: GenerationRequestInput,
    ) -> Result<StoredGenerationRequest, StoreError> {
        self.allocate_generation_request_with_plan(input, None, None)
    }

    /// Allocates a new request bound to an immutable bridge plan. Legacy
    /// allocation intentionally remains V1/unqualified and stores no plan.
    pub fn record_bridge_generation_request(
        &mut self,
        input: GenerationRequestInput,
        plan: BridgeGenerationPlan,
    ) -> Result<StoredGenerationRequest, StoreError> {
        self.allocate_generation_request_with_plan(input, Some(GenerationPlan::Bridge(plan)), None)
    }

    /// Capture an explicit Default or Play address without isolating it or
    /// changing authored state. The complete authored Hold duration owns the
    /// generated provider, independently of its rendered occurrence duration.
    pub fn record_scoped_bridge_generation_request(
        &mut self,
        input: GenerationRequestInput,
        target: ScopedNodeTarget,
        plan: BridgeGenerationPlan,
    ) -> Result<StoredGenerationRequest, StoreError> {
        self.record_scoped_generation_request(input, target, GenerationPlan::Bridge(plan))
    }

    /// Records a resolved operation and independently derives its complete
    /// input binding from retained measured metadata at the origin revision.
    pub fn record_scoped_generation_request(
        &mut self,
        input: GenerationRequestInput,
        target: ScopedNodeTarget,
        plan: GenerationPlan,
    ) -> Result<StoredGenerationRequest, StoreError> {
        self.allocate_generation_request_with_plan(input, Some(plan), Some(target))
    }

    fn allocate_generation_request_with_plan(
        &mut self,
        input: GenerationRequestInput,
        plan: Option<GenerationPlan>,
        target: Option<ScopedNodeTarget>,
    ) -> Result<StoredGenerationRequest, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        crate::generation_preparations::verify(&transaction)?;
        let result = allocate_request(&transaction, input, plan, target, true)?;
        crate::audit::refresh_generation_scopes(&transaction)?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn generation_request(
        &self,
        request_id: &RequestId,
    ) -> Result<Option<StoredGenerationRequest>, StoreError> {
        read_stored_request(&self.connection, request_id)
    }

    pub fn current_generation_requests(&self) -> Result<Vec<StoredGenerationRequest>, StoreError> {
        read_current_requests(&self.connection)
    }
}

pub(crate) fn read_stored_request(
    connection: &Connection,
    request_id: &RequestId,
) -> Result<Option<StoredGenerationRequest>, StoreError> {
    let mut statement =
        connection.prepare(&format!("{BOUNDED_REQUEST_SELECT} WHERE request_id=?2"))?;
    let mut rows = statement.query(params![MAX_IDENTITY_BYTES as i64, request_id.as_str()])?;
    let result = rows
        .next()?
        .map(|row| parse_request_row(connection, row))
        .transpose()?;
    if rows.next()?.is_some() {
        return Err(integrity("duplicate generation request identity"));
    }
    Ok(result)
}

/// Transaction-local allocation shared with preparation fulfilment.
pub(crate) fn allocate_request(
    connection: &Connection,
    input: GenerationRequestInput,
    plan: Option<GenerationPlan>,
    target: Option<ScopedNodeTarget>,
    supersede: bool,
) -> Result<StoredGenerationRequest, StoreError> {
    let document = read_snapshot(connection)?;
    require_revision(&document, &input.expected_revision)?;
    validate_new_target(&document, &input.hold_id, &input.constraints)?;
    let explicit_scope = target.is_some();
    let target = target.unwrap_or_else(|| ScopedNodeTarget {
        node: input.hold_id.clone(),
        repeats: Vec::new(),
    });
    if target.node != input.hold_id {
        return Err(StoreError::GenerationTarget(
            "scope differs from the requested Hold".into(),
        ));
    }
    target.validate(&document)?;
    if let Some(plan) = plan.as_ref() {
        validate_plan_binding(&input.constraints, plan)?;
        if !explicit_scope {
            crate::generation_acceptance::require_single_generation_occurrence(
                &document,
                &input.hold_id,
            )?;
        }
    }

    let input_binding = plan
        .as_ref()
        .map(|plan| {
            capture_request_inputs(connection, &document, &target, &input.constraints, plan)
        })
        .transpose()?;
    let input_binding_json = input_binding
        .as_ref()
        .map(bounded_input_binding_json)
        .transpose()?;

    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM generation_requests WHERE request_id=?1",
            [input.request_id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_some()
        || crate::retired::contains(connection, "generation_request", input.request_id.as_str())?
    {
        return Err(StoreError::GenerationRequestReused(
            input.request_id.as_str().to_owned(),
        ));
    }

    if supersede {
        crate::generation_preparations::supersede(connection, &target, &input.request_id)?;
    }

    let (scope_id, version) =
        crate::generation_scope::allocate(connection, &document, &target, &input.request_id)?;
    connection.execute(
        "UPDATE generation_requests SET relevance='stale'
         WHERE scope_id=?1 AND relevance='current'",
        [scope_id.as_str()],
    )?;

    let constraints = bounded_json(&input.constraints, "generation constraints")?;
    let provider = bounded_json(&input.provider, "generation provider")?;
    let plan_json = plan
        .as_ref()
        .map(|plan| bounded_json(plan, "generation plan"))
        .transpose()?;
    connection.execute(
        "INSERT INTO generation_requests(
            request_id,project_id,hold_id,request_version,origin_revision,
            context_sha256,constraints,provider,plan,relevance,scope_id,origin_target,input_binding
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'current',?10,?11,?12)",
        params![
            input.request_id.as_str(),
            document.project_id().as_str(),
            input.hold_id.as_str(),
            request_version_i64(version)?,
            document.revision_id().as_str(),
            input.context_sha256.as_str(),
            constraints,
            provider,
            plan_json,
            scope_id.as_str(),
            crate::generation_scope::target_json(&target)?,
            input_binding_json,
        ],
    )?;
    crate::generation_scope::check_stored_sizes(connection)?;
    Ok(StoredGenerationRequest {
        request_id: input.request_id,
        origin_revision: document.revision_id().clone(),
        scope_id,
        origin_target: target.clone(),
        target,
        binding: TargetBinding {
            project_id: document.project_id().clone(),
            hold_id: input.hold_id,
            request_version: version,
            context_sha256: input.context_sha256,
        },
        constraints: input.constraints,
        provider: input.provider,
        plan,
        input_binding,
        relevance: Relevance::Current,
    })
}

/// A host's view of whether a current generation request still describes
/// the same context after a document change. The store asks it for every
/// current request when a write supplies no explicit relevance plan.
pub trait GenerationContextResolver: Send + Sync {
    /// Production observers compare selected measured frames. The default
    /// preserves custom resolvers that do not inspect picture context.
    fn preparation_is_relevant_with_pictures(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        preparation: &crate::generation_preparations::StoredGenerationPreparation,
        _pictures: &dyn crate::generation_pictures::GenerationPictures,
    ) -> bool {
        self.preparation_is_relevant(origin, after, preparation)
    }

    fn observe_with_pictures(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        request: &StoredGenerationRequest,
        _pictures: &dyn crate::generation_pictures::GenerationPictures,
    ) -> ContextObservation {
        self.observe(origin, after, request)
    }

    /// Whether a pending replacement retains its original raw input context.
    /// Conservative resolvers may return false; no stale preparation is revived.
    fn preparation_is_relevant(
        &self,
        _origin: &ProjectDocument,
        _after: &ProjectDocument,
        _preparation: &crate::generation_preparations::StoredGenerationPreparation,
    ) -> bool {
        false
    }

    /// Prepare shared work for this exact prospective document. A failed
    /// write may reuse its revision ID with different contents, so prepared
    /// state must be confined to this borrowed transition, not cached by ID.
    /// `None` uses the resolver directly for each observation.
    fn prepare_transition<'a>(
        &'a self,
        _after: &'a ProjectDocument,
    ) -> Option<Box<dyn GenerationContextResolver + 'a>> {
        None
    }

    /// The context of `request`'s Hold in `after`, given `origin`, the
    /// document the request was made against. `Resolved` with the request's
    /// own hash keeps it current; anything else makes it stale.
    fn observe(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        request: &StoredGenerationRequest,
    ) -> ContextObservation;
}

/// Reconcile current requests for one document transition: an explicit plan,
/// else the installed resolver, else refuse while any request is current.
pub(crate) fn reconcile(
    connection: &Connection,
    before: &ProjectDocument,
    after: &ProjectDocument,
    explicit: Option<&RelevancePlan>,
    resolver: Option<&dyn GenerationContextResolver>,
) -> Result<(), StoreError> {
    match (explicit, resolver) {
        (Some(plan), _) => apply_relevance_plan(connection, before, after, plan),
        (None, Some(resolver)) => {
            let current = read_current_requests(connection)?;
            if current.is_empty() {
                return Ok(());
            }
            let prepared = resolver.prepare_transition(after);
            let resolver = prepared.as_deref().unwrap_or(resolver);
            let pictures = crate::generation_pictures::QualifiedGenerationPictures::new(connection);
            let mut observations = Vec::with_capacity(current.len());
            for request in current {
                let origin =
                    validation::read_revision(connection, request.origin_revision.as_str())?
                        .document;
                observations.push(RelevanceObservation {
                    request_id: request.request_id.clone(),
                    binding: request.binding.clone(),
                    target: request.target.clone(),
                    after_context: resolver
                        .observe_with_pictures(&origin, after, &request, &pictures),
                });
            }
            apply_relevance_plan(
                connection,
                before,
                after,
                &RelevancePlan {
                    from_revision: before.revision_id().clone(),
                    to_revision: after.revision_id().clone(),
                    observations,
                },
            )
        }
        (None, None) => ensure_no_current(connection),
    }
}

pub(crate) fn ensure_no_current(connection: &Connection) -> Result<(), StoreError> {
    let current: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM generation_requests WHERE relevance='current' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some() {
        return Err(StoreError::GenerationRelevanceRequired);
    }
    Ok(())
}

pub(crate) fn apply_relevance_plan(
    connection: &Connection,
    before: &ProjectDocument,
    after: &ProjectDocument,
    plan: &RelevancePlan,
) -> Result<(), StoreError> {
    if &plan.from_revision != before.revision_id() || &plan.to_revision != after.revision_id() {
        return Err(plan_error(
            "plan revisions do not match the document transition",
        ));
    }
    let current = read_current_requests(connection)?;
    if plan.observations.len() > MAX_DOCUMENT_NODES {
        return Err(plan_error("plan exceeds the current Hold limit"));
    }
    let mut observations = BTreeMap::new();
    for observation in &plan.observations {
        if observations
            .insert(observation.request_id.as_str(), observation)
            .is_some()
        {
            return Err(plan_error("plan contains a duplicate request observation"));
        }
    }
    if observations.len() != current.len() {
        return Err(plan_error(
            "plan does not cover exactly every current generation request",
        ));
    }

    for request in current {
        let observation = observations
            .remove(request.request_id.as_str())
            .ok_or_else(|| plan_error("plan is missing a current generation request"))?;
        if observation.binding != request.binding {
            return Err(plan_error(
                "plan binding does not match the persisted generation request",
            ));
        }
        if observation.target != request.target {
            return Err(plan_error(
                "plan target does not match the mapped generation scope",
            ));
        }
        let relevance = relevance_after(after, &request, &observation.after_context);
        if relevance != Relevance::Current {
            let changed = connection.execute(
                "UPDATE generation_requests SET relevance=?1
                 WHERE request_id=?2 AND relevance='current'",
                params![relevance_text(relevance), request.request_id.as_str()],
            )?;
            if changed != 1 {
                return Err(plan_error(
                    "current generation request changed during reconciliation",
                ));
            }
        }
    }
    Ok(())
}

fn relevance_after(
    document: &ProjectDocument,
    request: &StoredGenerationRequest,
    context: &ContextObservation,
) -> Relevance {
    let Some(node) = document.nodes().get(&request.target.node) else {
        return Relevance::Detached;
    };
    if request.target.validate(document).is_err() {
        return Relevance::Detached;
    }
    let NodeKind::Hold { recipe } = &node.kind else {
        return Relevance::Stale;
    };
    if request.constraints.video.frames() != recipe.duration
        || request.constraints.video.frame_rate() != document.presentation_basis().frame_rate
    {
        return Relevance::Stale;
    }
    match context {
        ContextObservation::Resolved(hash) if hash == &request.binding.context_sha256 => {
            Relevance::Current
        }
        ContextObservation::Resolved(_) | ContextObservation::Unresolved => Relevance::Stale,
    }
}

fn validate_new_target(
    document: &ProjectDocument,
    hold_id: &NodeId,
    constraints: &HoldConstraints,
) -> Result<(), StoreError> {
    let node = document
        .nodes()
        .get(hold_id)
        .ok_or_else(|| StoreError::GenerationTarget(format!("Hold {hold_id} does not exist")))?;
    let NodeKind::Hold { recipe } = &node.kind else {
        return Err(StoreError::GenerationTarget(format!(
            "node {hold_id} is not a Hold"
        )));
    };
    if constraints.video.frames() != recipe.duration {
        return Err(StoreError::GenerationTarget(
            "requested frame count differs from the Hold duration".into(),
        ));
    }
    if constraints.video.frame_rate() != document.presentation_basis().frame_rate {
        return Err(StoreError::GenerationTarget(
            "requested frame rate differs from the project rate".into(),
        ));
    }
    Ok(())
}

fn validate_current_target(
    document: &ProjectDocument,
    request: &StoredGenerationRequest,
) -> Result<(), StoreError> {
    request
        .target
        .validate(document)
        .map_err(|_| integrity("current generation scope is incompatible with the project head"))?;
    validate_new_target(document, &request.target.node, &request.constraints)
        .map_err(|_| integrity("current generation request is incompatible with the project head"))
}

fn require_revision(document: &ProjectDocument, expected: &RevisionId) -> Result<(), StoreError> {
    if document.revision_id() != expected {
        return Err(StoreError::RevisionConflict {
            expected: expected.as_str().to_owned(),
            current: document.revision_id().as_str().to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn read_current_requests(
    connection: &Connection,
) -> Result<Vec<StoredGenerationRequest>, StoreError> {
    let mut statement = connection.prepare(&format!(
        "{BOUNDED_REQUEST_SELECT} WHERE relevance='current' ORDER BY scope_id"
    ))?;
    let mut rows = statement.query([MAX_IDENTITY_BYTES as i64])?;
    let mut requests = Vec::new();
    while let Some(row) = rows.next()? {
        if requests.len() == MAX_DOCUMENT_NODES {
            return Err(integrity(
                "current generation requests exceed the document node limit",
            ));
        }
        requests.push(parse_request_row(connection, row)?);
    }
    Ok(requests)
}

const BOUNDED_REQUEST_SELECT: &str = "SELECT
    CASE WHEN typeof(request_id)='text'
         AND length(CAST(request_id AS BLOB)) BETWEEN 1 AND ?1 THEN request_id END,
    CASE WHEN typeof(project_id)='text'
         AND length(CAST(project_id AS BLOB)) BETWEEN 1 AND ?1 THEN project_id END,
    CASE WHEN typeof(hold_id)='text'
         AND length(CAST(hold_id AS BLOB)) BETWEEN 1 AND ?1 THEN hold_id END,
    CASE WHEN typeof(request_version)='integer' AND request_version>=1 THEN request_version END,
    CASE WHEN typeof(origin_revision)='text'
         AND length(CAST(origin_revision AS BLOB)) BETWEEN 1 AND ?1 THEN origin_revision END,
    CASE WHEN typeof(context_sha256)='text'
         AND length(CAST(context_sha256 AS BLOB))=64 THEN context_sha256 END,
    CASE WHEN typeof(constraints)='text'
         AND length(CAST(constraints AS BLOB))<=16384 THEN constraints END,
    CASE WHEN typeof(provider)='text'
         AND length(CAST(provider AS BLOB))<=16384 THEN provider END,
    CASE WHEN plan IS NULL THEN NULL
         WHEN typeof(plan)='text' AND length(CAST(plan AS BLOB))<=16384
         THEN plan
         ELSE '__invalid__' END,
    CASE WHEN relevance IN ('current','stale','detached') THEN relevance END,
    CASE WHEN typeof(scope_id)='text' AND length(CAST(scope_id AS BLOB)) BETWEEN 1 AND ?1 THEN scope_id END,
    CASE WHEN typeof(origin_target)='text' AND length(CAST(origin_target AS BLOB)) BETWEEN 1 AND 131072 THEN origin_target END,
    CASE WHEN input_binding IS NULL THEN NULL
         WHEN typeof(input_binding)='text' AND length(CAST(input_binding AS BLOB))<=524288
         THEN input_binding ELSE '__invalid__' END
 FROM generation_requests";

fn parse_request_row(
    connection: &Connection,
    row: &Row<'_>,
) -> Result<StoredGenerationRequest, StoreError> {
    let request_id: Option<String> = row.get(0)?;
    let project_id: Option<String> = row.get(1)?;
    let hold_id: Option<String> = row.get(2)?;
    let version: Option<i64> = row.get(3)?;
    let origin_revision: Option<String> = row.get(4)?;
    let context_sha256: Option<String> = row.get(5)?;
    let constraints: Option<String> = row.get(6)?;
    let provider: Option<String> = row.get(7)?;
    let plan: Option<String> = row.get(8)?;
    let relevance: Option<String> = row.get(9)?;
    let scope_id: Option<String> = row.get(10)?;
    let origin_target: Option<String> = row.get(11)?;
    let input_binding: Option<String> = row.get(12)?;
    (|| {
        let request_id = RequestId::new(required(request_id, "request ID")?)
            .map_err(|_| integrity("invalid generation request ID"))?;
        let project_id = deadpan_core::ProjectId::new(required(project_id, "project ID")?)
            .map_err(|_| integrity("invalid generation project ID"))?;
        let hold_id = NodeId::new(required(hold_id, "Hold ID")?)
            .map_err(|_| integrity("invalid generation Hold ID"))?;
        let version = version.ok_or_else(|| integrity("invalid generation request version"))?;
        let request_version = RequestVersion::new(version as u64)
            .map_err(|_| integrity("invalid generation request version"))?;
        let origin_revision = RevisionId::new(required(origin_revision, "origin revision")?)
            .map_err(|_| integrity("invalid generation origin revision"))?;
        let context_sha256 = Sha256::new(required(context_sha256, "context SHA-256")?)
            .map_err(|_| integrity("invalid generation context SHA-256"))?;
        let constraints = strict_json(
            &required(constraints, "constraints")?,
            "generation constraints",
        )?;
        let provider = strict_json(&required(provider, "provider")?, "generation provider")?;
        let plan: Option<GenerationPlan> = plan
            .map(|value| strict_json(&value, "generation plan"))
            .transpose()?;
        let input_binding: Option<GenerationInputBinding> = input_binding
            .map(|value| strict_json(&value, "generation input binding"))
            .transpose()?;
        if plan.is_some() != input_binding.is_some() {
            return Err(integrity(
                "generation plan and input binding presence disagree",
            ));
        }
        if let Some(plan) = &plan {
            validate_plan_binding(&constraints, plan)?;
            if input_binding.as_ref().is_none_or(|binding| {
                binding.capture_spec() != GenerationCaptureSpec::from_plan(plan)
            }) {
                return Err(integrity(
                    "generation plan and input capture operation disagree",
                ));
            }
        }
        let relevance = match required(relevance, "relevance")?.as_str() {
            "current" => Relevance::Current,
            "stale" => Relevance::Stale,
            "detached" => Relevance::Detached,
            _ => return Err(integrity("invalid generation relevance")),
        };
        let scope_id = crate::generation_scope::parse_id(required(scope_id, "scope identity")?)?;
        let origin_target =
            crate::generation_scope::parse_target(&required(origin_target, "origin scope")?)?;
        let scope = crate::generation_scope::read(connection, &scope_id)?;
        Ok(StoredGenerationRequest {
            request_id,
            origin_revision,
            scope_id,
            origin_target,
            target: scope.target,
            binding: TargetBinding {
                project_id,
                hold_id,
                request_version,
                context_sha256,
            },
            constraints,
            provider,
            plan,
            input_binding,
            relevance,
        })
    })()
}

fn bounded_json(value: &impl serde::Serialize, label: &str) -> Result<String, StoreError> {
    let json = serde_json::to_string(value)?;
    if json.len() > MAX_REQUEST_JSON_BYTES {
        return Err(StoreError::GenerationTarget(format!(
            "{label} exceeds the persistence limit"
        )));
    }
    Ok(json)
}

fn strict_json<T: serde::de::DeserializeOwned>(json: &str, label: &str) -> Result<T, StoreError> {
    serde_json::from_str(json).map_err(|_| integrity(&format!("invalid stored {label}")))
}

fn required(value: Option<String>, label: &str) -> Result<String, StoreError> {
    value.ok_or_else(|| integrity(&format!("invalid or oversized generation {label}")))
}

fn request_version_i64(version: RequestVersion) -> Result<i64, StoreError> {
    i64::try_from(version.get()).map_err(|_| integrity("generation request version exceeds SQLite"))
}

fn relevance_text(relevance: Relevance) -> &'static str {
    match relevance {
        Relevance::Current => "current",
        Relevance::Stale => "stale",
        Relevance::Detached => "detached",
    }
}

fn integrity(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}

fn plan_error(message: &str) -> StoreError {
    StoreError::GenerationPlan(message.into())
}

fn validate_plan_binding(
    constraints: &HoldConstraints,
    plan: &GenerationPlan,
) -> Result<(), StoreError> {
    if constraints.conditioning != plan.conditioning() {
        return Err(plan_error(
            "generation plan operation differs from requested conditioning",
        ));
    }
    if plan.project_frames() != constraints.video.frames()
        || plan.project_frame_rate() != constraints.video.frame_rate()
    {
        return Err(plan_error(
            "generation plan does not match the requested project video",
        ));
    }
    let dimensions = plan.native_dimensions();
    if dimensions.width() != constraints.video.width()
        || dimensions.height() != constraints.video.height()
    {
        return Err(plan_error(
            "generation plan dimensions do not match the requested video",
        ));
    }
    Ok(())
}

fn capture_request_inputs(
    connection: &Connection,
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    constraints: &HoldConstraints,
    generation_plan: &GenerationPlan,
) -> Result<GenerationInputBinding, StoreError> {
    if constraints
        .region_target
        .as_ref()
        .is_some_and(|id| !document.targets().contains_key(id))
    {
        return Err(plan_error("generation region target is absent"));
    }
    let plan = deadpan_plan::RenderPlan::compile(document)
        .map_err(|error| plan_error(&error.to_string()))?;
    GenerationInputBinding::capture_with_plan(
        document,
        &plan,
        target,
        GenerationCaptureSpec::from_plan(generation_plan),
        constraints.region_target.as_ref(),
        &QualifiedGenerationPictures::new(connection),
        &mut InputCaptureBudget::default(),
    )
}

fn bounded_input_binding_json(binding: &GenerationInputBinding) -> Result<String, StoreError> {
    let json = serde_json::to_string(binding)?;
    if json.len() > MAX_INPUT_BINDING_BYTES {
        return Err(plan_error(
            "generation input binding exceeds the persistence limit",
        ));
    }
    Ok(json)
}

#[cfg(test)]
#[path = "generation/operation_tests.rs"]
mod operation_tests;
