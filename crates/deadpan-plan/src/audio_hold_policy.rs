//! Current structural silent-Hold rules, before any voice-specific allowance.
//! Retained timing records cannot supply historical Hold policy or identities.

use std::ops::Range;

use deadpan_core::{AudioSample, InstancePath, IterationId, ProjectId, RevisionId};
use serde::Serialize;

use super::{
    AudioContent, AudioDefinitionSelector, AudioDomain, AudioQuery, AudioQueryLimits, AudioSignal,
    AudioSignalContent, LookupStats, RenderPlan, SignalSample, SilenceReason,
};
use crate::{AudioBoundaryRule, PlanError};

/// A current policy issuer in one immutable plan revision. A definition brands
/// its relative occurrence path; it never invents an enclosing Repeat play.
/// This read-only identity grants no persisted allowance or media admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AudioHoldIssuer {
    Node {
        #[serde(skip_serializing_if = "Option::is_none")]
        definition: Option<AudioDefinitionSelector>,
        instance: InstancePath,
    },
    RepeatGap {
        #[serde(skip_serializing_if = "Option::is_none")]
        definition: Option<AudioDefinitionSelector>,
        /// The Repeat owner and its enclosing occurrences.
        instance: InstancePath,
        /// Stable preceding play, absent only for a selected gap definition.
        gap_after: Option<IterationId>,
    },
}

impl AudioHoldIssuer {
    pub(crate) fn from_span(
        definition: Option<AudioDefinitionSelector>,
        instance: InstancePath,
        gap_after: Option<IterationId>,
    ) -> Self {
        let gap_definition = matches!(
            &definition,
            Some(AudioDefinitionSelector::RepeatGap { repeat }) if repeat == &instance.node
        );
        if gap_after.is_some() || gap_definition {
            Self::RepeatGap {
                definition,
                instance,
                gap_after,
            }
        } else {
            Self::Node {
                definition,
                instance,
            }
        }
    }

    /// A durable root occurrence, without promoting an intrinsic definition's
    /// relative path into a project-wide permission. Unplayed gap definitions
    /// have no concrete preceding play and therefore cannot name an allowance.
    pub fn sound_issuer(&self) -> Option<deadpan_core::SoundHoldIssuer> {
        match self {
            Self::Node {
                definition: None,
                instance,
            } => Some(deadpan_core::SoundHoldIssuer::Node {
                instance: instance.clone(),
            }),
            Self::RepeatGap {
                definition: None,
                instance,
                gap_after: Some(gap_after),
            } => Some(deadpan_core::SoundHoldIssuer::RepeatGap {
                instance: instance.clone(),
                gap_after: gap_after.clone(),
            }),
            _ => None,
        }
    }
}

/// Default suppression issued by one explicit silent Hold on the queried grid.
/// The half-open samples are clipped to the query. Adjacent issuers stay distinct.
/// A Hold allocated no samples on this grid contributes no rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioHoldRule<S> {
    pub samples: Range<S>,
    pub issuer: AudioHoldIssuer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioHoldPolicyQuery<S> {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub samples: Range<S>,
    pub rules: Vec<AudioHoldRule<S>>,
    pub lookup: LookupStats,
    /// Indexed traversal work, including spans that issue no Hold rule.
    pub work: usize,
}

impl RenderPlan {
    /// Current explicit silent Holds on the absolute RoundEven output grid.
    /// Limits cover the entire structural query, including non-Hold spans.
    /// Fully composed retimes determine policy; retained audio bindings do not.
    pub fn audio_hold_policy(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioHoldPolicyQuery<AudioSample>, PlanError> {
        Ok(root_rules(self.audio(samples, limits)?))
    }
}

impl AudioDomain<'_> {
    /// Current explicit silent Holds in this scoped absolute root placement.
    /// This evaluates the borrowed current subtree, not an old timing layout.
    pub fn hold_policy(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioHoldPolicyQuery<AudioSample>, PlanError> {
        Ok(root_rules(self.audio(samples, limits)?))
    }
}

impl AudioSignal<'_> {
    /// Current explicit silent Holds on this signal's exact PointCeil grid.
    /// Crossing Preserve for policy inspection does not bypass its DSP. A
    /// projected processing operand requires its separately declared policy
    /// signal and cannot be treated as an implicit structural policy source.
    pub fn hold_policy(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioHoldPolicyQuery<SignalSample>, PlanError> {
        self.hold_policy_on_grid(samples, limits, AudioBoundaryRule::PointCeil)
    }

    /// Re-evaluate the structural issuers on the consuming physical grid. A
    /// source voice's input and output views expose the same issuers here;
    /// only its output view applies their default suppression during playback.
    pub(crate) fn hold_policy_on_grid(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        rule: AudioBoundaryRule,
    ) -> Result<AudioHoldPolicyQuery<SignalSample>, PlanError> {
        let carrier = self.structural_hold_carrier();
        let query = carrier.query_inner(samples, limits, rule, false, false)?;
        let mut rules = Vec::new();
        for span in query.spans {
            let AudioSignalContent::Leaf(content) = span.content else {
                return Err(PlanError::InvalidPlan(
                    "Hold policy requires a structural signal",
                ));
            };
            if is_silent_hold(&content) {
                rules.push(AudioHoldRule {
                    samples: span.samples,
                    issuer: AudioHoldIssuer::from_span(
                        span.definition,
                        span.instance,
                        span.gap_after,
                    ),
                });
            }
        }
        Ok(AudioHoldPolicyQuery {
            project_id: query.project_id,
            revision_id: query.revision_id,
            samples: query.samples,
            rules,
            lookup: query.lookup,
            work: query.work,
        })
    }
}

fn root_rules(query: AudioQuery) -> AudioHoldPolicyQuery<AudioSample> {
    AudioHoldPolicyQuery {
        project_id: query.project_id,
        revision_id: query.revision_id,
        samples: query.samples,
        rules: query
            .spans
            .into_iter()
            .filter(|span| is_silent_hold(&span.content))
            .map(|span| AudioHoldRule {
                samples: span.samples,
                issuer: AudioHoldIssuer::from_span(span.definition, span.instance, span.gap_after),
            })
            .collect(),
        lookup: query.lookup,
        work: query.work,
    }
}

fn is_silent_hold(content: &AudioContent) -> bool {
    matches!(
        content,
        AudioContent::Silence {
            reason: SilenceReason::SilentHold
        }
    )
}
