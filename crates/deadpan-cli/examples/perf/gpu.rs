//! Shared SDR picture submission on a real Metal device, as the preview worker
//! does after decoding: upload, compose framing and wait for GPU completion.

use std::time::{Duration, Instant};

use deadpan_cli::picture::{PreparedPicture, PreparedProjectPicture};
use deadpan_render::{FitMode, FramingLayer, PictureRenderer, RenderTarget, Rgba8Frame};

use crate::Result;

pub struct Gpu {
    device: wgpu::Device,
    renderer: PictureRenderer,
    target: RenderTarget,
    pub adapter: String,
}

/// Milliseconds from the start of submission to the return of `render_*`,
/// and to observed GPU completion of that submission.
pub struct Presented {
    pub submit_ms: f64,
    pub complete_ms: f64,
}

impl Gpu {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))?;
        let info = adapter.get_info();
        if info.backend != wgpu::Backend::Metal {
            return Err("performance measurement requires a real Metal adapter".into());
        }
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("Deadpan performance measurement"),
                ..Default::default()
            }))?;
        let renderer = PictureRenderer::new(&device, &queue);
        let target = renderer.create_target(width, height)?;
        Ok(Self {
            device,
            renderer,
            target,
            adapter: info.name,
        })
    }

    /// Compose an already decoded picture (a preview proxy picture) on the
    /// canvas, as the native viewer does, and wait for GPU completion.
    pub fn present_frame(
        &mut self,
        frame: &Rgba8Frame,
        canvas: [u32; 2],
        layers: &[FramingLayer],
    ) -> Result<Presented> {
        let started = Instant::now();
        let submission = self.renderer.render_composed(
            frame,
            &self.target,
            None,
            canvas,
            FitMode::Fit,
            layers,
        )?;
        self.complete(started, submission)
    }

    /// Clear the canvas to the authored Background, as for Background and
    /// Blank pictures, and wait for GPU completion.
    pub fn present_background(&mut self) -> Result<Presented> {
        let started = Instant::now();
        let submission = self.renderer.render_background(&self.target)?;
        self.complete(started, submission)
    }

    pub fn present(&mut self, prepared: &PreparedProjectPicture) -> Result<Presented> {
        let started = Instant::now();
        let layers = prepared.render_layers()?;
        let submission = match &prepared.picture {
            PreparedPicture::Frame { frame, .. } | PreparedPicture::Generated { frame, .. } => {
                self.renderer.render_composed(
                    frame,
                    &self.target,
                    prepared.picture_context.as_deref(),
                    prepared.canvas,
                    FitMode::Fit,
                    &layers,
                )?
            }
            PreparedPicture::Background => self.renderer.render_background(&self.target)?,
        };
        self.complete(started, submission)
    }

    fn complete(
        &mut self,
        started: Instant,
        submission: wgpu::SubmissionIndex,
    ) -> Result<Presented> {
        let submit_ms = crate::ms(started);
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })?;
        // The renderer's single-flight permit clears in its completion callback.
        while !self.renderer.is_idle()? {
            std::thread::yield_now();
        }
        Ok(Presented {
            submit_ms,
            complete_ms: crate::ms(started),
        })
    }
}
