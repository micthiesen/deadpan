use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Instant;

use deadpan_core::CapturedFraming;

use crate::WorkingRgba16Frame;
use crate::export::{allocated, readback_layout};
use crate::{
    CaptionOverlay, ColorPipeline, FitMode, FramingLayer, OutputColor, PictureGeometry, Primaries,
    RenderError, Rgba8Frame, SampleDepth, Transfer,
};

/// Twelve flat vec4<f32> uniform values (see picture.wgsl `Parameters`).
const UNIFORM_BYTES: u64 = 192;
use crate::{color::conversion, surface::validate_dimensions};

/// Single-flight allocation ownership survives both ticket cancellation and
/// map cancellation until the submitted GPU copy has also completed.
struct ReadbackPermit {
    busy: Arc<AtomicBool>,
}

impl Drop for ReadbackPermit {
    fn drop(&mut self) {
        self.busy.store(false, Ordering::Release);
    }
}

/// One same-queue snapshot of a working target. Poll from a preparation worker;
/// this API creates no thread and performs no blocking device wait. A successful
/// poll copies at most MAX_WORKING_FRAME_BYTES into owned CPU memory. The host
/// owns the number of completed frames it retains and its polling schedule.
///
/// Deadline/cancellation are cooperative, checked before and after device
/// polling and the bounded CPU copy; they cannot interrupt a driver call.
/// Dropping a ticket cancels mapping, but does not cancel submitted GPU work.
/// Its renderer keeps the single-flight permit until both GPU completion and
/// the mapping callback have drained. Continue ordinary device polling (or
/// attempt begin_working_readback again) to drain cancelled work.
pub struct WorkingReadback {
    device: wgpu::Device,
    buffer: Option<wgpu::Buffer>,
    receive: mpsc::Receiver<Result<(), String>>,
    permit: Option<Arc<ReadbackPermit>>,
    width: u32,
    height: u32,
    stride: u32,
    length: u64,
    deadline: Instant,
}

impl WorkingReadback {
    /// None means mapping is pending. Success or failure consumes this ticket's
    /// allocation; another poll then returns ReadbackFinished. Results are never
    /// published after the captured deadline or an observed cancellation.
    pub fn poll(
        &mut self,
        cancelled: &AtomicBool,
    ) -> Result<Option<WorkingRgba16Frame>, RenderError> {
        if self.buffer.is_none() {
            return Err(RenderError::ReadbackFinished);
        }
        let result = self.poll_inner(cancelled);
        if !matches!(result, Ok(None)) {
            self.release();
        }
        result
    }

    fn poll_inner(
        &self,
        cancelled: &AtomicBool,
    ) -> Result<Option<WorkingRgba16Frame>, RenderError> {
        check_readback_control(cancelled, self.deadline)?;
        self.device
            .poll(wgpu::PollType::Poll)
            .map_err(|error| RenderError::Poll(error.to_string()))?;
        check_readback_control(cancelled, self.deadline)?;
        match self.receive.try_recv() {
            Ok(result) => result.map_err(RenderError::Readback)?,
            Err(mpsc::TryRecvError::Empty) => return Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(RenderError::Readback(
                    "mapping callback disconnected".into(),
                ));
            }
        }
        let buffer = self.buffer.as_ref().ok_or(RenderError::ReadbackFinished)?;
        let view = buffer
            .get_mapped_range(..)
            .map_err(|error| RenderError::Readback(error.to_string()))?;
        let length = usize::try_from(self.length).map_err(|_| RenderError::WorkingLayout)?;
        if view.len() != length {
            return Err(RenderError::WorkingLayout);
        }
        let mut bytes = allocated(length, 0_u8)?;
        bytes.copy_from_slice(&view);
        drop(view);
        check_readback_control(cancelled, self.deadline)?;
        WorkingRgba16Frame::new(self.width, self.height, self.stride, bytes).map(Some)
    }

    fn release(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            buffer.unmap();
        }
        self.permit.take();
    }
}

impl Drop for WorkingReadback {
    fn drop(&mut self) {
        self.release();
    }
}

fn check_readback_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), RenderError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(RenderError::ReadbackCancelled);
    }
    if Instant::now() >= deadline {
        return Err(RenderError::ReadbackDeadline);
    }
    Ok(())
}

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

    /// Linear Rec.2020 D65 composite in Rgba16Float, working 1.0 = 203 cd/m^2.
    /// There is no normalized range clamp (binary16 holds up to 65504, i.e.
    /// far above 10000 cd/m^2). Display clipping and the HDR-output preview
    /// tone map occur only in the second pass and never alter this texture.
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
    readback_busy: Arc<AtomicBool>,
    layout: wgpu::BindGroupLayout,
    layout16: wgpu::BindGroupLayout,
    interpret: wgpu::RenderPipeline,
    interpret16: wgpu::RenderPipeline,
    display: wgpu::RenderPipeline,
    caption: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    source: Option<wgpu::Texture>,
    overlay: Option<wgpu::Texture>,
    /// The identity of the caption raster currently in `overlay`.
    overlay_identity: Option<u64>,
    color_pipeline: ColorPipeline,
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
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTES),
                    },
                    count: None,
                },
            ],
        });
        let layout16 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Deadpan RGBA64 picture bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTES),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout16 = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Deadpan RGBA64 picture layout"),
            bind_group_layouts: &[Some(&layout16)],
            immediate_size: 0,
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
        let interpret16 = pipeline(
            device,
            &pipeline_layout16,
            &shader,
            "interpret16",
            wgpu::TextureFormat::Rgba16Float,
        );
        let display = pipeline(
            device,
            &pipeline_layout,
            &shader,
            "display",
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let caption = blended_pipeline(device, &pipeline_layout, &shader);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Deadpan picture parameters"),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            device: device.clone(),
            queue: queue.clone(),
            owner: Arc::new(()),
            complete: Arc::new(AtomicBool::new(true)),
            readback_busy: Arc::new(AtomicBool::new(false)),
            layout,
            layout16,
            interpret,
            interpret16,
            display,
            caption,
            uniform,
            source: None,
            overlay: None,
            overlay_identity: None,
            color_pipeline: ColorPipeline::default(),
        }
    }

    /// Select the output color branch for later submissions (default: SDR).
    /// SDR output tone-maps PQ/HLG sources per texel before compositing and
    /// leaves SDR sources bit-identical. HDR output keeps working light above
    /// 1.0 for encoder readback; only the display (SDR preview) pass
    /// tone-maps the composite. Already submitted pictures are unaffected.
    pub fn set_color_pipeline(&mut self, pipeline: ColorPipeline) {
        self.color_pipeline = pipeline;
    }

    pub const fn color_pipeline(&self) -> ColorPipeline {
        self.color_pipeline
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

    /// Snapshot the working texture after prior submissions on this renderer's
    /// queue. Call after render/render_composed: a newly allocated target has no
    /// authored picture. The copy precedes later renders on this same queue and
    /// never changes or unregisters either preview texture. Only one readback
    /// allocation may be outstanding per renderer, including cancelled work.
    ///
    /// Use on a preparation worker: submission and polling are cooperative GPU
    /// operations, not a preemptive wall-time guarantee. No wait or thread is
    /// hidden here. The captured monotonic deadline applies to all later polls.
    pub fn begin_working_readback(
        &mut self,
        target: &RenderTarget,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<WorkingReadback, RenderError> {
        if !Arc::ptr_eq(&self.owner, &target.owner) {
            return Err(RenderError::ForeignTarget);
        }
        check_readback_control(cancelled, deadline)?;
        let (stride, length) = readback_layout(target.width(), target.height())?;
        if length > self.device.limits().max_buffer_size {
            return Err(RenderError::DeviceLimit);
        }
        // Also drains callbacks for a previously dropped/cancelled ticket.
        self.device
            .poll(wgpu::PollType::Poll)
            .map_err(|error| RenderError::Poll(error.to_string()))?;
        check_readback_control(cancelled, deadline)?;
        if self.readback_busy.swap(true, Ordering::AcqRel) {
            return Err(RenderError::ReadbackBusy);
        }
        let permit = Arc::new(ReadbackPermit {
            busy: Arc::clone(&self.readback_busy),
        });
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Deadpan bounded working picture readback"),
            size: length,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Deadpan working picture snapshot"),
            });
        encoder.copy_texture_to_buffer(
            target.working.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(target.height()),
                },
            },
            target.working.size(),
        );
        self.queue.submit([encoder.finish()]);
        let submitted_permit = Arc::clone(&permit);
        self.queue
            .on_submitted_work_done(move || drop(submitted_permit));
        let (send, receive) = mpsc::sync_channel(1);
        let mapped_permit = Arc::clone(&permit);
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = send.try_send(result.map_err(|error| error.to_string()));
            drop(mapped_permit);
        });
        Ok(WorkingReadback {
            device: self.device.clone(),
            buffer: Some(buffer),
            receive,
            permit: Some(permit),
            width: target.width(),
            height: target.height(),
            stride,
            length,
            deadline,
        })
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
        self.render_framed(frame, target, [target.width(), target.height()], mode, &[])
    }

    /// Render the authored opaque black Background/Blank picture into both
    /// shared targets. This is a picture submission, not a missing-media
    /// fallback. Its working pixels are valid for the same encoder readback as
    /// a source picture, without inventing an original source clock or image.
    pub fn render_background(
        &mut self,
        target: &RenderTarget,
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        self.render_background_captioned(target, None)
    }

    /// [`Self::render_background`] with captions drawn over the black
    /// picture through the same composite pass as a source picture.
    pub fn render_background_captioned(
        &mut self,
        target: &RenderTarget,
        captions: Option<&CaptionOverlay>,
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        if !Arc::ptr_eq(&self.owner, &target.owner) {
            return Err(RenderError::ForeignTarget);
        }
        if !self.is_idle()? {
            return Err(RenderError::Busy);
        }
        if let Some(captions) = captions {
            let overlay = self.upload_overlay(captions, target)?;
            // The display pass reads only the display rows and branch flags.
            self.queue.write_buffer(
                &self.uniform,
                0,
                &background_parameters(self.color_pipeline),
            );
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Deadpan captioned black picture"),
                });
            {
                let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Deadpan opaque black working picture"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.working_view,
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
            }
            draw_over(&mut encoder, &self.caption, &overlay, &target.working_view);
            let display_bindings = self.bindings(&target.working_view);
            draw(
                &mut encoder,
                &self.display,
                &display_bindings,
                &target.display_view,
            );
            return Ok(self.submit(encoder));
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Deadpan authored black picture"),
            });
        for view in [&target.working_view, &target.display_view] {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Deadpan opaque black picture pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
        }
        Ok(self.submit(encoder))
    }

    /// Render evaluated provider-to-root framing in the committed canvas space.
    /// Geometry and all limits are checked before any upload or submission.
    pub fn render_framed(
        &mut self,
        frame: &Rgba8Frame,
        target: &RenderTarget,
        canvas: [u32; 2],
        mode: FitMode,
        layers: &[FramingLayer],
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        self.render_composed(frame, target, None, canvas, mode, layers)
    }

    /// Render a retained picture context before the current live scopes. The
    /// composed geometry validates all captured canvases before any upload.
    pub fn render_composed(
        &mut self,
        frame: &Rgba8Frame,
        target: &RenderTarget,
        context: Option<&CapturedFraming>,
        canvas: [u32; 2],
        mode: FitMode,
        layers: &[FramingLayer],
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        self.render_composed_captioned(frame, target, context, canvas, mode, layers, None)
    }

    /// [`Self::render_composed`] with captions composited over the framed
    /// picture in linear working light, before the display transform and
    /// before any working readback for encoding.
    #[allow(clippy::too_many_arguments)]
    pub fn render_composed_captioned(
        &mut self,
        frame: &Rgba8Frame,
        target: &RenderTarget,
        context: Option<&CapturedFraming>,
        canvas: [u32; 2],
        mode: FitMode,
        layers: &[FramingLayer],
        captions: Option<&CaptionOverlay>,
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        if !Arc::ptr_eq(&self.owner, &target.owner) {
            return Err(RenderError::ForeignTarget);
        }
        self.validate_size(frame.metadata().width, frame.metadata().height)?;
        if !self.is_idle()? {
            return Err(RenderError::Busy);
        }
        let metadata = frame.metadata();
        let geometry = PictureGeometry::composed(
            metadata,
            context,
            canvas,
            [target.width(), target.height()],
            mode,
            layers,
        )?;
        let parameters = parameters(frame, &geometry, self.color_pipeline)?;
        let overlay = captions
            .map(|captions| self.upload_overlay(captions, target))
            .transpose()?;
        // RGBA64 uploads exact integer codes (Rgba16Uint, a core format); the
        // shader normalizes them in f32, so every 10-bit code is preserved.
        let format = match frame.sample_depth() {
            SampleDepth::Eight => wgpu::TextureFormat::Rgba8Unorm,
            SampleDepth::Sixteen => wgpu::TextureFormat::Rgba16Uint,
        };
        if self.source.as_ref().is_none_or(|texture| {
            texture.width() != metadata.width
                || texture.height() != metadata.height
                || texture.format() != format
        }) {
            self.source = Some(self.texture(
                metadata.width,
                metadata.height,
                format,
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
        self.queue.write_buffer(&self.uniform, 0, &parameters);
        let source_view = source.create_view(&Default::default());
        let (interpret, source_bindings) = match frame.sample_depth() {
            SampleDepth::Eight => (&self.interpret, self.bindings(&source_view)),
            SampleDepth::Sixteen => (&self.interpret16, self.bindings16(&source_view)),
        };
        let display_bindings = self.bindings(&target.working_view);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Deadpan shared picture"),
            });
        draw(
            &mut encoder,
            interpret,
            &source_bindings,
            &target.working_view,
        );
        if let Some(overlay) = &overlay {
            draw_over(&mut encoder, &self.caption, overlay, &target.working_view);
        }
        draw(
            &mut encoder,
            &self.display,
            &display_bindings,
            &target.display_view,
        );
        Ok(self.submit(encoder))
    }

    /// Upload a caption overlay matching `target` and bind it for the
    /// composite pass. The CPU bytes may be released on return.
    fn upload_overlay(
        &mut self,
        captions: &CaptionOverlay,
        target: &RenderTarget,
    ) -> Result<wgpu::BindGroup, RenderError> {
        if captions.width() != target.width() || captions.height() != target.height() {
            return Err(RenderError::CaptionRaster);
        }
        if self.overlay.as_ref().is_none_or(|texture| {
            texture.width() != captions.width() || texture.height() != captions.height()
        }) {
            self.overlay_identity = None;
            self.overlay = Some(self.texture(
                captions.width(),
                captions.height(),
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                "Deadpan caption coverage upload",
            ));
        }
        let overlay = self.overlay.as_ref().expect("overlay allocated");
        // A caption usually spans many frames; its pixels are uploaded once.
        if self.overlay_identity == Some(captions.identity()) {
            return Ok(self.bindings(&overlay.create_view(&Default::default())));
        }
        self.overlay_identity = Some(captions.identity());
        self.queue.write_texture(
            overlay.as_image_copy(),
            captions.bytes(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(captions.width() * 4),
                rows_per_image: Some(captions.height()),
            },
            overlay.size(),
        );
        Ok(self.bindings(&overlay.create_view(&Default::default())))
    }

    fn submit(&self, encoder: wgpu::CommandEncoder) -> wgpu::SubmissionIndex {
        self.complete.store(false, Ordering::Release);
        let submitted = Instant::now();
        let submission = self.queue.submit([encoder.finish()]);
        deadpan_diagnostics::GPU.submitted();
        let complete = Arc::clone(&self.complete);
        // The latency ends when the callback runs, which requires a device
        // poll; it includes any delay before the owner polls.
        self.queue.on_submitted_work_done(move || {
            deadpan_diagnostics::GPU.completed(submitted.elapsed());
            complete.store(true, Ordering::Release);
        });
        submission
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

    fn bindings16(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Deadpan RGBA64 picture input"),
            layout: &self.layout16,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        })
    }
}

fn parameters(
    frame: &Rgba8Frame,
    geometry: &PictureGeometry,
    pipeline: ColorPipeline,
) -> Result<Vec<u8>, RenderError> {
    let metadata = frame.metadata();
    let transfer = match metadata.color.transfer {
        Transfer::Srgb => 0.0,
        Transfer::Rec709 => 1.0,
        Transfer::Linear => 2.0,
        Transfer::Pq => 3.0,
        Transfer::Hlg => 4.0,
    };
    let mut vectors = Vec::with_capacity(12);
    vectors.extend(geometry.sampling_parameters()?);
    vectors.push(geometry.coverage.map(|value| value as f32));
    vectors.push([0.0, transfer, 0.0, 0.0]);
    for matrix in [
        conversion(metadata.color.primaries, Primaries::Rec2020),
        conversion(Primaries::Rec2020, Primaries::Rec709),
    ] {
        for row in matrix {
            vectors.push([row[0] as f32, row[1] as f32, row[2] as f32, 0.0]);
        }
    }
    let flag = |enabled: bool| if enabled { 1.0 } else { 0.0 };
    vectors.push([
        flag(pipeline.tone_maps_source(metadata.color)),
        flag(matches!(pipeline.output, OutputColor::Hdr(_))),
        flag(metadata.color.transfer == Transfer::Hlg),
        0.0,
    ]);
    vectors.push(pipeline.tone_map.shader_parameters());
    Ok(flatten(vectors))
}

/// Uniform for the black Background picture: only the display rows and the
/// preview tone-map flag/constants are read (by the display pass).
fn background_parameters(pipeline: ColorPipeline) -> Vec<u8> {
    let mut vectors = vec![[0.0; 4]; 7];
    for row in conversion(Primaries::Rec2020, Primaries::Rec709) {
        vectors.push([row[0] as f32, row[1] as f32, row[2] as f32, 0.0]);
    }
    vectors.push([
        0.0,
        if matches!(pipeline.output, OutputColor::Hdr(_)) {
            1.0
        } else {
            0.0
        },
        0.0,
        0.0,
    ]);
    vectors.push(pipeline.tone_map.shader_parameters());
    flatten(vectors)
}

// Uniform consists solely of twelve vec4<f32> values; no native struct casts,
// unsafe code, implicit padding, or external ABI representation is involved.
fn flatten(vectors: Vec<[f32; 4]>) -> Vec<u8> {
    debug_assert_eq!(vectors.len() * 16, UNIFORM_BYTES as usize);
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

/// The caption composite: premultiplied color over the working picture, with
/// the working alpha (always 1) kept.
fn blended_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("caption"),
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
            entry_point: Some("caption"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Draw over the existing contents of `target` instead of clearing it.
fn draw_over(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bindings: &wgpu::BindGroup,
    target: &wgpu::TextureView,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Deadpan caption composite pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
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
    use super::*;
    use std::time::Duration;

    #[test]
    fn cancelled_ticket_cannot_release_inflight_copy_or_mapping_permits() {
        for gpu_finishes_first in [false, true] {
            let busy = Arc::new(AtomicBool::new(true));
            let ticket = Arc::new(ReadbackPermit {
                busy: Arc::clone(&busy),
            });
            let gpu_callback = Arc::clone(&ticket);
            let mapping_callback = Arc::clone(&ticket);
            drop(ticket);
            assert!(busy.load(Ordering::Acquire));
            if gpu_finishes_first {
                drop(gpu_callback);
                assert!(busy.load(Ordering::Acquire));
                drop(mapping_callback);
            } else {
                drop(mapping_callback);
                assert!(busy.load(Ordering::Acquire));
                drop(gpu_callback);
            }
            assert!(!busy.load(Ordering::Acquire));
        }
    }

    #[test]
    fn readback_controls_reject_expired_deadlines_and_observed_cancellation() {
        let cancelled = AtomicBool::new(false);
        let expired = Instant::now();
        assert!(matches!(
            check_readback_control(&cancelled, expired),
            Err(RenderError::ReadbackDeadline)
        ));
        let future = Instant::now() + Duration::from_secs(60);
        assert!(check_readback_control(&cancelled, future).is_ok());
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            check_readback_control(&cancelled, future),
            Err(RenderError::ReadbackCancelled)
        ));
    }

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
