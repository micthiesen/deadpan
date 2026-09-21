use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{FitMode, PictureGeometry, Primaries, RenderError, Rgba8Frame, Rotation, Transfer};
use crate::{color::conversion, surface::validate_dimensions};

/// Reusable destination and its owned GPU textures. Views remain stable for
/// egui registration or export readback until this target is dropped. The host
/// must keep it alive while registered and unregister before releasing it.
/// Both textures are single-layer, single-sample, and COPY_SRC capable.
pub struct RenderTarget {
    owner: Arc<()>,
    working: wgpu::Texture,
    working_view: wgpu::TextureView,
    display: wgpu::Texture,
    display_view: wgpu::TextureView,
}

impl RenderTarget {
    /// Explicitly sRGB-encoded Rec.709 RGB, opaque alpha, Rgba8Unorm format.
    /// Do not reinterpret this view as an sRGB texture and encode it again.
    pub fn display_view(&self) -> &wgpu::TextureView {
        &self.display_view
    }

    pub fn display_texture(&self) -> &wgpu::Texture {
        &self.display
    }

    /// Linear Rec.2020 D65 composite in Rgba16Float. There is no normalized
    /// range clamp; precision/range are those of IEEE binary16, not full HDR
    /// input qualification. Display clipping occurs only in the second pass.
    pub fn working_texture(&self) -> &wgpu::Texture {
        &self.working
    }

    pub fn width(&self) -> u32 {
        self.display.width()
    }

    pub fn height(&self) -> u32 {
        self.display.height()
    }
}

/// One picture submission in flight, one reusable upload texture, and two
/// reusable pipelines on the caller's device/queue. No device creation, thread,
/// media I/O, wait, or internal work queue occurs in render(). A busy caller can
/// drop stale requests and retry its latest frame after normal device polling.
pub struct PictureRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    owner: Arc<()>,
    complete: Arc<AtomicBool>,
    layout: wgpu::BindGroupLayout,
    interpret: wgpu::RenderPipeline,
    display: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    source: Option<wgpu::Texture>,
}

impl PictureRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Deadpan shared picture bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(128),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Deadpan shared picture layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Deadpan linear Rec2020 picture and SDR display"),
            source: wgpu::ShaderSource::Wgsl(include_str!("picture.wgsl").into()),
        });
        let interpret = pipeline(
            device,
            &pipeline_layout,
            &shader,
            "interpret",
            wgpu::TextureFormat::Rgba16Float,
        );
        let display = pipeline(
            device,
            &pipeline_layout,
            &shader,
            "display",
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Deadpan picture parameters"),
            size: 128,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            device: device.clone(),
            queue: queue.clone(),
            owner: Arc::new(()),
            complete: Arc::new(AtomicBool::new(true)),
            layout,
            interpret,
            display,
            uniform,
            source: None,
        }
    }

    pub fn create_target(&self, width: u32, height: u32) -> Result<RenderTarget, RenderError> {
        self.validate_size(width, height)?;
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC;
        let working = self.texture(
            width,
            height,
            wgpu::TextureFormat::Rgba16Float,
            usage,
            "Deadpan linear Rec2020 working composite",
        );
        let display = self.texture(
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            usage,
            "Deadpan encoded sRGB display",
        );
        Ok(RenderTarget {
            owner: Arc::clone(&self.owner),
            working_view: working.create_view(&Default::default()),
            display_view: display.create_view(&Default::default()),
            working,
            display,
        })
    }

    /// Poll once without waiting. False means the one allowed picture is still
    /// running. Device loss/poll errors are returned, never called completion.
    pub fn is_idle(&self) -> Result<bool, RenderError> {
        self.device
            .poll(wgpu::PollType::Poll)
            .map_err(|error| RenderError::Poll(error.to_string()))?;
        Ok(self.complete.load(Ordering::Acquire))
    }

    /// Upload a validated immutable source and encode both picture passes into
    /// one submission. The source's CPU bytes may be released on return: wgpu
    /// copies them into its staging allocation before write_texture returns.
    /// Target reads submitted later on this same queue observe this rendering.
    pub fn render(
        &mut self,
        frame: &Rgba8Frame,
        target: &RenderTarget,
        mode: FitMode,
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        if !Arc::ptr_eq(&self.owner, &target.owner) {
            return Err(RenderError::ForeignTarget);
        }
        self.validate_size(frame.metadata().width, frame.metadata().height)?;
        if !self.is_idle()? {
            return Err(RenderError::Busy);
        }
        let metadata = frame.metadata();
        let geometry = PictureGeometry::new(metadata, target.width(), target.height(), mode)?;
        if self.source.as_ref().is_none_or(|texture| {
            texture.width() != metadata.width || texture.height() != metadata.height
        }) {
            self.source = Some(self.texture(
                metadata.width,
                metadata.height,
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                "Deadpan owned source upload",
            ));
        }
        let source = self.source.as_ref().expect("source allocated");
        self.queue.write_texture(
            source.as_image_copy(),
            frame.bytes(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(metadata.row_stride_bytes),
                rows_per_image: Some(metadata.height),
            },
            source.size(),
        );
        self.queue
            .write_buffer(&self.uniform, 0, &parameters(frame, geometry));
        let source_view = source.create_view(&Default::default());
        let source_bindings = self.bindings(&source_view);
        let display_bindings = self.bindings(&target.working_view);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Deadpan shared picture"),
            });
        draw(
            &mut encoder,
            &self.interpret,
            &source_bindings,
            &target.working_view,
        );
        draw(
            &mut encoder,
            &self.display,
            &display_bindings,
            &target.display_view,
        );
        self.complete.store(false, Ordering::Release);
        let submission = self.queue.submit([encoder.finish()]);
        let complete = Arc::clone(&self.complete);
        self.queue
            .on_submitted_work_done(move || complete.store(true, Ordering::Release));
        Ok(submission)
    }

    fn validate_size(&self, width: u32, height: u32) -> Result<(), RenderError> {
        validate_dimensions(width, height)?;
        if width.max(height) > self.device.limits().max_texture_dimension_2d {
            return Err(RenderError::DeviceLimit);
        }
        Ok(())
    }

    fn texture(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
        label: &str,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    fn bindings(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Deadpan picture input"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        })
    }
}

fn parameters(frame: &Rgba8Frame, geometry: PictureGeometry) -> Vec<u8> {
    let metadata = frame.metadata();
    let rotation = match metadata.rotation {
        Rotation::None => 0.0,
        Rotation::Clockwise90 => 1.0,
        Rotation::Clockwise180 => 2.0,
        Rotation::Clockwise270 => 3.0,
    };
    let transfer = match metadata.color.transfer {
        Transfer::Srgb => 0.0,
        Transfer::Rec709 => 1.0,
        Transfer::Linear => 2.0,
    };
    let mut vectors = Vec::with_capacity(8);
    vectors.push(geometry.rectangle.map(|value| value as f32));
    vectors.push([rotation, transfer, 0.0, 0.0]);
    for matrix in [
        conversion(metadata.color.primaries, Primaries::Rec2020),
        conversion(Primaries::Rec2020, Primaries::Rec709),
    ] {
        for row in matrix {
            vectors.push([row[0] as f32, row[1] as f32, row[2] as f32, 0.0]);
        }
    }
    // Uniform consists solely of eight vec4<f32> values; no native struct casts,
    // unsafe code, implicit padding, or external ABI representation is involved.
    vectors
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect()
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fragment: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("fullscreen"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn draw(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bindings: &wgpu::BindGroup,
    target: &wgpu::TextureView,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Deadpan picture pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bindings, &[]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_shader_validates_without_a_gpu() {
        let module = wgpu::naga::front::wgsl::parse_str(include_str!("picture.wgsl"))
            .expect("shared picture WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("shared picture WGSL validates without optional shader capabilities");
    }
}
