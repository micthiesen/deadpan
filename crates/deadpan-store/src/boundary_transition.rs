//! Store-owned automatic provider tails and their immutable intent births.
//! Admission and Compound checkpoints belong to the unchanged base command.
//! The final provider set is independently derived on preview, commit and full
//! replay; a serialized envelope never supplies its own renewal authority.

use std::collections::{BTreeMap, BTreeSet};

use deadpan_core::{
    BoundaryReplacement, BoundaryReplacementEdit, Command, CommandRequest, EditTransaction,
    HoldVideo, NodeKind, ProjectDocument, RevisionId, ValidatedDocument,
};
use rusqlite::Connection;

use crate::generation_intents::{
    self as intents, IntentAuthorization, IntentCause, IntentHead, IntentInputBinding,
};
use crate::generation_origins::AcceptedOriginReceipt;
use crate::generation_preparations::{Birth, PreparationControls, PreparationOrigin};
use crate::{CommandPlan, StoreError};

struct Derived {
    entries: Vec<BoundaryReplacement>,
    births: Vec<Birth>,
}

pub(crate) fn sort_births(births: &mut [Birth]) -> Result<(), StoreError> {
    births.sort_unstable_by(|a, b| a.target.cmp(&b.target));
    if births
        .windows(2)
        .any(|pair| pair[0].target == pair[1].target)
    {
        return Err(invalid("multiple automatic births address one final Hold"));
    }
    Ok(())
}

pub(crate) fn prepare(
    connection: &Connection,
    current: &ValidatedDocument,
    request: &CommandRequest,
    mut base: CommandPlan,
    pending: Option<&AcceptedOriginReceipt>,
) -> Result<CommandPlan, StoreError> {
    // Register-only transactions do not create an authored revision.
    if base
        .compound
        .as_ref()
        .is_some_and(|compound| compound.steps.is_empty())
    {
        require_envelope(request, &[])?;
        return Ok(base);
    }
    let base_request = base_request(request);
    let heads = intents::heads(connection)?;
    let derived = derive(
        connection,
        current,
        &base_request,
        &base.next,
        &heads,
        pending,
    )?;
    require_envelope(request, &derived.entries)?;
    if derived.entries.is_empty() {
        base.boundary_births = derived.births;
        return Ok(base);
    }
    let wrapped = CommandRequest {
        command: Command::WithBoundaryReplacements {
            edit: BoundaryReplacementEdit::new(base_request.command, derived.entries)?,
        },
        ..base_request
    };
    let (edit, next) = deadpan_core::apply_validated(current, &wrapped)?;
    let mut result = crate::command_plan(base.current, next, edit, &wrapped, base.compound)?;
    result.boundary_births = derived.births;
    Ok(result)
}

/// Replay the admitted base first, including every Compound leaf/capture, then
/// prove the exact automatic tail against historical origins and live intents.
pub(crate) fn replay(
    connection: &Connection,
    current: &ValidatedDocument,
    request: &CommandRequest,
    admitted: &BTreeSet<RevisionId>,
    heads: &[IntentHead],
) -> Result<(EditTransaction, Vec<Birth>), StoreError> {
    let base = base_request(request);
    let base_edit = if matches!(base.command, Command::Compound { .. }) {
        crate::compound::replay(connection, current, &base, admitted)?
    } else {
        deadpan_core::apply_validated(current, &base)?.0
    };
    let intermediate = current.apply_patch(&base_edit.forward)?;
    if !matches!(base.command, Command::Compound { .. }) {
        crate::compound::validate_ordinary_history(
            connection,
            current,
            &intermediate,
            &base,
            admitted,
        )?;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::source_registration::validate_hold_audio_source(
        connection,
        current,
        &intermediate,
        &base,
    )?;
    let derived = derive(connection, current, &base, &intermediate, heads, None)?;
    // Unlike first preparation, historical ordinary commands must already
    // contain the exact derived envelope when a provider tail is required.
    require_envelope(request, &derived.entries)?;
    if !derived.entries.is_empty()
        && !matches!(request.command, Command::WithBoundaryReplacements { .. })
    {
        return Err(invalid("history omitted required boundary replacements"));
    }
    let edit = if derived.entries.is_empty() {
        base_edit
    } else {
        deadpan_core::apply_validated(current, request)?.0
    };
    Ok((edit, derived.births))
}

fn base_request(request: &CommandRequest) -> CommandRequest {
    CommandRequest {
        command: request.command.base_command().clone(),
        ..request.clone()
    }
}

fn require_envelope(
    request: &CommandRequest,
    entries: &[BoundaryReplacement],
) -> Result<(), StoreError> {
    if let Command::WithBoundaryReplacements { edit } = &request.command
        && edit.replacements() != entries
    {
        return Err(invalid(
            "caller boundary replacements differ from the store-derived final set",
        ));
    }
    Ok(())
}

fn derive(
    connection: &Connection,
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ValidatedDocument,
    heads: &[IntentHead],
    pending: Option<&AcceptedOriginReceipt>,
) -> Result<Derived, StoreError> {
    let accepted = after.nodes().values().any(|node| {
        matches!(&node.kind, NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. }))
    });
    if !accepted && heads.is_empty() {
        return Ok(Derived {
            entries: Vec::new(),
            births: Vec::new(),
        });
    }
    let choices = crate::generation_preparations::provider_choices(before, request, heads)?;
    let mut mapped = heads.to_vec();
    crate::generation_scope::with_command_proof(before, request, after, |proof| {
        for head in &mut mapped {
            head.target = proof.map_retained_forward(&head.target);
        }
        Ok(())
    })?;
    let mut intent_nodes = BTreeSet::new();
    for head in &mapped {
        if !choices.contains(&head.activation_id)
            && matches!(
                intents::read_birth(connection, &head.activation_id)?
                    .receipt
                    .input_binding,
                IntentInputBinding::Measured { .. }
            )
        {
            intent_nodes.insert(head.target.node.clone());
        }
    }
    let derived = crate::boundary_replacements::derive_with_bindings(
        connection,
        after,
        &BTreeSet::new(),
        &intent_nodes,
        pending,
    )?;
    let mut births = Vec::new();
    let entries_by_target: BTreeMap<_, _> = derived
        .entries
        .iter()
        .map(|entry| (&entry.target, entry))
        .collect();
    for birth in derived.births {
        let replacement = entries_by_target
            .get(&birth.target)
            .ok_or_else(|| invalid("derived boundary birth lacks its exact provider tail"))?;
        births.push(Birth {
            target: birth.target,
            duration: birth.input_binding.duration,
            fallback: intents::fallback_video(&replacement.accepted.fallback),
            origin: PreparationOrigin::AcceptedBoundary {
                accepted: Box::new(birth.origin.artifact().clone()),
                controls: PreparationControls::Request {
                    request_id: birth.origin.request_id().clone(),
                    options: birth.origin.options().clone(),
                },
            },
            cause: birth.cause,
            authorization: IntentAuthorization::AuthoredOrigin,
        });
    }
    // Canonical targets also recover changed ancestry of a retained identity.
    // Cloned or pasted nodes never inherit a predecessor's operational head.
    let by_node: BTreeMap<_, _> = derived
        .bindings
        .iter()
        .map(|(target, binding)| (&target.node, (target, binding)))
        .collect();
    for head in mapped {
        if choices.contains(&head.activation_id) || !intent_nodes.contains(&head.target.node) {
            continue;
        }
        let Some((target, binding)) = by_node.get(&head.target.node) else {
            continue;
        };
        let previous = intents::read_birth(connection, &head.activation_id)?;
        let IntentInputBinding::Measured { binding: prior } = &previous.receipt.input_binding
        else {
            continue;
        };
        if prior == *binding {
            continue;
        }
        let Some(NodeKind::Hold { recipe }) =
            after.nodes().get(&target.node).map(|node| &node.kind)
        else {
            continue;
        };
        let fallback = intents::fallback_video(&previous.receipt.fallback);
        if recipe.video != fallback {
            continue;
        }
        births.push(Birth {
            target: (*target).clone(),
            origin: previous.origin,
            duration: recipe.duration,
            fallback,
            cause: if prior.duration != binding.duration {
                IntentCause::DurationChanged
            } else {
                IntentCause::SourceBoundaryChanged
            },
            authorization: IntentAuthorization::Renewal {
                predecessor: head.activation_id,
            },
        });
    }
    sort_births(&mut births)?;
    Ok(Derived {
        entries: derived.entries,
        births,
    })
}

fn invalid(reason: &str) -> StoreError {
    StoreError::GenerationPlan(format!("automatic boundary replacement: {reason}"))
}
