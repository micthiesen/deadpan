use super::*;
use deadpan_core::{
    Command, CommandRequest, GeneratedArtifact, HoldFallback, HoldVideo, NodeKind, OccurrenceEdit,
    ProjectDocument, RepeatEditBranch, RepeatEditStep, ScopedNodeTarget, ValidatedScopedIsolation,
};
use deadpan_jobs::GenerationOptions;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(crate) struct Birth {
    pub target: ScopedNodeTarget,
    pub accepted: GeneratedArtifact,
    pub duration: FrameDuration,
    pub controls: PreparationControls,
    pub fallback: HoldVideo,
}

fn duration_node(command: &Command) -> Option<&deadpan_core::NodeId> {
    match command {
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
    match command {
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
            edit: deadpan_core::ScopedNodeEdit::AcceptGeneratedHold { .. },
            ..
        } => edited == target,
        Command::EditScopedMany { edits, .. } => edits.iter().any(|edit| {
            &edit.target == target
                && matches!(
                    edit.edit,
                    deadpan_core::ScopedNodeEdit::AcceptGeneratedHold { .. }
                )
        }),
        _ => false,
    }
}

fn has_duration(command: &Command) -> bool {
    match command {
        Command::SetHoldDuration { .. } => true,
        Command::EditOccurrence {
            edit: OccurrenceEdit::SetHoldDuration { .. },
            ..
        } => true,
        Command::Compound { transaction } => transaction
            .steps()
            .iter()
            .filter_map(|step| step.edit())
            .any(|edit| has_duration(edit.command.as_command())),
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

/// Traverse authored definitions, never every rendered iteration. Each sparse
/// override has its own exact Play address; an unused Default remains present.
fn targets(document: &ProjectDocument) -> Vec<ScopedNodeTarget> {
    let mut work = vec![ScopedNodeTarget {
        node: document.root().clone(),
        repeats: Vec::new(),
    }];
    let mut result = Vec::new();
    while let Some(target) = work.pop() {
        match &document.nodes()[&target.node].kind {
            NodeKind::Hold { .. } => result.push(target),
            NodeKind::Sequence { children } => {
                work.extend(children.iter().map(|node| ScopedNodeTarget {
                    node: node.clone(),
                    repeats: target.repeats.clone(),
                }))
            }
            NodeKind::Retime { child, .. } => work.push(ScopedNodeTarget {
                node: child.clone(),
                repeats: target.repeats,
            }),
            NodeKind::Repeat { child, .. } => {
                let mut repeats = target.repeats.clone();
                repeats.push(RepeatEditStep {
                    repeat: target.node.clone(),
                    branch: RepeatEditBranch::Default,
                });
                work.push(ScopedNodeTarget {
                    node: child.clone(),
                    repeats,
                });
                for overrides in [
                    document.overrides().get(&target.node),
                    document.gap_overrides().get(&target.node),
                ]
                .into_iter()
                .flatten()
                {
                    for (iteration, node) in overrides.iter() {
                        let mut repeats = target.repeats.clone();
                        repeats.push(RepeatEditStep {
                            repeat: target.node.clone(),
                            branch: RepeatEditBranch::Play {
                                iteration: iteration.clone(),
                            },
                        });
                        work.push(ScopedNodeTarget {
                            node: node.clone(),
                            repeats,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    result
}

fn leaf_births(
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
) -> Result<Vec<Birth>, StoreError> {
    let Some(edited_node) = duration_node(&request.command) else {
        return Ok(Vec::new());
    };
    let mut inverse = BTreeMap::new();
    let targets = targets(after);
    crate::generation_scope::with_command_proof(before, request, after, |proof| {
        for target in &targets {
            inverse.insert(target.clone(), proof.map_retained_backward(target));
        }
        Ok(())
    })?;
    let mut result = Vec::new();
    for target in targets {
        let previous = inverse.get(&target).unwrap_or(&target);
        if &previous.node != edited_node {
            continue;
        }
        let Some(NodeKind::Hold { recipe: old }) =
            before.nodes().get(&previous.node).map(|node| &node.kind)
        else {
            continue;
        };
        let HoldVideo::Generated { accepted } = &old.video else {
            continue;
        };
        let NodeKind::Hold { recipe } = &after.nodes()[&target.node].kind else {
            continue;
        };
        if recipe.duration > accepted.artifact.sampling.output_frame_count()
            && recipe.video == fallback(&accepted.fallback)
        {
            target.validate(after)?;
            result.push(Birth {
                target,
                accepted: accepted.artifact.clone(),
                duration: recipe.duration,
                controls: PreparationControls::AcceptedArtifact,
                fallback: recipe.video.clone(),
            });
        }
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
    if !has_duration(&request.command) {
        return Ok(Vec::new());
    }
    if !matches!(request.command, Command::Compound { .. }) {
        return leaf_births(before, request, after);
    }
    let mut result: Vec<Birth> = Vec::new();
    deadpan_core::replay_compound(before, request, |visit| -> Result<(), StoreError> {
        let Some(leaf) = visit.request else {
            return Ok(());
        };
        result.retain(|birth| !changes_provider(&leaf.command, &birth.target, visit.before));
        crate::generation_scope::with_command_proof(visit.before, leaf, visit.after, |proof| {
            for birth in &mut result {
                birth.target = proof.map_retained_forward(&birth.target);
            }
            Ok(())
        })?;
        result.extend(leaf_births(visit.before, leaf, visit.after)?);
        result.retain_mut(|birth| {
            let Some(NodeKind::Hold { recipe }) = visit
                .after
                .nodes()
                .get(&birth.target.node)
                .map(|node| &node.kind)
            else {
                return false;
            };
            if recipe.video != birth.fallback
                || recipe.duration <= birth.accepted.sampling.output_frame_count()
                || birth.target.validate(visit.after).is_err()
            {
                return false;
            }
            birth.duration = recipe.duration;
            true
        });
        Ok(())
    })?;
    let mut unique = BTreeMap::new();
    for birth in result {
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
    cancel_provider_changes(connection, before, request)?;
    let mut births = derive(before, request, after)?;
    if births.is_empty() {
        return Ok(births);
    }
    let requests = crate::generation_scope::preview_command(connection, before, request, after)?;
    for birth in &mut births {
        if let Some(request) = requests
            .iter()
            .find(|request| request.target == birth.target)
        {
            birth.controls = PreparationControls::Request {
                request_id: request.request_id.clone(),
                options: GenerationOptions::from_constraints(&request.constraints),
            };
        }
    }
    Ok(births)
}

fn has_provider(command: &Command) -> bool {
    match command {
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
        ),
        Command::EditScopedMany { edits, .. } => edits.iter().any(|edit| {
            matches!(
                edit.edit,
                deadpan_core::ScopedNodeEdit::AcceptGeneratedHold { .. }
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

/// An explicit provider choice supersedes an existing intention even when it
/// draws the same fallback. Walk compound leaves in their actual local scopes;
/// the normal isolation transition subsequently maps the stored addresses.
fn cancel_provider_changes(
    connection: &Connection,
    before: &ProjectDocument,
    request: &CommandRequest,
) -> Result<(), StoreError> {
    if !has_provider(&request.command) {
        return Ok(());
    }
    let mut pending: Vec<_> = all(connection)?
        .into_iter()
        .filter(|(value, _)| value.state.is_active())
        .map(|(value, _)| (value.target.clone(), value))
        .collect();
    let mut visit = |document: &ProjectDocument, command: &Command| -> Result<(), StoreError> {
        for (target, value) in &mut pending {
            if value.state.is_active() && changes_provider(command, target, document) {
                advance(value)?;
                value.state = PreparationState::Cancelled;
                value.reason = Some("Superseded by an explicit picture provider choice.".into());
                save(connection, value)?;
            }
        }
        Ok(())
    };
    if !matches!(request.command, Command::Compound { .. }) {
        return visit(before, &request.command);
    }
    // End the first borrow before the compound visitor also maps addresses.
    deadpan_core::replay_compound(before, request, |leaf| -> Result<(), StoreError> {
        let Some(request) = leaf.request else {
            return Ok(());
        };
        for (target, value) in &mut pending {
            if value.state.is_active() && changes_provider(&request.command, target, leaf.before) {
                advance(value)?;
                value.state = PreparationState::Cancelled;
                value.reason = Some("Superseded by an explicit picture provider choice.".into());
                save(connection, value)?;
            }
        }
        crate::generation_scope::with_command_proof(leaf.before, request, leaf.after, |proof| {
            for (target, _) in &mut pending {
                *target = proof.map_retained_forward(target);
            }
            Ok(())
        })?;
        Ok(())
    })?;
    Ok(())
}

/// Operational supersession belongs only to the writer's navigation. Full
/// history validation derives births without applying historical user actions
/// to the current queue.
pub(crate) fn history_provider_changes(
    connection: &Connection,
    entry: i64,
    redo: bool,
) -> Result<(), StoreError> {
    if !redo {
        return Ok(());
    }
    let history = crate::validation::read_history(connection, entry)?;
    if !has_provider(&history.request.command) {
        return Ok(());
    }
    let before =
        crate::validation::read_revision(connection, history.request.expected_revision.as_str())?
            .document;
    cancel_provider_changes(connection, &before, &history.request)
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
    if !has_duration(&history.request.command) {
        return Ok(Vec::new());
    }
    let before =
        crate::validation::read_revision(connection, history.request.expected_revision.as_str())?
            .document;
    let after = history.edit.forward.apply_stored(&before)?;
    let mut births = derive(&before, &history.request, &after)?;
    // Redo is a fresh intent, but retains the original captured controls even
    // though Undo deliberately made the old request stale.
    for birth in &mut births {
        let id = id_for(&history.request.new_revision, &birth.target)?;
        if let Some(record) = read(connection, &id)? {
            birth.controls = record.controls.clone();
        } else if let Some(record) = retention::read(connection, &id)? {
            birth.controls = record.controls;
        } else {
            return Err(invalid("redo has no original replacement controls proof"));
        }
    }
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
    // Derived births have unique final targets. Read the previous queue once,
    // rather than decoding every retained artifact for every resized Hold.
    let mut existing: BTreeMap<_, _> = all(connection)?
        .into_iter()
        .filter(|(value, _)| value.state.is_active())
        .map(|(value, _)| (value.target.clone(), value))
        .collect();
    for birth in births {
        if let Some(mut previous) = existing.remove(&birth.target) {
            // Earlier capacity eviction may already have retired this row.
            if read(connection, &previous.id)?.is_some() {
                advance(&mut previous)?;
                previous.state = PreparationState::Cancelled;
                previous.reason = Some("Superseded by a newer replacement duration edit.".into());
                save(connection, &previous)?;
            }
        }
        let value = StoredGenerationPreparation {
            id: id_for(after.revision_id(), &birth.target)?,
            project_id: after.project_id().clone(),
            origin_revision: after.revision_id().clone(),
            origin_target: birth.target.clone(),
            current_revision: after.revision_id().clone(),
            target: birth.target,
            duration: birth.duration,
            accepted: birth.accepted,
            controls: birth.controls,
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
    let notices = if total == 0 {
        Vec::new()
    } else {
        vec![PreparationNotice::QueueCapacity { displaced, total }]
    };
    Ok((active_ids, notices))
}

pub(crate) fn map(
    connection: &Connection,
    proof: &ValidatedScopedIsolation<'_>,
    forward: bool,
) -> Result<(), StoreError> {
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
) -> bool {
    let Some(NodeKind::Hold { recipe: original }) = origin
        .nodes()
        .get(&value.origin_target.node)
        .map(|node| &node.kind)
    else {
        return false;
    };
    let Some(NodeKind::Hold { recipe }) =
        after.nodes().get(&value.target.node).map(|node| &node.kind)
    else {
        return false;
    };
    value.target.validate(after).is_ok()
        && recipe.duration == value.duration
        && recipe.video == original.video
}

pub(crate) fn reconcile(
    connection: &Connection,
    after: &ProjectDocument,
    resolver: Option<&dyn crate::generation::GenerationContextResolver>,
) -> Result<(), StoreError> {
    let mut records = all(connection)?;
    if records.is_empty() {
        return Ok(());
    }
    let prepared = resolver.and_then(|resolver| resolver.prepare_transition(after));
    let resolver = prepared.as_deref().or(resolver);
    records.sort_unstable_by(|(a, _), (b, _)| a.origin_revision.cmp(&b.origin_revision));
    let mut origin: Option<ProjectDocument> = None;
    for (mut value, _) in records {
        if value.state.is_active() {
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
            let relevant = same_hold(origin, after, &value)
                && resolver.is_some_and(|resolver| {
                    resolver.preparation_is_relevant(origin, after, &value)
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
    Ok(())
}
