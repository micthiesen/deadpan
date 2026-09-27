//! Independent catalog audio in an already checked structural owner clock.

use deadpan_core::{AudioSample, SourceAudio, SourceAudioMapping};

use super::*;
use crate::plan::CompiledSourceAudio;

/// Complete natural-rate source recipe. Placement and selection are in the
/// owner's local frame clock; offset is independently signed 48 kHz time.
/// This is a borrowed preparation operand, not a persisted sound event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSourceVoiceRecipe {
    pub source: SourceAudio,
    pub mapping: SourceAudioMapping,
    pub offset: AudioSample,
}

#[derive(Debug)]
pub(super) struct SourceVoiceProvider {
    audio: CompiledSourceAudio,
}

/// Identity of one immutable voice recipe, retained by every derived view.
/// Equal source metadata does not make separately constructed voices identical.
#[derive(Debug, Clone)]
pub struct AudioSourceVoiceIdentity(Arc<SourceVoiceProvider>);

impl PartialEq for AudioSourceVoiceIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for AudioSourceVoiceIdentity {}

/// A catalog sound owned by a checked plan scope. Its source never replaces
/// authored structure or inherits the Original's retained sampling history.
#[derive(Debug, Clone)]
pub struct AudioSourceVoice<'plan> {
    signal: AudioSignal<'plan>,
    identity: AudioSourceVoiceIdentity,
}

impl<'plan> AudioSourceVoice<'plan> {
    /// Complete input for downstream DSP. Current silent Holds are output
    /// policy and must not remove the input needed to prepare adjacent samples.
    pub fn input_signal(&self) -> AudioSignal<'plan> {
        self.signal.clone()
    }

    /// The same source with current scoped silent-Hold suppression enabled.
    /// It has no allowances yet. After Preserve, source endpoint masks are
    /// omitted by the ordinary output-policy reader so decay remains available.
    pub fn output_signal(&self) -> AudioSignal<'plan> {
        let mut signal = self.signal.clone();
        signal.provider = SignalProvider::Source {
            recipe: Arc::clone(&self.identity.0),
            apply_holds: true,
        };
        signal
    }

    pub fn identity(&self) -> AudioSourceVoiceIdentity {
        self.identity.clone()
    }

    /// The complete catalog selection, including samples hidden by a routed
    /// output mask. Hosts must admit it even for a wholly silent route query.
    pub fn source(&self) -> &SourceAudio {
        &self.identity.0.audio.source
    }

    pub(crate) fn plan(&self) -> &'plan RenderPlan {
        self.signal.plan
    }

    pub(crate) fn route_grid(&self) -> Result<AudioSampleGrid<SignalSample>, PlanError> {
        let full = ExactRatio::ZERO
            ..ExactRatio::integer(
                self.signal.plan.nodes[self.signal.root]
                    .inspection
                    .duration
                    .frames(),
            );
        if self.signal.support != full
            || self.signal.sampling_support.is_some()
            || self.signal.constrain_support
            || self.signal.placed_transform.is_some()
        {
            return Err(PlanError::InvalidPlan(
                "routed source requires its complete untransformed input capture",
            ));
        }
        self.signal.transform()?.grid(AudioBoundaryRule::PointCeil)
    }
}

impl<'plan> AudioSignal<'plan> {
    /// Attach an independent natural-rate catalog source to this checked owner.
    /// Metadata admission is necessary but not sufficient: the existing media
    /// provider must still admit the exact revision, receipt and original bytes.
    /// Non-natural processing belongs to explicit enclosing processing stages.
    pub fn source_voice(
        &self,
        recipe: AudioSourceVoiceRecipe,
    ) -> Result<AudioSourceVoice<'plan>, PlanError> {
        if !matches!(self.provider, SignalProvider::Structural) {
            return Err(PlanError::InvalidPlan(
                "a source voice requires a structural owner",
            ));
        }
        let asset =
            self.plan
                .audio_assets
                .get(&recipe.source.asset)
                .ok_or(PlanError::InvalidPlan(
                    "source voice asset is absent from the plan catalog",
                ))?;
        if asset.source_qualification.is_none() {
            return Err(PlanError::InvalidPlan(
                "source voice asset has no source qualification",
            ));
        }
        if !asset
            .audio
            .is_some_and(|span| span.contains_span(recipe.source.span))
        {
            return Err(PlanError::InvalidPlan(
                "source voice selection exceeds the catalog audio bounds or clock",
            ));
        }
        if recipe.mapping == SourceAudioMapping::FitBeat {
            return Err(PlanError::InvalidPlan(
                "source voice requires an explicit natural-rate mapping",
            ));
        }
        let rate = self.plan.metadata.presentation_basis.frame_rate;
        // FitBeat is excluded, so the duration sentinel cannot affect mapping.
        let duration = recipe.mapping.duration_frames(FrameDuration::ZERO)?;
        if duration
            != SourceAudioMapping::natural_rate(recipe.source.span, rate)?
                .duration_frames(FrameDuration::ZERO)?
        {
            return Err(PlanError::InvalidPlan(
                "source voice mapping must preserve the original audio rate",
            ));
        }
        let audio = CompiledSourceAudio {
            start: recipe
                .mapping
                .start_frames_with_offset(recipe.offset, rate)?,
            duration,
            selection: recipe.mapping.selection_frames_with_offset(
                FrameDuration::ZERO,
                recipe.offset,
                rate,
            )?,
            selected: matches!(recipe.mapping, SourceAudioMapping::SelectedPlacement { .. }),
            source: recipe.source,
        };
        let provider = Arc::new(SourceVoiceProvider { audio });
        let mut signal = self.clone();
        signal.provider = SignalProvider::Source {
            recipe: Arc::clone(&provider),
            apply_holds: false,
        };
        Ok(AudioSourceVoice {
            signal,
            identity: AudioSourceVoiceIdentity(provider),
        })
    }

    pub(in crate::plan) fn is_source_voice(&self) -> bool {
        matches!(self.provider, SignalProvider::Source { .. })
    }

    pub(in crate::plan) fn structural_hold_carrier(&self) -> Self {
        let mut carrier = self.clone();
        if self.is_source_voice() {
            carrier.provider = SignalProvider::Structural;
        }
        carrier
    }

    pub(in crate::plan) fn source_voice_hold_carrier(&self) -> Option<Self> {
        matches!(
            self.provider,
            SignalProvider::Source {
                apply_holds: true,
                ..
            }
        )
        .then(|| self.structural_hold_carrier())
    }

    pub(super) fn source_voice_span(
        &self,
        provider: &SourceVoiceProvider,
        sample: SignalSample,
        grid: AudioSampleGrid<SignalSample>,
    ) -> Result<AudioSignalSpan<'plan>, PlanError> {
        let audio = &provider.audio;
        let transform = self.transform()?;
        let start = transform.signal_from_local(audio.selection.start)?;
        let end = transform.signal_from_local(audio.selection.end)?;
        let mut extent = self.support.clone();
        let content = if sample < grid.boundary(start)? {
            extent.end = minimum(extent.end, start)?;
            AudioContent::Silence {
                reason: audio.outside_reason(),
            }
        } else if sample >= grid.boundary(end)? {
            extent.start = maximum(extent.start, end)?;
            AudioContent::Silence {
                reason: audio.outside_reason(),
            }
        } else {
            extent = intersect(extent, start..end)?;
            // Keep the complete recipe across physical fragment/run seams.
            // An explicit intrinsic input selection still constrains filtering.
            let support = if self.constrain_support {
                intersect(
                    self.sampling_support
                        .clone()
                        .unwrap_or_else(|| self.support.clone()),
                    start..end,
                )?
            } else {
                start..end
            };
            AudioContent::Source {
                source: audio.source.clone(),
                start: audio.start,
                duration: audio.duration,
                support: SourceSamplingSupport::from_local(
                    &audio.source,
                    audio.start,
                    audio.duration,
                    transform.local_at_signal_frame(support.start)?
                        ..transform.local_at_signal_frame(support.end)?,
                )?,
            }
        };
        let allocated_samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
        if !allocated_samples.contains(&sample) {
            return Err(PlanError::InvalidPlan(
                "source voice interval did not advance",
            ));
        }
        Ok(AudioSignalSpan {
            definition: self.definition.clone(),
            samples: allocated_samples.clone(),
            sampling: AudioSampleMap::new(
                allocated_samples.start,
                transform.local_at(allocated_samples.start)?,
                transform
                    .signal_frames_per_sample
                    .checked_div(transform.signal_frames_per_local_frame)?,
            )?,
            allocated_samples,
            signal_extent: extent,
            instance: InstancePath {
                node: self.plan.nodes[self.root].inspection.id.clone(),
                repeats: self.repeats.clone(),
            },
            gap_after: self.gap.as_ref().and_then(|gap| gap.after.clone()),
            transform,
            grid,
            retimes: Vec::new(),
            content: AudioSignalContent::Leaf(content),
        })
    }
}
