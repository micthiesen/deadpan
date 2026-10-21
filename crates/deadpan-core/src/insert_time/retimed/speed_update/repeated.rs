//! Rate admission for compact physical carriers inside one Repeat level.
use super::*;
use crate::{AudioBindingState, OwnedAudioBinding};

#[derive(Clone)]
pub(super) struct RepeatedOwner {
    pub(super) repeat: NodeId,
    pub(super) usage: RepeatOwnerUse,
}

fn binding<'a>(
    document: &ProjectDocument,
    bindings: &'a AudioBindingState,
    owner: &NodeId,
) -> Option<&'a OwnedAudioBinding> {
    if matches!(document.nodes()[owner].kind, NodeKind::Repeat { .. }) {
        bindings.gap_bindings().get(owner)
    } else {
        bindings.bindings().get(owner)
    }
}

pub(super) fn has_binding(document: &ProjectDocument, owner: &NodeId) -> bool {
    binding(document, document.audio_bindings(), owner).is_some()
}

fn spend(work: &mut usize, amount: usize) -> Result<(), EditError> {
    *work = work
        .checked_add(amount)
        .filter(|next| *next <= MAX_DOCUMENT_NODES)
        .ok_or_else(|| super::super::super::limit("exact repeated speed update work"))?;
    Ok(())
}

fn remaining(work: usize) -> Result<usize, EditError> {
    MAX_DOCUMENT_NODES
        .checked_sub(work)
        .filter(|left| *left > 0)
        .ok_or_else(|| super::super::super::limit("exact repeated speed update work"))
}

pub(super) fn validate_owner(
    document: &ProjectDocument,
    frozen: &FrozenAudioLayout,
    owner: &NodeId,
    repeated: &RepeatedOwner,
    (factor, frames_per_sample): (ExactRatio, ExactRatio),
    prospective: &mut Option<AudioBindingState>,
    work: &mut usize,
) -> Result<(), EditError> {
    let (current_scale, used) = frozen.scale_in_scope(frozen.root(), owner, remaining(*work)?)?;
    spend(work, used)?;
    let state = if has_binding(document, owner) {
        document.audio_bindings()
    } else {
        if prospective.is_none() {
            // The same bounded factory used by commit installs missing node
            // and gap templates together. Its temporary table key is local to
            // this pure query; no authored allocation or binding is changed.
            let mut timing = AudioTimingId {
                allocation: document.revision_id().clone(),
                ordinal: 0,
            };
            loop {
                spend(work, 1)?;
                if !document.audio_bindings().timings().contains_key(&timing) {
                    break;
                }
                timing.ordinal = timing.ordinal.checked_add(1).ok_or_else(|| {
                    super::super::super::limit("prospective audio timing identity")
                })?;
            }
            spend(work, document.nodes().len())?;
            *prospective = Some(crate::capture_unbound_audio_bindings(document, timing)?);
        }
        prospective
            .as_ref()
            .expect("prospective capture was prepared")
    };
    // A configured zero-duration default gap has no capture or allocation.
    let Some(binding) = binding(document, state, owner) else {
        if matches!(&document.nodes()[owner].kind,
            NodeKind::Repeat { gap, .. } if gap.as_ref().is_none_or(|gap| gap.duration == FrameDuration::ZERO))
        {
            return Ok(());
        }
        return Err(invalid(
            "the repeated audio carrier has no pre-edit capture",
        ));
    };
    let layout = state
        .timings()
        .get(&binding.lattice.reference.timing)
        .ok_or_else(|| invalid("the repeated audio carrier's timing table is missing"))?;
    let (classes, used) = crate::audio_binding::repeated_lattice_classes(
        crate::audio_binding::RepeatRateContext {
            document,
            layout: frozen,
        },
        &repeated.repeat,
        owner,
        &repeated.usage,
        &binding.lattice,
        layout,
        remaining(*work)?,
    )?;
    spend(work, used)?;
    for class in classes {
        spend(work, 1)?;
        let transfer = frames_per_sample
            .checked_div(current_scale)
            .and_then(|unit| unit.checked_div(factor))
            .and_then(|unit| unit.checked_div(class.local_frames_per_sample()?))
            .map_err(DocumentError::from)?;
        if transfer.compare_integer(64).is_gt()
            || transfer
                .checked_mul(ExactRatio::integer(64))
                .map_err(DocumentError::from)?
                .compare_integer(1)
                .is_lt()
        {
            // Class resolution proves rates, not every occurrence's rounded
            // support. No sampled play may grant a class-wide empty exception.
            return Err(invalid(
                "a repeated audio clock has an unqualified transfer outside 1/64..64; choose a closer speed",
            ));
        }
    }
    Ok(())
}
