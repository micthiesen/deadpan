//! Full egui composition without a screenshot readback in measurement runs.

use std::time::{Duration, Instant};

use eframe::{egui, egui_wgpu, wgpu};

pub(crate) struct Offscreen {
    state: egui_wgpu::RenderState,
    target: Option<(wgpu::Texture, [u32; 2])>,
}

impl Offscreen {
    pub fn new(state: egui_wgpu::RenderState) -> Self {
        Self {
            state,
            target: None,
        }
    }

    /// Includes composition and waiting for this submission, but no image copy
    /// or PNG encoding. The native compositor and physical display are absent.
    pub fn submit(
        &mut self,
        context: &egui::Context,
        output: &egui::FullOutput,
    ) -> Result<(f64, f64), String> {
        let started = Instant::now();
        let size = context.content_rect().size() * context.pixels_per_point();
        if !size.is_finite() || size.x < 1.0 || size.y < 1.0 || size.x > 4096.0 || size.y > 4096.0 {
            return Err("Offscreen dimensions must be finite and within 1..=4096 pixels".into());
        }
        let dimensions = [size.x.round() as u32, size.y.round() as u32];
        if self
            .target
            .as_ref()
            .is_none_or(|(_, current)| *current != dimensions)
        {
            self.target = Some((
                self.state.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Deadpan UI feedback target"),
                    size: wgpu::Extent3d {
                        width: dimensions[0],
                        height: dimensions[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.state.target_format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                }),
                dimensions,
            ));
        }
        let target = &self.target.as_ref().ok_or("Missing offscreen target")?.0;
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: dimensions,
            pixels_per_point: context.pixels_per_point(),
        };
        let shapes = context.tessellate(output.shapes.clone(), context.pixels_per_point());
        let mut encoder =
            self.state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Deadpan UI feedback composition"),
                });
        let mut renderer = self.state.renderer.write();
        let buffers = renderer.update_buffers(
            &self.state.device,
            &self.state.queue,
            &mut encoder,
            &shapes,
            &screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Deadpan UI feedback composition"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &shapes, &screen);
        }
        let submission = self
            .state
            .queue
            .submit(buffers.into_iter().chain([encoder.finish()]));
        drop(renderer);
        let submitted_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.state
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(5)),
            })
            .map_err(|error| format!("UI GPU completion: {error}"))?;
        Ok((submitted_ms, started.elapsed().as_secs_f64() * 1000.0))
    }
}
