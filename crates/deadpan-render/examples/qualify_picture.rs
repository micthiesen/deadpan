//! Real offscreen Metal qualification, deliberately separate from unit tests.
//! Usage: cargo run -p deadpan-render --example qualify_picture --locked -- REPORT.json
use std::{
    error::Error,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

use deadpan_core::{ExactRatio, SourceTimeBase, SourceTimestamp};
use deadpan_render::{
    FitMode, FrameMetadata, FramingLayer, MAX_DIMENSION, PictureGeometry, PictureRenderer,
    Primaries, RenderError, Rgba8Frame, Rotation, SampleAspectRatio, SourceColor, Transfer,
    reference_pixel, reference_pixel_with_geometry,
};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 1 {
        return Err("usage: qualify_picture REPORT.json (new file)".into());
    }
    let path = PathBuf::from(&arguments[0]);
    // Reserve the report before GPU work. A failed run retains a failure record.
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    let started = Instant::now();
    let result = qualify();
    let report = match &result {
        Ok(report) => report.clone(),
        Err(error) => json!({"schema_version": 1, "status": "failed", "error": error.to_string()}),
    };
    serde_json::to_writer_pretty(&mut file, &report)?;
    writeln!(file)?;
    file.sync_all()?;
    result?;
    println!(
        "Metal picture qualification passed in {:.3}s; report: {}",
        started.elapsed().as_secs_f64(),
        path.display()
    );
    Ok(())
}

fn qualify() -> Result<Value> {
    if !cfg!(target_os = "macos") {
        return Err(
            "this qualification requires macOS Metal; no fallback backend is permitted".into(),
        );
    }
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
        return Err(format!("required Metal, received {:?}", info.backend).into());
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Deadpan picture qualification"),
        ..Default::default()
    }))?;
    let mut renderer = PictureRenderer::new(&device, &queue);
    if !matches!(renderer.create_target(0, 1), Err(RenderError::Dimensions))
        || !matches!(
            renderer.create_target(MAX_DIMENSION + 1, 1),
            Err(RenderError::Dimensions)
        )
        || !matches!(
            renderer.create_target(8192, 8192),
            Err(RenderError::Dimensions)
        )
    {
        return Err("output size admission failed".into());
    }
    let mut cases = Vec::new();
    for primaries in [
        Primaries::Rec709,
        Primaries::Rec2020,
        Primaries::DisplayP3D65,
    ] {
        for transfer in [Transfer::Srgb, Transfer::Rec709, Transfer::Linear] {
            for rotation in [
                Rotation::None,
                Rotation::Clockwise90,
                Rotation::Clockwise180,
                Rotation::Clockwise270,
            ] {
                for mode in [FitMode::Fit, FitMode::Fill] {
                    let frame = fixture(
                        7,
                        5,
                        12,
                        SourceColor {
                            transfer,
                            primaries,
                        },
                        rotation,
                        SampleAspectRatio::new(4, 3)?,
                    )?;
                    let label =
                        format!("odd-padded-{primaries:?}-{transfer:?}-{rotation:?}-{mode:?}");
                    cases.push(check_case(
                        &device,
                        &queue,
                        &mut renderer,
                        &frame,
                        (13, 9),
                        mode,
                        &label,
                    )?);
                }
            }
        }
    }
    let color = SourceColor {
        transfer: Transfer::Srgb,
        primaries: Primaries::Rec709,
    };
    // Independently known black/white interpolation and transparent-edge cases.
    for (label, bytes, expected) in [
        (
            "linear-light-midpoint",
            vec![0, 0, 0, 255, 255, 255, 255, 255],
            [188, 188, 188, 255],
        ),
        (
            "transparent-magenta-no-fringe",
            vec![255, 0, 255, 0, 0, 255, 0, 255],
            [0, 188, 0, 255],
        ),
    ] {
        let frame = Rgba8Frame::new(
            metadata(2, 1, 0, color, Rotation::None, SampleAspectRatio::SQUARE)?,
            bytes,
        )?;
        if reference_pixel(&frame, 1, 1, FitMode::Fill, 0, 0)? != expected {
            return Err("independent reference anchor failed".into());
        }
        cases.push(check_case(
            &device,
            &queue,
            &mut renderer,
            &frame,
            (1, 1),
            FitMode::Fill,
            label,
        )?);
    }
    let frame = fixture(1, 1, 4, color, Rotation::None, SampleAspectRatio::SQUARE)?;
    cases.push(check_case(
        &device,
        &queue,
        &mut renderer,
        &frame,
        (7, 3),
        FitMode::Fit,
        "single-pixel-fit-bars",
    )?);
    let frame = fixture(
        193,
        109,
        28,
        color,
        Rotation::Clockwise270,
        SampleAspectRatio::new(10, 11)?,
    )?;
    cases.push(check_case(
        &device,
        &queue,
        &mut renderer,
        &frame,
        (319, 181),
        FitMode::Fill,
        "larger-anamorphic-padded",
    )?);

    let half_ratio = ExactRatio::new(1, 2)?;
    let child = FramingLayer::new(
        [ExactRatio::new(3, 5)?, half_ratio],
        ExactRatio::new(27, 20)?,
    )?;
    let parent = FramingLayer::new([ExactRatio::new(2, 5)?, half_ratio], ExactRatio::new(3, 4)?)?;
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        let frame = fixture(7, 5, 12, color, rotation, SampleAspectRatio::new(4, 3)?)?;
        cases.push(check_framed_case(
            &device,
            &queue,
            &mut renderer,
            &frame,
            FramedCase {
                size: (31, 17),
                canvas: [16, 9],
                mode: FitMode::Fit,
                layers: &[child, FramingLayer::identity(), parent],
                label: &format!("framed-nested-anamorphic-{rotation:?}"),
                strict: false,
            },
        )?);
    }
    let white = Rgba8Frame::new(
        metadata(4, 4, 0, color, Rotation::None, SampleAspectRatio::SQUARE)?,
        vec![255; 64],
    )?;
    let zoom_in = FramingLayer::new([half_ratio; 2], ExactRatio::integer(2))?;
    let zoom_out = FramingLayer::new([half_ratio; 2], half_ratio)?;
    let small = FramingLayer::new([half_ratio; 2], ExactRatio::new(1, 4)?)?;
    let after_edge = FramingLayer::new(
        [half_ratio.checked_sub(ExactRatio::new(1, 1 << 32)?)?; 2],
        ExactRatio::new(1, 4)?,
    )?;
    for (label, layers) in [
        (
            "framed-child-crop-retained-after-group-zoom-out",
            vec![zoom_in, zoom_out],
        ),
        (
            "framed-provider-identity-before-group-zoom",
            vec![FramingLayer::identity(), zoom_out],
        ),
        ("framed-exact-half-open-coverage-edge", vec![small]),
        ("framed-q32-after-half-open-coverage-edge", vec![after_edge]),
    ] {
        cases.push(check_framed_case(
            &device,
            &queue,
            &mut renderer,
            &white,
            FramedCase {
                size: (4, 4),
                canvas: [4, 4],
                mode: FitMode::Fit,
                layers: &layers,
                label,
                strict: true,
            },
        )?);
    }

    // Read the actual working intermediate. P3 red has a small negative blue
    // component in Rec2020, which normalized storage would incorrectly discard.
    let color = SourceColor {
        transfer: Transfer::Linear,
        primaries: Primaries::DisplayP3D65,
    };
    let frame = Rgba8Frame::new(
        metadata(1, 1, 0, color, Rotation::None, SampleAspectRatio::SQUARE)?,
        vec![255, 0, 0, 255],
    )?;
    let target = renderer.create_target(1, 1)?;
    renderer.render(&frame, &target, FitMode::Fit)?;
    let bytes = readback(&device, &queue, target.working_texture(), 8)?;
    let working: Vec<f64> = bytes
        .chunks_exact(2)
        .map(|pair| half(u16::from_le_bytes([pair[0], pair[1]])))
        .collect();
    let expected = deadpan_render::source_to_working([1.0, 0.0, 0.0], color);
    if !working.iter().all(|value| value.is_finite()) {
        return Err("nonfinite working texture pixels".into());
    }
    for channel in 0..3 {
        if (working[channel] - expected[channel]).abs() > 0.0005 {
            return Err("working Rec2020 readback mismatch".into());
        }
    }
    if working[2] >= -0.001 {
        return Err("working texture clamped negative gamut coordinate".into());
    }
    let another_renderer = PictureRenderer::new(&device, &queue);
    let foreign_target = another_renderer.create_target(1, 1)?;
    if !matches!(
        renderer.render(&frame, &foreign_target, FitMode::Fit),
        Err(RenderError::ForeignTarget)
    ) {
        return Err("foreign target admission failed".into());
    }
    Ok(json!({
        "schema_version": 1, "status": "passed", "renderer": "deadpan-render-0.1.0", "wgpu": "30.0.1",
        "adapter": {"name": info.name, "backend": format!("{:?}", info.backend), "driver": info.driver, "driver_info": info.driver_info, "vendor": info.vendor, "device": info.device, "device_type": format!("{:?}", info.device_type)},
        "platform": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "provenance": {
            "git_head": command_text("git", &["rev-parse", "HEAD"] )?,
            "crate_working_tree_status": command_text("git", &["status", "--short", "--", "."] )?,
            "rustc": command_text("rustc", &["--version"] )?,
            "macos_version": command_text("sw_vers", &["-productVersion"] )?,
        },
        "case_count": cases.len(), "cases": cases,
        "working_p3_red_rgba": working, "working_absolute_tolerance": 0.0005,
        "display_channel_tolerance_codes": 2,
        "input": "full-range straight-alpha progressive RGBA8; row padding retained; original PTS unchanged",
        "working": "linear Rec2020 D65 RGBA16Float, no normalized clamp",
        "output": "Rec709 primaries, explicit sRGB encoding, opaque RGBA8Unorm; SDR clipping only",
        "checks": ["invalid output limits rejected", "foreign targets rejected", "all output pixels compared to f64 CPU reference", "negative working gamut coordinate retained", "canonical-canvas nested framing and upright source interpretation", "exact half-open coverage and retained child clips"],
        "limitations": ["synthetic SDR input only", "no HDR tone mapping or HDR input", "no ICC display management", "no encoder/output-file verification", "no realtime throughput claim", "no UI/Metal interop qualification"]
    }))
}

fn command_text(program: &str, arguments: &[&str]) -> Result<String> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;
    if !output.status.success() || output.stdout.len() > 4096 {
        return Err(format!("could not record qualification provenance from {program}").into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn metadata(
    width: u32,
    height: u32,
    padding: u32,
    color: SourceColor,
    rotation: Rotation,
    sample_aspect_ratio: SampleAspectRatio,
) -> Result<FrameMetadata> {
    Ok(FrameMetadata {
        width,
        height,
        row_stride_bytes: width * 4 + padding,
        color,
        rotation,
        sample_aspect_ratio,
        pts: SourceTimestamp {
            ticks: -9001,
            time_base: SourceTimeBase::new(1, 90_000)?,
        },
    })
}

fn fixture(
    width: u32,
    height: u32,
    padding: u32,
    color: SourceColor,
    rotation: Rotation,
    sar: SampleAspectRatio,
) -> Result<Rgba8Frame> {
    let meta = metadata(width, height, padding, color, rotation, sar)?;
    let mut bytes =
        vec![0xa7; usize::try_from(u64::from(meta.row_stride_bytes) * u64::from(height))?];
    let patches = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
        [0, 0, 0, 255],
        [128, 128, 128, 255],
        [255, 0, 255, 0],
        [255, 255, 0, 128],
        [10, 20, 21, 255],
    ];
    for y in 0..height {
        for x in 0..width {
            let offset = usize::try_from(
                u64::from(y) * u64::from(meta.row_stride_bytes) + u64::from(x) * 4,
            )?;
            let patch = usize::try_from((x + 3 * y) % u32::try_from(patches.len())?)?;
            bytes[offset..offset + 4].copy_from_slice(&patches[patch]);
        }
    }
    Ok(Rgba8Frame::new(meta, bytes)?)
}

fn check_case(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut PictureRenderer,
    frame: &Rgba8Frame,
    size: (u32, u32),
    mode: FitMode,
    label: &str,
) -> Result<Value> {
    check_framed_case(
        device,
        queue,
        renderer,
        frame,
        FramedCase {
            size,
            canvas: [size.0, size.1],
            mode,
            layers: &[],
            label,
            strict: false,
        },
    )
}

struct FramedCase<'a> {
    size: (u32, u32),
    canvas: [u32; 2],
    mode: FitMode,
    layers: &'a [FramingLayer],
    label: &'a str,
    strict: bool,
}

fn check_framed_case(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut PictureRenderer,
    frame: &Rgba8Frame,
    case: FramedCase<'_>,
) -> Result<Value> {
    let (width, height) = case.size;
    let label = case.label;
    let geometry = PictureGeometry::framed(
        frame.metadata(),
        case.canvas,
        [width, height],
        case.mode,
        case.layers,
    )?;
    let target = renderer.create_target(width, height)?;
    let started = Instant::now();
    renderer.render_framed(frame, &target, case.canvas, case.mode, case.layers)?;
    let submitted_ms = started.elapsed().as_secs_f64() * 1000.0;
    let actual = readback(device, queue, target.display_texture(), 4)?;
    let readback_ms = started.elapsed().as_secs_f64() * 1000.0;
    if !renderer.is_idle()? {
        return Err("submission callback did not release the renderer after readback".into());
    }
    let mut max_difference = 0;
    for y in 0..height {
        for x in 0..width {
            let expected = reference_pixel_with_geometry(frame, &geometry, x, y)?;
            let offset = usize::try_from((u64::from(y) * u64::from(width) + u64::from(x)) * 4)?;
            for channel in 0..4 {
                let difference = actual[offset + channel].abs_diff(expected[channel]);
                max_difference = max_difference.max(difference);
                if difference > if case.strict { 0 } else { 2 } {
                    return Err(format!("{label} ({x},{y}) channel {channel}: GPU {} reference {} difference {difference}", actual[offset + channel], expected[channel]).into());
                }
            }
        }
    }
    Ok(
        json!({"name": label, "source_size": [frame.metadata().width, frame.metadata().height], "source_stride": frame.metadata().row_stride_bytes,
        "target_size": [width, height], "canvas": case.canvas, "framing": case.layers.iter().map(|layer| layer.pose()).collect::<Vec<_>>(),
        "channel_tolerance": if case.strict { 0 } else { 2 }, "max_channel_difference": max_difference, "submit_cpu_ms": submitted_ms, "submit_through_readback_ms": readback_ms}),
    )
}

fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    bytes_per_pixel: u32,
) -> Result<Vec<u8>> {
    let row_bytes = texture.width() * bytes_per_pixel;
    let padded =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Deadpan bounded qualification readback"),
        size: u64::from(padded) * u64::from(texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    let submission = queue.submit([encoder.finish()]);
    let (send, receive) = mpsc::sync_channel(1);
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = send.send(result);
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: Some(Duration::from_secs(10)),
    })?;
    receive.recv_timeout(Duration::from_secs(1))??;
    let view = buffer.get_mapped_range(..)?;
    let mut bytes = Vec::with_capacity(usize::try_from(
        u64::from(row_bytes) * u64::from(texture.height()),
    )?);
    for row in view.chunks_exact(usize::try_from(padded)?) {
        bytes.extend_from_slice(&row[..usize::try_from(row_bytes)?]);
    }
    drop(view);
    buffer.unmap();
    Ok(bytes)
}

fn half(bits: u16) -> f64 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f64::from(bits & 1023);
    match exponent {
        0 => sign * 2.0_f64.powi(-14) * fraction / 1024.0,
        31 if fraction == 0.0 => sign * f64::INFINITY,
        31 => f64::NAN,
        _ => sign * 2.0_f64.powi(exponent - 15) * (1.0 + fraction / 1024.0),
    }
}
