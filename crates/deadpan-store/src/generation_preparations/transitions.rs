use super::*;
use crate::generation_intents::{
    self as intents, IntentAuthorization, IntentBirthReceipt, IntentCause, IntentTerminal,
    IntentTerminalReason, TerminalPhase,
};
use deadpan_core::{
    Command, CommandRequest, HoldFallback, HoldVideo, NodeKind, OccurrenceEdit, ProjectDocument,
    RepeatEditBranch, RepeatEditStep, ScopedNodeTarget, ValidatedScopedIsolation,
};
use deadpan_jobs::GenerationOptions;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod canonical;
pub(super) use canonical::{AddressBudget, CanonicalTargets};

#[derive(Clone)]
pub(crate) struct Birth {
    pub target: ScopedNodeTarget,
    pub origin: PreparationOrigin,
    pub duration: FrameDuration,
    pub fallback: HoldVideo,
    pub cause: IntentCause,
    pub authorization: IntentAuthorization,
}

fn duration_node(command: &Command) -> Option<&deadpan_core::NodeId> {
    match command.base_command() {
        Command::SetHoldDuration { node, .. } => Some(node),
        Command::EditOccurrence {
            instance,
            edit: OccurrenceEdit::SetHoldDuration { .. },
            ..
        } => Some(&instance.node),
        _ => None,
    }
}

fn changes_provider(
    command: &Command,
    target: &ScopedNodeTarget,
    document: &ProjectDocument,
) -> bool {
    match command.base_command() {
        Command::SetHoldProvider { node, .. }
        | Command::AcceptGeneratedHold { node, .. }
        | Command::RevertGeneratedHold { node } => node == &target.node,
        Command::EditOccurrence {
            instance,
            edit:
                OccurrenceEdit::SetHoldProvider { .. }
                | OccurrenceEdit::AcceptGeneratedHold { .. }
                | OccurrenceEdit::RevertGeneratedHold,
            ..
        } => target.matches_instance(document, instance).unwrap_or(false),
        Command::EditScoped {
            target: edited,
            edit:
                deadpan_core::ScopedNodeEdit::AcceptGeneratedHold { .. }
                | deadpan_core::ScopedNodeEdit::RevertGeneratedHold,
            ..
        } => edited == target,
        Command::EditScopedMany { edits, .. } => edits.iter().any(|edit| {
            &edit.target == target
                && matches!(
                    edit.edit,
                    deadpan_core::ScopedNodeEdit::AcceptGeneratedHold { .. }
                        | deadpan_core::ScopedNodeEdit::RevertGeneratedHold
                )
        }),
        _ => false,
    }
}

fn has_birth(command: &Command) -> bool {
    match command.base_command() {
        Command::SetHoldDuration { .. } | Command::InsertAiTime { .. } => true,
        Command::EditOccurrence {
            edit: OccurrenceEdit::SetHoldDuration { .. },
            ..
        } => true,
        Command::Compound { transaction } => transaction
            .steps()
            .iter()
            .filter_map(|step| step.edit())
            .any(|edit| has_birth(edit.command.as_command())),
        _ => false,
    }
}

fn fallback(value: &HoldFallback) -> HoldVideo {
    match value {
        HoldFallback::Background => HoldVideo::Background,
        HoldFallback::Freeze { asset, timestamp } => HoldVideo::Freeze {
            asset: asset.clone(),
            timestamp: *timestamp,
        },
    }
}

fn leaf_births(
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
    targets: &CanonicalTargets<'_>,
    budget: &mut AddressBudget,
) -> Result<Vec<Birth>, StoreError> {
    if let Command::InsertAiTime { id, .. } = &request.command {
        let target = targets
            .get(id, budget)?
            .ok_or_else(|| invalid("AI insertion has no allocated Hold in its result"))?;
        let NodeKind::Hold { recipe } = &after.nodes()[id].kind else {
            return Err(invalid("AI insertion allocated a non-Hold node"));
        };
        return Ok(vec![Birth {
            target,
            origin: PreparationOrigin::inserted_pause(),
            duration: recipe.duration,
            fallback: recipe.video.clone(),
            cause: IntentCause::InsertedPause,
            authorization: IntentAuthorization::AuthoredOrigin,
        }]);
    }
    let Some(edited_node) = duration_node(&request.command) else {
        return Ok(Vec::new());
    };
    let Some(NodeKind::Hold { recipe: old }) =
        before.nodes().get(edited_node).map(|node| &node.kind)
    else {
        return Ok(Vec::new());
    };
    let HoldVideo::Generated { accepted } = &old.video else {
        return Ok(Vec::new());
    };
    // A duration leaf edits one retained identity. Follow that exact selected
    // occurrence through the core proof instead of materializing every Hold
    // in a potentially wide, deeply nested definition.
    let mut mapped = edited_node.clone();
    crate::generation_scope::with_command_proof(before, request, after, |proof| {
        let Command::EditOccurrence { instance, .. } = &request.command else {
            return Ok(());
        };
        budget.clone_steps(instance.repeats.len())?;
        budget.clone_steps(instance.repeats.len())?;
        let selected = ScopedNodeTarget {
            node: instance.node.clone(),
            repeats: instance
                .repeats
                .iter()
                .map(|step| RepeatEditStep {
                    repeat: step.node.clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: step.iteration.clone(),
                    },
                })
                .collect(),
        };
        mapped = proof.map_retained_forward(&selected).node;
        Ok(())
    })?;
    let Some(target) = targets.get(&mapped, budget)? else {
        return Ok(Vec::new());
    };
    let NodeKind::Hold { recipe } = &after.nodes()[&target.node].kind else {
        unreachable!("indexed Hold")
    };
    let mut result = Vec::new();
    if recipe.duration > accepted.artifact.sampling.output_frame_count()
        && recipe.video == fallback(&accepted.fallback)
    {
        result.push(Birth {
            target,
            origin: PreparationOrigin::AcceptedExtension {
                accepted: Box::new(accepted.artifact.clone()),
                controls: PreparationControls::AcceptedArtifact,
            },
            duration: recipe.duration,
            fallback: recipe.video.clone(),
            cause: IntentCause::DurationExtension,
            authorization: IntentAuthorization::AuthoredOrigin,
        });
    }
    Ok(result)
}

/// Derive from the real core reducer, including each Compound leaf. No caller
/// supplies replacement destinations or node maps.
pub(crate) fn derive(
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
) -> Result<Vec<Birth>, StoreError> {
    if matches!(request.command, Command::WithBoundaryReplacements { .. }) {
        let base = CommandRequest {
            command: request.command.base_command().clone(),
            ..request.clone()
        };
        let edit = deadpan_core::apply(before, &base)?;
        let intermediate = edit.forward.apply_stored(before)?;
        return derive(before, &base, &intermediate);
    }
    if !has_birth(&request.command) {
        return Ok(Vec::new());
    }
    let mut budget = AddressBudget::default();
    if !matches!(request.command, Command::Compound { .. }) {
        let canonical = CanonicalTargets::new(after, &mut budget)?;
        return leaf_births(before, request, after, &canonical, &mut budget);
    }
    let mut result: Vec<Birth> = Vec::new();
    deadpan_core::replay_compound(before, request, |visit| -> Result<(), StoreError> {
        let Some(leaf) = visit.request else {
            return Ok(());
        };
        result.retain(|birth| !changes_provider(&leaf.command, &birth.target, visit.before));
        crate::generation_scope::with_command_proof(visit.before, leaf, visit.after, |proof| {
            for birth in &mut result {
                budget.clone_steps(birth.target.repeats.len())?;
                birth.target = proof.map_retained_forward(&birth.target);
            }
            Ok(())
        })?;
        if result.is_empty() && !has_birth(&leaf.command) {
            return Ok(());
        }
        let canonical = CanonicalTargets::new(visit.after, &mut budget)?;
        result.extend(leaf_births(
            visit.before,
            leaf,
            visit.after,
            &canonical,
            &mut budget,
        )?);
        // Isolation proves cloned identities. Ordinary wrapping or ungrouping
        // can move the exact retained node to a different authored address.
        // Resolve once through the leaf's ownership index, never a shared Play.
        let mut retained = Vec::with_capacity(result.len());
        for mut birth in std::mem::take(&mut result) {
            if !canonical.matches(&birth.target, &mut budget)? {
                let Some(target) = canonical.get(&birth.target.node, &mut budget)? else {
                    continue;
                };
                birth.target = target;
            }
            let Some(NodeKind::Hold { recipe }) = visit
                .after
                .nodes()
                .get(&birth.target.node)
                .map(|node| &node.kind)
            else {
                continue;
            };
            if recipe.video != birth.fallback || !birth.origin.supports_duration(recipe.duration) {
                continue;
            }
            birth.duration = recipe.duration;
            retained.push(birth);
        }
        result = retained;
        Ok(())
    })?;
    let mut unique = BTreeMap::new();
    for birth in result {
        budget.clone_steps(birth.target.repeats.len())?;
        unique.insert(birth.target.clone(), birth);
    }
    Ok(unique.into_values().collect())
}

pub(crate) fn command_births(
    connection: &Connection,
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
) -> Result<Vec<Birth>, StoreError> {
    let mut births = derive(before, request, after)?;
    if births.is_empty() {
        return Ok(births);
    }
    let requests = crate::generation_scope::preview_command(connection, before, request, after)?;
    for birth in &mut births {
        let PreparationOrigin::AcceptedExtension { controls, .. } = &mut birth.origin else {
            continue;
        };
        if let Some(request) = requests
            .iter()
            .find(|request| request.target == birth.target)
        {
            *controls = PreparationControls::Request {
                request_id: request.request_id.clone(),
                options: GenerationOptions::from_constraints(&request.constraints),
            };
        }
    }
    Ok(births)
}

fn has_provider(command: &Command) -> bool {
    match command.base_command() {
        Command::SetHoldProvider { .. }
        | Command::AcceptGeneratedHold { .. }
        | Command::RevertGeneratedHold { .. } => true,
        Command::EditOccurrence { edit, .. } => matches!(
            edit,
            OccurrenceEdit::SetHoldProvider { .. }
                | OccurrenceEdit::AcceptGeneratedHold { .. }
                | OccurrenceEdit::RevertGeneratedHold
        ),
        Command::EditScoped { edit, .. } => matches!(
            edit,
            deadpan_core::ScopedNodeEdit::AcceptGeneratedHold { .. }
                | deadpan_core::ScopedNodeEdit::RevertGeneratedHold
        ),
        Command::EditScopedMany { edits, .. } => edits.iter().any(|edit| {
            matches!(
                edit.edit,
                deadpan_core::ScopedNodeEdit::AcceptGeneratedHold { .. }
                    | deadpan_core::ScopedNodeEdit::RevertGeneratedHold
            )
        }),
        Command::Compound { transaction } => transaction
            .steps()
            .iter()
            .filter_map(|step| step.edit())
            .any(|edit| has_provider(edit.command.as_command())),
        _ => false,
    }
}

/// Determine provider choices at each actual leaf address. The returned IDs
/// are captured before normal isolation mapping; apply them only after request
/// relevance and the authored revision have been committed in this transaction.
pub(crate) fn provider_choices(
    before: &ProjectDocument,
    request: &CommandRequest,
    heads: &[intents::IntentHead],
) -> Result<std::collections::BTreeSet<PreparationId>, StoreError> {
    if matches!(request.command, Command::WithBoundaryReplacements { .. }) {
        let base = CommandRequest {
            command: request.command.base_command().clone(),
            ..request.clone()
        };
        return provider_choices(before, &base, heads);
    }
    let mut chosen = std::collections::BTreeSet::new();
    if heads.is_empty() || !has_provider(&request.command) {
        return Ok(chosen);
    }
    if !matches!(request.command, Command::Compound { .. }) {
        for head in heads {
            if changes_provider(&request.command, &head.target, before) {
                chosen.insert(head.activation_id.clone());
            }
        }
        return Ok(chosen);
    }
    let mut budget = AddressBudget::default();
    let mut targets = Vec::with_capacity(heads.len());
    for head in heads {
        budget.clone_steps(head.target.repeats.len())?;
        targets.push((head.activation_id.clone(), head.target.clone()));
    }
    deadpan_core::replay_compound(before, request, |leaf| -> Result<(), StoreError> {
        let Some(request) = leaf.request else {
            return Ok(());
        };
        for (id, target) in &targets {
            if changes_provider(&request.command, target, leaf.before) {
                chosen.insert(id.clone());
            }
        }
        crate::generation_scope::with_command_proof(leaf.before, request, leaf.after, |proof| {
            for (_, target) in &mut targets {
                budget.clone_steps(target.repeats.len())?;
                *target = proof.map_retained_forward(target);
            }
            Ok(())
        })?;
        let canonical = CanonicalTargets::new(leaf.after, &mut budget)?;
        for (_, target) in &mut targets {
            if !canonical.matches(target, &mut budget)?
                && let Some(relocated) = canonical.get(&target.node, &mut budget)?
            {
                *target = relocated;
            }
        }
        Ok(())
    })?;
    Ok(chosen)
}

pub(crate) fn apply_command_terminals(
    connection: &Connection,
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
    acceptance: Option<&deadpan_jobs::MessageIdentity>,
) -> Result<(), StoreError> {
    apply_provider_terminals(
        connection,
        before,
        request,
        after,
        after.revision_id(),
        acceptance,
    )
}

fn apply_provider_terminals(
    connection: &Connection,
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
    revision: &RevisionId,
    acceptance: Option<&deadpan_jobs::MessageIdentity>,
) -> Result<(), StoreError> {
    let current = intents::heads(connection)?;
    let mut original = current.clone();
    crate::generation_scope::with_command_proof(before, request, after, |proof| {
        for head in &mut original {
            head.target = proof.map_retained_backward(&head.target);
        }
        Ok(())
    })?;
    let choices = provider_choices(before, request, &original)?;
    for head in current {
        if !choices.contains(&head.activation_id) {
            continue;
        }
        let reason = if let Some(identity) =
            acceptance.filter(|identity| head.request_id.as_ref() == Some(&identity.request_id))
        {
            IntentTerminalReason::Accepted {
                request_id: identity.request_id.clone(),
                attempt_id: identity.attempt_id.clone(),
            }
        } else {
            IntentTerminalReason::ProviderChoice
        };
        intents::close(
            connection,
            &head,
            IntentTerminal {
                activation_id: head.activation_id.clone(),
                at_revision: revision.clone(),
                phase: TerminalPhase::Transition,
                target: head.target.clone(),
                reason,
            },
        )?;
    }
    Ok(())
}

pub(crate) fn apply_history_terminals(
    connection: &Connection,
    entry: i64,
    redo: bool,
    after: &ProjectDocument,
) -> Result<(), StoreError> {
    if !redo {
        return Ok(());
    }
    let history = crate::validation::read_history(connection, entry)?;
    let before =
        crate::validation::read_revision(connection, history.request.expected_revision.as_str())?
            .document;
    let original_after = history.edit.forward.apply_stored(&before)?;
    // Redo restores the command's same allocated identities. No acceptance
    // worker is completing here; the historical explicit choice still closes.
    apply_provider_terminals(
        connection,
        &before,
        &history.request,
        &original_after,
        after.revision_id(),
        None,
    )
}

pub(crate) fn history_births(
    connection: &Connection,
    entry: i64,
    redo: bool,
) -> Result<Vec<Birth>, StoreError> {
    if !redo {
        return Ok(Vec::new());
    }
    let history = crate::validation::read_history(connection, entry)?;
    let after =
        crate::validation::read_revision(connection, history.request.new_revision.as_str())?
            .document;
    let mut births = Vec::new();
    for original in super::read_intent_births_at(connection, &history.request.new_revision)? {
        if original.receipt.history_id != entry
            || !original.matches_hold(&after, &original.origin_target)
        {
            return Err(invalid(
                "redo original activation differs from its authored snapshot",
            ));
        }
        births.push(Birth {
            target: original.origin_target,
            origin: original.origin,
            duration: original.duration,
            fallback: intents::fallback_video(&original.receipt.fallback),
            cause: original.receipt.cause,
            authorization: IntentAuthorization::Redo {
                original_activation: original.activation_id,
            },
        });
    }
    births.sort_by(|a, b| a.target.cmp(&b.target));
    Ok(births)
}

pub(crate) fn id_for(
    revision: &RevisionId,
    target: &ScopedNodeTarget,
) -> Result<PreparationId, StoreError> {
    let mut hash = Sha256::new();
    hash.update(b"deadpan-generation-preparation-1\0");
    hash.update(serde_json::to_vec(&(revision, target))?);
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    PreparationId::new(format!("replacement-{digest}"))
}

pub(crate) fn insert_births(
    connection: &Connection,
    after: &ProjectDocument,
    history: i64,
    births: Vec<Birth>,
) -> Result<(Vec<PreparationId>, Vec<PreparationNotice>), StoreError> {
    let mut ids = Vec::new();
    let mut displaced = Vec::new();
    let mut total = 0;
    let bindings = intents::capture_inputs(
        connection,
        after,
        &births
            .iter()
            .map(|birth| birth.target.clone())
            .collect::<Vec<_>>(),
    )?;
    let mut intent_displaced = Vec::new();
    let mut capacity = intents::ActivationCapacity::new(connection, after.revision_id())?;
    for (birth, input_binding) in births.into_iter().zip(bindings) {
        let fallback = match &birth.fallback {
            HoldVideo::Background => HoldFallback::Background,
            HoldVideo::Freeze { asset, timestamp } => HoldFallback::Freeze {
                asset: asset.clone(),
                timestamp: *timestamp,
            },
            _ => return Err(invalid("automatic intent fallback is not deterministic")),
        };
        let intent = IntentBirthReceipt {
            schema_version: 1,
            history_id: history,
            cause: birth.cause,
            authorization: birth.authorization,
            fallback,
            input_binding,
        };
        let value = StoredGenerationPreparation {
            id: id_for(after.revision_id(), &birth.target)?,
            project_id: after.project_id().clone(),
            origin_revision: after.revision_id().clone(),
            origin_target: birth.target.clone(),
            current_revision: after.revision_id().clone(),
            target: birth.target,
            duration: birth.duration,
            origin: birth.origin,
            intent,
            state: PreparationState::Queued,
            claim_sequence: 0,
            reason: None,
            request_id: None,
        };
        let json = serde_json::to_string(&value)?;
        retention::make_room(
            connection,
            charged_bytes(&value, json.len())?,
            &mut displaced,
            &mut total,
        )?;
        connection.execute("INSERT INTO generation_preparations(id,origin_revision,current_revision,history_id,state,record,charged_bytes) VALUES (?1,?2,?2,?3,'queued',?4,?5)",
            params![value.id.as_str(), value.origin_revision.as_str(), history, json, charged_bytes(&value, json.len())? as i64])?;
        intent_displaced.extend(intents::activate(
            connection,
            &value.intent_birth(),
            &mut capacity,
        )?);
        ids.push(value.id);
    }
    retention::compact(connection)?;
    let mut active_ids = Vec::new();
    for id in ids {
        if read(connection, &id)?.is_some_and(|value| value.state.is_active()) {
            active_ids.push(id);
        }
    }
    check_stored_sizes(connection)?;
    let mut notices = if total == 0 {
        Vec::new()
    } else {
        vec![PreparationNotice::QueueCapacity { displaced, total }]
    };
    if !intent_displaced.is_empty() {
        let total = intent_displaced.len() as u64;
        intent_displaced.truncate(MAX_PREPARATION_PAGE);
        notices.push(PreparationNotice::IntentCapacity {
            displaced: intent_displaced,
            total,
        });
    }
    Ok((active_ids, notices))
}

pub(crate) fn map(
    connection: &Connection,
    proof: &ValidatedScopedIsolation<'_>,
    forward: bool,
) -> Result<(), StoreError> {
    intents::map(connection, proof, forward)?;
    for (mut value, _) in all(connection)? {
        value.target = if forward {
            proof.map_retained_forward(&value.target)
        } else {
            proof.map_retained_backward(&value.target)
        };
        save(connection, &value)?;
    }
    Ok(())
}

pub(crate) fn same_hold(
    origin: &ProjectDocument,
    after: &ProjectDocument,
    value: &StoredGenerationPreparation,
    canonical: &CanonicalTargets<'_>,
    budget: &mut AddressBudget,
) -> Result<bool, StoreError> {
    let Some(NodeKind::Hold { recipe: original }) = origin
        .nodes()
        .get(&value.origin_target.node)
        .map(|node| &node.kind)
    else {
        return Ok(false);
    };
    let Some(NodeKind::Hold { recipe }) =
        after.nodes().get(&value.target.node).map(|node| &node.kind)
    else {
        return Ok(false);
    };
    Ok(canonical.matches(&value.target, budget)?
        && recipe.duration == value.duration
        && recipe.video == original.video)
}

pub(crate) fn reconcile(
    connection: &Connection,
    after: &ProjectDocument,
    resolver: Option<&dyn crate::generation::GenerationContextResolver>,
) -> Result<Vec<PreparationNotice>, StoreError> {
    let notices = intents::reconcile(connection, after)?;
    let mut records = all(connection)?;
    if records.is_empty() {
        return Ok(notices);
    }
    let prepared = resolver.and_then(|resolver| resolver.prepare_transition(after));
    let resolver = prepared.as_deref().or(resolver);
    let pictures = crate::generation_pictures::QualifiedGenerationPictures::new(connection);
    let mut budget = AddressBudget::default();
    let canonical = records
        .iter()
        .any(|(value, _)| value.state.is_active() && value.origin_revision != *after.revision_id())
        .then(|| CanonicalTargets::new(after, &mut budget))
        .transpose()?;
    records.sort_unstable_by(|(a, _), (b, _)| a.origin_revision.cmp(&b.origin_revision));
    let mut origin: Option<ProjectDocument> = None;
    for (mut value, _) in records {
        if value.state.is_active() && value.origin_revision != *after.revision_id() {
            if origin
                .as_ref()
                .is_none_or(|document| document.revision_id() != &value.origin_revision)
            {
                origin = Some(
                    crate::validation::read_revision(connection, value.origin_revision.as_str())?
                        .document,
                );
            }
            let origin = origin.as_ref().expect("origin loaded");
            let relevant = has_current_intent(connection, &value)?
                && same_hold(
                    origin,
                    after,
                    &value,
                    canonical.as_ref().expect("active ownership index"),
                    &mut budget,
                )?
                && resolver.is_some_and(|resolver| {
                    resolver.preparation_is_relevant_with_pictures(origin, after, &value, &pictures)
                });
            advance(&mut value)?;
            if !relevant {
                value.state = PreparationState::Cancelled;
                value.reason = Some("The pause, its conditioning context or its history changed before replacement generation.".into());
            } else if value.state == PreparationState::Claimed {
                value.state = PreparationState::Queued;
                value.reason = None;
            }
        }
        value.current_revision = after.revision_id().clone();
        save(connection, &value)?;
    }
    Ok(notices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{
        BeatNode, BoundaryQueryLimits, ColorPolicy, FrameRate, HoldAudio, HoldRecipe,
        IterationOrder, NodeId, PresentationBasis, Subtree,
    };

    #[test]
    fn canonical_hold_index_matches_owned_plan_addresses_without_expanding_shared_plays() {
        let id = |name: &str| NodeId::new(name).unwrap();
        let revision = |name: &str| RevisionId::new(name).unwrap();
        let empty = ProjectDocument::new(
            ProjectId::new("addresses").unwrap(),
            revision("empty"),
            PresentationBasis {
                width: 512,
                height: 320,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let recipe = HoldRecipe {
            duration: FrameDuration::new(12).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        };
        let outer_plays = IterationOrder::new(revision("outer-plays"), u32::MAX).unwrap();
        let inner_plays = IterationOrder::new(revision("inner-plays"), 2).unwrap();
        let outer_second = outer_plays.at(1).unwrap();
        let gap_play = inner_plays.at(0).unwrap();
        let mut outer = BeatNode::sequence("Outer", vec![]);
        outer.kind = NodeKind::Repeat {
            child: id("inner"),
            iterations: outer_plays,
            gap: None,
            escalation: None,
        };
        let mut inner = BeatNode::sequence("Inner", vec![]);
        inner.kind = NodeKind::Repeat {
            child: id("default"),
            iterations: inner_plays,
            gap: Some(recipe.clone()),
            escalation: None,
        };
        let request = CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: revision("fixture"),
            command: Command::Insert {
                parent: id("root"),
                index: 0,
                subtree: Subtree {
                    root: id("outer"),
                    nodes: BTreeMap::from([
                        (id("outer"), outer),
                        (id("inner"), inner),
                        (id("default"), BeatNode::hold("Default", recipe.clone())),
                        (id("private"), BeatNode::hold("Private", recipe.clone())),
                        (id("private-gap"), BeatNode::hold("Gap", recipe)),
                    ]),
                    overrides: BTreeMap::from([(
                        id("outer"),
                        deadpan_core::PlayOverrides::try_from(vec![deadpan_core::PlayOverride {
                            iteration: outer_second.clone(),
                            root: id("private"),
                        }])
                        .unwrap(),
                    )]),
                    gap_overrides: BTreeMap::from([(
                        id("inner"),
                        deadpan_core::PlayOverrides::try_from(vec![deadpan_core::PlayOverride {
                            iteration: gap_play.clone(),
                            root: id("private-gap"),
                        }])
                        .unwrap(),
                    )]),
                },
            },
        };
        let document = deadpan_core::apply(&empty, &request)
            .unwrap()
            .forward
            .apply(&empty)
            .unwrap();
        // Insert gives every retained play the command's allocation identity.
        // Read the resulting clocks so ownership assertions and the rejected
        // shared address all name real iterations in this exact document.
        let NodeKind::Repeat {
            iterations: outer_plays,
            ..
        } = &document.nodes()[&id("outer")].kind
        else {
            panic!()
        };
        let NodeKind::Repeat {
            iterations: inner_plays,
            ..
        } = &document.nodes()[&id("inner")].kind
        else {
            panic!()
        };
        let outer_first = outer_plays.at(0).unwrap();
        let outer_second = outer_plays.at(1).unwrap();
        let gap_play = inner_plays.at(0).unwrap();
        let mut budget = AddressBudget::default();
        let ownership = CanonicalTargets::new(&document, &mut budget).unwrap();
        let index: BTreeMap<_, _> = [id("default"), id("private"), id("private-gap")]
            .into_iter()
            .map(|node| {
                (
                    node.clone(),
                    ownership.get(&node, &mut budget).unwrap().unwrap(),
                )
            })
            .collect();
        assert_eq!(index.len(), 3);
        assert_eq!(
            index[&id("default")].repeats,
            vec![
                RepeatEditStep {
                    repeat: id("outer"),
                    branch: RepeatEditBranch::Default
                },
                RepeatEditStep {
                    repeat: id("inner"),
                    branch: RepeatEditBranch::Default
                },
            ]
        );
        assert_eq!(
            index[&id("private")].repeats,
            vec![RepeatEditStep {
                repeat: id("outer"),
                branch: RepeatEditBranch::Play {
                    iteration: outer_second
                }
            }]
        );
        assert_eq!(
            index[&id("private-gap")].repeats,
            vec![
                RepeatEditStep {
                    repeat: id("outer"),
                    branch: RepeatEditBranch::Default
                },
                RepeatEditStep {
                    repeat: id("inner"),
                    branch: RepeatEditBranch::Play {
                        iteration: gap_play
                    }
                },
            ]
        );
        for target in index.values() {
            target.validate(&document).unwrap();
            assert!(ownership.matches(target, &mut budget).unwrap());
        }
        let plan = deadpan_plan::RenderPlan::compile(&document).unwrap();
        let limits = BoundaryQueryLimits {
            max_scopes: 100,
            max_comparisons: 100,
        };
        let expected: BTreeMap<_, _> = plan
            .authored_hold_targets(limits)
            .unwrap()
            .into_iter()
            .map(|target| (target.node.clone(), target))
            .collect();
        assert_eq!(index, expected);
        plan.scoped_hold_boundaries_batch(&index.values().cloned().collect::<Vec<_>>(), limits)
            .unwrap();
        let mut shared = index[&id("default")].clone();
        shared.repeats[0].branch = RepeatEditBranch::Play {
            iteration: outer_first,
        };
        assert_ne!(index.get(&shared.node), Some(&shared));
        assert!(!ownership.matches(&shared, &mut budget).unwrap());
        assert!(
            plan.scoped_hold_boundaries_batch(&[shared], limits)
                .is_err()
        );
    }
}
