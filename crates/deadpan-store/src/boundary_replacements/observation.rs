//! A canonical input descriptor and its plan-proven provider alternatives.

use super::*;
use crate::generation_inputs::{GenerationCaptureSpec, GenerationInputs};
use deadpan_core::{ExactRatio, ExtensionDirection};
use deadpan_plan::{ScopedHoldBoundaries, ScopedHoldContext};

#[derive(Serialize)]
pub(super) struct Observation {
    binding: GenerationInputBinding,
    // Same order as visit_pictures. Absence/presence and coordinates belong to
    // the binding shape, never to a guessed dependency on an authored NodeId.
    alternatives: Vec<Option<(usize, GenerationPictureIdentity)>>,
}

impl Observation {
    pub(super) fn boundaries(
        plan: &RenderPlan,
        document: &ProjectDocument,
        boundary: &ScopedHoldBoundaries,
        settings: &GenerationInputSettings,
        by_node: &BTreeMap<NodeId, usize>,
        pictures: &dyn GenerationPictures,
    ) -> Result<Self, StoreError> {
        let binding = GenerationInputBinding::from_boundaries(document, boundary, pictures)?
            .with_region(document, settings.region.as_ref())?;
        let alternatives = boundary
            .left
            .iter()
            .chain(&boundary.right)
            .map(|sample| sample_alternative(plan, document, sample, by_node, pictures))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            binding,
            alternatives,
        })
    }

    pub(super) fn context(
        plan: &RenderPlan,
        document: &ProjectDocument,
        context: &ScopedHoldContext,
        settings: &GenerationInputSettings,
        by_node: &BTreeMap<NodeId, usize>,
        pictures: &dyn GenerationPictures,
    ) -> Result<Self, StoreError> {
        let binding =
            GenerationInputBinding::from_context(document, context, settings.capture, pictures)?
                .with_region(document, settings.region.as_ref())?;
        let mut alternatives =
            Vec::with_capacity(context.pictures.len() + 2 * context.coverage.spans.len() + 2);
        for sample in &context.pictures {
            alternatives.push(sample_alternative(
                plan, document, sample, by_node, pictures,
            )?);
        }
        let opposite = match context.direction {
            ExtensionDirection::FromLeft => &context.boundaries.right,
            ExtensionDirection::FromRight => &context.boundaries.left,
        };
        if let Some(sample) = opposite {
            alternatives.push(sample_alternative(
                plan, document, sample, by_node, pictures,
            )?);
        }
        for span in &context.coverage.spans {
            let fallback = if let Some(&dependency) = by_node.get(&span.start.instance.node) {
                plan.definition_hold_fallback_span(span)
                    .map_err(invalid)?
                    .map(|span| {
                        pictures
                            .support(document, &span)
                            .map(|support| (dependency, support))
                    })
                    .transpose()?
            } else {
                None
            };
            alternatives.push(
                fallback
                    .as_ref()
                    .map(|(dependency, support)| (*dependency, support.first.clone())),
            );
            alternatives.push(fallback.map(|(dependency, support)| (dependency, support.last)));
        }
        alternatives.push(sample_alternative(
            plan,
            document,
            &context.coverage.terminal,
            by_node,
            pictures,
        )?);
        Ok(Self {
            binding,
            alternatives,
        })
    }

    pub(super) fn terms(&self, required: &GenerationInputBinding) -> Vec<decisions::MismatchTerm> {
        let required_pictures = picture_refs(&required.inputs);
        let current = picture_refs(&self.binding.inputs);
        let mut terms = Vec::with_capacity(current.len() + 1);
        terms.push(if same_shape(&self.binding, required) {
            decisions::MismatchTerm::Same
        } else {
            decisions::MismatchTerm::Different
        });
        for (index, picture) in current.into_iter().enumerate() {
            let expected = required_pictures.get(index).copied();
            terms.push(match &self.alternatives[index] {
                Some((dependency, fallback)) => decisions::MismatchTerm::from_matches(
                    *dependency,
                    Some(picture) == expected,
                    Some(fallback) == expected,
                ),
                None if Some(picture) == expected => decisions::MismatchTerm::Same,
                None => decisions::MismatchTerm::Different,
            });
        }
        terms
    }

    pub(super) fn resolve(mut self, decisions: &decisions::Decisions) -> GenerationInputBinding {
        let mut alternatives = self.alternatives.into_iter();
        visit_pictures_mut(&mut self.binding.inputs, |picture| {
            if let Some((dependency, fallback)) = alternatives
                .next()
                .expect("one alternative per observed picture")
                && decisions.nodes[dependency].replace
            {
                *picture = fallback;
            }
        });
        debug_assert!(alternatives.next().is_none());
        self.binding
    }
}

fn sample_alternative(
    plan: &RenderPlan,
    document: &ProjectDocument,
    sample: &DefinitionPictureSample,
    by_node: &BTreeMap<NodeId, usize>,
    pictures: &dyn GenerationPictures,
) -> Result<Option<(usize, GenerationPictureIdentity)>, StoreError> {
    let Some(&dependency) = by_node.get(&sample.instance.node) else {
        return Ok(None);
    };
    plan.definition_hold_fallback_picture(sample)
        .map_err(invalid)?
        .map(|picture| {
            pictures
                .identity(document, &picture)
                .map(|identity| (dependency, identity))
        })
        .transpose()
}

/// Recheck the complete retained descriptor after all provider choices have
/// been applied. Shortening retains a sampled movie's authored selection; it
/// only moves the unconditioned opposite picture relative to the anchor.
pub(super) fn matches_retained(
    current: &GenerationInputBinding,
    required: &GenerationInputBinding,
) -> bool {
    same_shape(current, required) && picture_refs(&current.inputs) == picture_refs(&required.inputs)
}

fn same_shape(current: &GenerationInputBinding, required: &GenerationInputBinding) -> bool {
    if current.frame_rate != required.frame_rate
        || current.canvas != required.canvas
        || current.region != required.region
    {
        return false;
    }
    match (&current.inputs, &required.inputs) {
        (
            GenerationInputs::Bridge {
                left: cl,
                right: cr,
            },
            GenerationInputs::Bridge {
                left: rl,
                right: rr,
            },
        ) => cl.is_some() == rl.is_some() && cr.is_some() == rr.is_some(),
        (
            GenerationInputs::Extension {
                capture: cc,
                samples: cs,
                opposite: co,
                support: cu,
                terminal: ct,
            },
            GenerationInputs::Extension {
                capture: rc,
                samples: rs,
                opposite: ro,
                support: ru,
                terminal: rt,
            },
        ) => {
            if cc != rc
                || cs.len() != rs.len()
                || cu.len() != ru.len()
                || cs.iter().zip(rs).any(|(a, b)| a.position != b.position)
                || cu
                    .iter()
                    .zip(ru)
                    .any(|(a, b)| a.start != b.start || a.end_exclusive != b.end_exclusive)
                || ct.position != rt.position
            {
                return false;
            }
            match (co, ro) {
                (None, None) => true,
                (Some(a), Some(b)) if a.position == b.position => true,
                (Some(a), Some(b)) if current.duration <= required.duration => {
                    let GenerationCaptureSpec::Extension { direction, .. } = cc else {
                        return false;
                    };
                    let shift =
                        ExactRatio::integer(current.duration.frames() - required.duration.frames());
                    let expected = match direction {
                        ExtensionDirection::FromLeft => b.position.checked_add(shift),
                        ExtensionDirection::FromRight => b.position.checked_sub(shift),
                    };
                    expected.is_ok_and(|position| position == a.position)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn picture_refs(inputs: &GenerationInputs) -> Vec<&GenerationPictureIdentity> {
    match inputs {
        GenerationInputs::Bridge { left, right } => left.iter().chain(right).collect(),
        GenerationInputs::Extension {
            samples,
            opposite,
            support,
            terminal,
            ..
        } => samples
            .iter()
            .map(|sample| &sample.picture)
            .chain(opposite.iter().map(|sample| &sample.picture))
            .chain(support.iter().flat_map(|span| [&span.first, &span.last]))
            .chain(std::iter::once(&terminal.picture))
            .collect(),
    }
}

fn visit_pictures_mut(
    inputs: &mut GenerationInputs,
    mut visit: impl FnMut(&mut GenerationPictureIdentity),
) {
    match inputs {
        GenerationInputs::Bridge { left, right } => {
            for picture in left.iter_mut().chain(right) {
                visit(picture);
            }
        }
        GenerationInputs::Extension {
            samples,
            opposite,
            support,
            terminal,
            ..
        } => {
            for sample in samples.iter_mut().chain(opposite) {
                visit(&mut sample.picture);
            }
            for span in support {
                visit(&mut span.first);
                visit(&mut span.last);
            }
            visit(&mut terminal.picture);
        }
    }
}
