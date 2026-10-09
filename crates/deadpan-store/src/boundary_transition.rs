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

use crate::boundary_replacements::IntentCaptureSettings;
use crate::generation_inputs::GenerationInputSettings;
use crate::generation_intents::{
    self as intents, InputUnavailableCause, IntentAuthorization, IntentBirth, IntentCause,
    IntentHead, IntentInputBinding,
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
    if matches!(request.command, Command::RestoreSnapshot { .. }) {
        // A take explicitly chooses its complete saved composition. Reconcile
        // operational work without rewriting providers or renewing AI intents.
        return Ok(Derived {
            entries: Vec::new(),
            births: Vec::new(),
        });
    }
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
    let mut intent_nodes = BTreeMap::new();
    for head in &mapped {
        if !choices.contains(&head.activation_id)
            && let Some(settings) = intent_settings(
                connection,
                &intents::read_birth(connection, &head.activation_id)?,
            )?
        {
            intent_nodes.insert(head.target.node.clone(), settings);
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
        if birth.input_binding.as_ref().is_some_and(|binding| {
            binding.duration != birth.duration || binding.settings() != birth.settings
        }) {
            return Err(invalid(
                "derived boundary birth differs from its final input settings",
            ));
        }
        let replacement = entries_by_target
            .get(&birth.target)
            .ok_or_else(|| invalid("derived boundary birth lacks its exact provider tail"))?;
        births.push(Birth {
            target: birth.target,
            capture: Some(birth.settings.capture),
            duration: birth.duration,
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
        .map(|(target, binding)| (&target.node, (target, Some(binding))))
        .chain(
            derived
                .unavailable
                .keys()
                .map(|target| (&target.node, (target, None))),
        )
        .collect();
    for head in mapped {
        if choices.contains(&head.activation_id) || !intent_nodes.contains_key(&head.target.node) {
            continue;
        }
        let Some((target, binding)) = by_node.get(&head.target.node) else {
            continue;
        };
        let previous = intents::read_birth(connection, &head.activation_id)?;
        let Some(NodeKind::Hold { recipe }) =
            after.nodes().get(&target.node).map(|node| &node.kind)
        else {
            continue;
        };
        let fallback = intents::fallback_video(&previous.receipt.fallback);
        if recipe.video != fallback {
            continue;
        }
        let same_inputs = match (&previous.receipt.input_binding, binding) {
            (IntentInputBinding::Measured { binding: prior }, Some(current)) => {
                prior.as_ref() == *current
            }
            (
                IntentInputBinding::Unavailable {
                    cause: InputUnavailableCause::MissingContext,
                    ..
                },
                None,
            ) => true,
            _ => false,
        };
        let capture = binding
            .map(|binding| binding.capture_spec())
            .or_else(|| {
                derived
                    .unavailable
                    .get(*target)
                    .map(|settings| settings.capture)
            })
            .ok_or_else(|| invalid("resolved intent has no final capture settings"))?;
        if same_inputs
            && previous.duration == recipe.duration
            && previous.receipt.capture == Some(capture)
        {
            continue;
        }
        births.push(Birth {
            target: (*target).clone(),
            capture: Some(capture),
            origin: previous.origin,
            duration: recipe.duration,
            fallback,
            cause: if previous.duration != recipe.duration {
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

fn intent_settings(
    connection: &Connection,
    birth: &IntentBirth,
) -> Result<Option<IntentCaptureSettings>, StoreError> {
    match &birth.receipt.input_binding {
        IntentInputBinding::Measured { binding } => Ok(Some(binding.settings().into())),
        IntentInputBinding::Unavailable {
            cause: InputUnavailableCause::MissingContext,
            ..
        } => {
            let retained = if birth.origin.options().is_none() {
                birth
                    .origin
                    .accepted_artifact()
                    .map(|artifact| crate::generation_origins::read(connection, artifact))
                    .transpose()?
                    .flatten()
            } else {
                None
            };
            let options = birth
                .origin
                .options()
                .or_else(|| retained.as_ref().map(|origin| origin.options()))
                .ok_or_else(|| {
                    invalid("missing-context intent has no retained generation controls")
                })?;
            if matches!(
                options.region_target,
                deadpan_jobs::GenerationTarget::Inherit
            ) {
                return Err(invalid(
                    "missing-context intent has unresolved region controls",
                ));
            }
            let region = options.region_target.resolve(None);
            match birth.receipt.capture {
                Some(capture) => Ok(Some(GenerationInputSettings { capture, region }.into())),
                None if options.mode == deadpan_jobs::GenerationModePreference::Automatic => {
                    Ok(Some(IntentCaptureSettings::Unresolved {
                        preference: options.mode,
                        region,
                    }))
                }
                None => Ok(None),
            }
        }
        IntentInputBinding::Unavailable { .. } => Ok(None),
    }
}

fn invalid(reason: &str) -> StoreError {
    StoreError::GenerationPlan(format!("automatic boundary replacement: {reason}"))
}
