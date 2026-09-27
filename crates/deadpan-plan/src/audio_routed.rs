//! Checked complete providers for retained physical sound routes.

use std::{ops::Range, sync::Arc};

use deadpan_core::{AudioSample, ExactRatio, MIX_SAMPLE_RATE};

use crate::{
    AudioBoundaryRule, AudioProjectedRoot, AudioSampleGrid, AudioSoundRoute, AudioSourceVoice,
    AudioStageProjection, PlanError, RenderPlan, SignalSample,
};

/// The closed set of complete intrinsic providers that can retain sampled
/// routing. A source always means its input view, before current Hold gates.
/// Projection identity and its complete preparation history remain shared.
#[derive(Debug, Clone)]
pub enum AudioRoutedSignalInput<'plan> {
    Source(Box<AudioSourceVoice<'plan>>),
    Projected(Arc<AudioStageProjection<'plan>>),
}

/// One immutable provider captured on its original PointCeil Recipe grid.
/// Routes select its old physical output without narrowing source/DSP support.
/// This preparation handle does not apply current consuming Hold policy or
/// creative edges and does not persist or admit media.
#[derive(Debug, Clone)]
pub struct AudioRoutedSignal<'plan> {
    input: AudioRoutedSignalInput<'plan>,
    route: AudioSoundRoute<SignalSample>,
}

impl<'plan> AudioRoutedSignal<'plan> {
    pub fn source(
        voice: AudioSourceVoice<'plan>,
        route: AudioSoundRoute<SignalSample>,
    ) -> Result<Self, PlanError> {
        let grid = voice.route_grid()?;
        let support = voice.input_signal().support();
        validate_signal_capture(&route, grid, support.end)?;
        Ok(Self {
            input: AudioRoutedSignalInput::Source(Box::new(voice)),
            route,
        })
    }

    pub fn projected(
        projection: Arc<AudioStageProjection<'plan>>,
        route: AudioSoundRoute<SignalSample>,
    ) -> Result<Self, PlanError> {
        let rate = projection
            .stage()
            .plan()
            .metadata()
            .presentation_basis
            .frame_rate;
        let grid = AudioSampleGrid::new(
            ExactRatio::ZERO,
            ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
            AudioBoundaryRule::PointCeil,
        )?;
        validate_signal_capture(
            &route,
            grid,
            ExactRatio::integer(projection.output_frames()),
        )?;
        Ok(Self {
            input: AudioRoutedSignalInput::Projected(projection),
            route,
        })
    }

    pub fn input(&self) -> &AudioRoutedSignalInput<'plan> {
        &self.input
    }

    pub fn route(&self) -> &AudioSoundRoute<SignalSample> {
        &self.route
    }

    pub fn samples(&self) -> Range<SignalSample> {
        self.route.samples()
    }

    pub fn plan(&self) -> &'plan RenderPlan {
        match &self.input {
            AudioRoutedSignalInput::Source(voice) => voice.plan(),
            AudioRoutedSignalInput::Projected(projection) => projection.stage().plan(),
        }
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan(), plan)
    }
}

fn validate_signal_capture(
    route: &AudioSoundRoute<SignalSample>,
    grid: AudioSampleGrid<SignalSample>,
    extent: ExactRatio,
) -> Result<(), PlanError> {
    if route.recipe_grid() != grid
        || route.route().recipe_extent() != extent
        || route.recipe_samples() != (grid.boundary(ExactRatio::ZERO)?..grid.boundary(extent)?)
    {
        return Err(PlanError::InvalidPlan(
            "sound route does not match its complete intrinsic provider capture",
        ));
    }
    Ok(())
}

/// A complete projected output captured on its original absolute RoundEven
/// labels. The route's Recipe frames are relative to that output's start;
/// the retained root supplies the original phase, support and policy.
#[derive(Debug, Clone)]
pub struct AudioRoutedRoot<'plan> {
    root: AudioProjectedRoot<'plan>,
    route: AudioSoundRoute<AudioSample>,
}

impl<'plan> AudioRoutedRoot<'plan> {
    pub fn new(
        root: AudioProjectedRoot<'plan>,
        route: AudioSoundRoute<AudioSample>,
    ) -> Result<Self, PlanError> {
        let grid = root.route_grid()?;
        let extent = root.extent();
        if route.recipe_grid() != grid
            || route.route().recipe_extent() != extent.end.checked_sub(extent.start)?
            || route.recipe_samples() != root.samples()
        {
            return Err(PlanError::InvalidPlan(
                "sound route does not match its complete projected root capture",
            ));
        }
        Ok(Self { root, route })
    }

    pub fn root(&self) -> &AudioProjectedRoot<'plan> {
        &self.root
    }

    pub fn route(&self) -> &AudioSoundRoute<AudioSample> {
        &self.route
    }

    pub fn samples(&self) -> Range<AudioSample> {
        self.route.samples()
    }

    pub fn plan(&self) -> &'plan RenderPlan {
        self.root.projection().stage().plan()
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        self.root.belongs_to(plan)
    }
}
