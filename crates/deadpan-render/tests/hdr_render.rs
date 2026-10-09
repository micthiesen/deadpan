//! Real Metal HDR branch: RGBA64 PQ/HLG uploads decode to working light
//! (working 1.0 = 203 cd/m^2) matching the f64 reference, HDR output keeps
//! values above reference white for P10 encoder pixels, and both the SDR
//! export branch and the HDR preview tone map match the reference-white-
//! preserving tone map reference.
#![cfg(target_os = "macos")]

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{SourceTimeBase, SourceTimestamp};
use deadpan_render::{
    ColorPipeline, FitMode, FrameMetadata, HdrTransfer, PictureGeometry, PictureRenderer,
    Primaries, Rec2100Yuv420P10Frame, RenderError, RenderTarget, Rgba8Frame, Rotation,
    SampleAspectRatio, SourceColor, ToneMap, Transfer, WorkingRgba16Frame, pq_inverse_eotf,
    reference_pixel_with_pipeline, reference_working_with_geometry, working_to_rec2100_nonlinear,
};

const WIDTH: u32 = 48;
const HEIGHT: u32 = 16;

fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .expect("a Metal adapter");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Deadpan HDR render test"),
        ..Default::default()
    }))
    .expect("a Metal device")
}

/// Signal codes (16-bit) covering black, near black, mid, reference white,
/// highlights and full scale, plus saturated primaries and partial alpha.
fn code(x: u32, y: u32) -> [u16; 4] {
    let ramp = (x * 65535 / (WIDTH - 1)) as u16;
    match y {
        0..=7 => [ramp, ramp, ramp, 65535],
        8 | 9 => [ramp, 0, 0, 65535],
        10 | 11 => [0, ramp, 0, 65535],
        12 | 13 => [0, 0, ramp, 65535],
        14 => [ramp, 65535 - ramp, 40000, 65535],
        _ => [50000, 30000, 10000, (x * 1365) as u16],
    }
}

fn frame16(transfer: Transfer) -> Rgba8Frame {
    let stride = WIDTH * 8 + 16;
    let mut bytes = vec![0xab; (stride * HEIGHT) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let offset = (y * stride + x * 8) as usize;
            for (channel, value) in code(x, y).into_iter().enumerate() {
                bytes[offset + channel * 2..offset + channel * 2 + 2]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    Rgba8Frame::new_rgba16(metadata(transfer, stride), bytes).unwrap()
}

fn metadata(transfer: Transfer, stride: u32) -> FrameMetadata {
    FrameMetadata {
        clean_aperture: None,
        width: WIDTH,
        height: HEIGHT,
        row_stride_bytes: stride,
        sample_aspect_ratio: SampleAspectRatio::SQUARE,
        rotation: Rotation::None,
        color: SourceColor {
            transfer,
            primaries: Primaries::Rec2020,
        },
        pts: SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 90_000).unwrap(),
        },
    }
}

fn half(bits: u16) -> f64 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f64::from(bits & 1023);
    match exponent {
        0 => sign * 2.0_f64.powi(-14) * fraction / 1024.0,
        31 => f64::NAN,
        _ => sign * 2.0_f64.powi(exponent - 15) * (1.0 + fraction / 1024.0),
    }
}

fn working(renderer: &mut PictureRenderer, target: &RenderTarget) -> WorkingRgba16Frame {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut pending = loop {
        match renderer.begin_working_readback(target, &cancelled, deadline) {
            Ok(pending) => break pending,
            Err(RenderError::ReadbackBusy) => std::thread::yield_now(),
            Err(error) => panic!("{error}"),
        }
    };
    loop {
        if let Some(frame) = pending.poll(&cancelled).unwrap() {
            return frame;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

fn working_pixel(frame: &WorkingRgba16Frame, x: u32, y: u32) -> [f64; 4] {
    let offset = (y * frame.row_stride_bytes() + x * 8) as usize;
    std::array::from_fn(|channel| {
        let start = offset + channel * 2;
        half(u16::from_le_bytes([
            frame.bytes()[start],
            frame.bytes()[start + 1],
        ]))
    })
}

fn display(device: &wgpu::Device, queue: &wgpu::Queue, target: &RenderTarget) -> Vec<u8> {
    let texture = target.display_texture();
    let row = texture.width() * 4;
    let stride = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(stride * texture.height()),
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
                bytes_per_row: Some(stride),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    buffer.map_async(wgpu::MapMode::Read, .., |result| result.unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    let view = buffer.get_mapped_range(..).unwrap();
    view.chunks(stride as usize)
        .flat_map(|chunk| chunk[..row as usize].to_vec())
        .collect()
}

struct Measured {
    working_relative: f64,
    display_codes: u8,
    luma_codes: u16,
}

fn render_and_measure(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut PictureRenderer,
    source: &Rgba8Frame,
    pipeline: ColorPipeline,
) -> Measured {
    renderer.set_color_pipeline(pipeline);
    assert_eq!(renderer.color_pipeline(), pipeline);
    let target = renderer.create_target(WIDTH, HEIGHT).unwrap();
    renderer
        .render_composed(source, &target, None, [WIDTH, HEIGHT], FitMode::Fit, &[])
        .unwrap();
    let actual = working(renderer, &target);
    let shown = display(device, queue, &target);
    let geometry = PictureGeometry::new(source.metadata(), WIDTH, HEIGHT, FitMode::Fit).unwrap();
    let transfer = match pipeline.output {
        deadpan_render::OutputColor::Hdr(transfer) => transfer,
        deadpan_render::OutputColor::Sdr => HdrTransfer::Pq,
    };
    let mut measured = Measured {
        working_relative: 0.0,
        display_codes: 0,
        luma_codes: 0,
    };
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let expected =
                reference_working_with_geometry(source, &geometry, pipeline, x, y).unwrap();
            let gpu = working_pixel(&actual, x, y);
            assert_eq!(gpu[3], 1.0);
            for channel in 0..3 {
                // binary16 storage (2^-11 relative) plus f32 transfer math;
                // absolute floor for near-black/subnormal values.
                let error = (gpu[channel] - expected[channel]).abs();
                let relative = error / expected[channel].abs().max(1e-3);
                measured.working_relative = measured.working_relative.max(relative);
                assert!(
                    relative < 2e-3,
                    "working ({x}, {y}) {channel}: {} vs {}",
                    gpu[channel],
                    expected[channel]
                );
            }
            let reference =
                reference_pixel_with_pipeline(source, &geometry, pipeline, x, y).unwrap();
            let offset = ((y * WIDTH + x) * 4) as usize;
            for channel in 0..3 {
                let difference = shown[offset + channel].abs_diff(reference[channel]);
                measured.display_codes = measured.display_codes.max(difference);
            }
            // P10 luma code from the GPU working pixel vs the f64 reference.
            let code = |rgb: [f64; 3]| {
                let signal = working_to_rec2100_nonlinear(rgb, transfer);
                let luma = 0.2627 * signal[0] + 0.6780 * signal[1] + 0.0593 * signal[2];
                (64.0 + 876.0 * luma).clamp(64.0, 940.0).round() as u16
            };
            let gpu_code = code([gpu[0], gpu[1], gpu[2]]);
            measured.luma_codes = measured.luma_codes.max(gpu_code.abs_diff(code(expected)));
        }
    }
    assert!(
        measured.display_codes <= 1,
        "display codes {}",
        measured.display_codes
    );
    assert!(
        measured.luma_codes <= 1,
        "luma codes {}",
        measured.luma_codes
    );
    if let deadpan_render::OutputColor::Hdr(transfer) = pipeline.output {
        let (pixels, light) = Rec2100Yuv420P10Frame::from_working(&actual, transfer).unwrap();
        assert_eq!(pixels.width(), WIDTH);
        assert!(
            light.max_nits > 203.0,
            "HDR output retains highlights: {light:?}"
        );
    }
    measured
}

#[test]
fn fractional_clean_apertures_keep_hdr_interpretation_before_filtering() {
    use deadpan_core::ExactRatio;
    use deadpan_render::CleanAperture;

    let (device, queue) = device();
    let mut renderer = PictureRenderer::new(&device, &queue);
    let q = |n, d| ExactRatio::new(n, d).unwrap();
    for transfer in [Transfer::Pq, Transfer::Hlg] {
        for rotation in [Rotation::None, Rotation::Clockwise90] {
            let original = frame16(transfer);
            let mut metadata = *original.metadata();
            metadata.clean_aperture =
                Some(CleanAperture::new([q(5, 4), q(1, 2), q(91, 2), q(27, 2)]).unwrap());
            metadata.rotation = rotation;
            metadata.sample_aspect_ratio = SampleAspectRatio::new(4, 3).unwrap();
            let frame = Rgba8Frame::new_rgba16(metadata, original.bytes().to_vec()).unwrap();
            let tone = ToneMap::new(10000.0).unwrap();
            for pipeline in [
                ColorPipeline::sdr(tone),
                ColorPipeline::hdr(HdrTransfer::Pq, tone),
            ] {
                let measured = render_and_measure(&device, &queue, &mut renderer, &frame, pipeline);
                assert!(measured.display_codes <= 1);
                assert!(measured.luma_codes <= 1);
            }
        }
    }
}

#[test]
fn hdr_frames_decode_tone_map_and_encode_against_the_f64_reference() {
    let (device, queue) = device();
    let mut renderer = PictureRenderer::new(&device, &queue);
    let tone_map = ToneMap::default();
    let pq = frame16(Transfer::Pq);
    let hlg = frame16(Transfer::Hlg);
    let cases = [
        (
            "PQ, HDR PQ output",
            &pq,
            ColorPipeline::hdr(HdrTransfer::Pq, tone_map),
        ),
        (
            "HLG, HDR HLG output",
            &hlg,
            ColorPipeline::hdr(HdrTransfer::Hlg, tone_map),
        ),
        ("PQ, SDR output", &pq, ColorPipeline::sdr(tone_map)),
        ("HLG, SDR output", &hlg, ColorPipeline::sdr(tone_map)),
        (
            "PQ, SDR output, 4000 cd/m^2 tone map",
            &pq,
            ColorPipeline::sdr(ToneMap::new(4000.0).unwrap()),
        ),
    ];
    for (name, source, pipeline) in cases {
        let measured = render_and_measure(&device, &queue, &mut renderer, source, pipeline);
        println!(
            "{name}: max working relative error {:.3e}, display code {}, P10 luma code {}",
            measured.working_relative, measured.display_codes, measured.luma_codes
        );
    }
    // SDR output tone-maps HDR sources into working <= 1.0 before compositing.
    renderer.set_color_pipeline(ColorPipeline::sdr(tone_map));
    let target = renderer.create_target(WIDTH, HEIGHT).unwrap();
    renderer
        .render_composed(&pq, &target, None, [WIDTH, HEIGHT], FitMode::Fit, &[])
        .unwrap();
    let mapped = working(&mut renderer, &target);
    let brightest = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .map(|(x, y)| {
            working_pixel(&mapped, x, y)[..3]
                .iter()
                .copied()
                .fold(0.0, f64::max)
        })
        .fold(0.0, f64::max);
    assert!(brightest <= 1.001 && brightest > 0.99, "{brightest}");
}

#[test]
fn rgba16_sdr_source_matches_the_reference_and_its_rgba8_equivalent() {
    let (device, queue) = device();
    let mut renderer = PictureRenderer::new(&device, &queue);
    let stride = WIDTH * 8;
    let mut wide = Vec::new();
    let mut narrow = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let value = ((x * 5 + y * 11) % 256) as u8;
            let rgba = [value, 255 - value, value / 2, 255];
            narrow.extend(rgba);
            for channel in rgba {
                // code * 257 is exactly code / 255 in 16-bit normalization.
                wide.extend((u16::from(channel) * 257).to_le_bytes());
            }
        }
    }
    let mut meta = metadata(Transfer::Srgb, stride);
    meta.color.primaries = Primaries::Rec709;
    let wide = Rgba8Frame::new_rgba16(meta, wide).unwrap();
    meta.row_stride_bytes = WIDTH * 4;
    let narrow = Rgba8Frame::new(meta, narrow).unwrap();
    let target = renderer.create_target(WIDTH, HEIGHT).unwrap();
    renderer
        .render_composed(&wide, &target, None, [WIDTH, HEIGHT], FitMode::Fit, &[])
        .unwrap();
    let wide_display = display(&device, &queue, &target);
    let wide_working = working(&mut renderer, &target);
    renderer
        .render_composed(&narrow, &target, None, [WIDTH, HEIGHT], FitMode::Fit, &[])
        .unwrap();
    let narrow_display = display(&device, &queue, &target);
    let narrow_working = working(&mut renderer, &target);
    let max_working = wide_working
        .bytes()
        .chunks(2)
        .zip(narrow_working.bytes().chunks(2))
        .map(|(a, b)| {
            (half(u16::from_le_bytes([a[0], a[1]])) - half(u16::from_le_bytes([b[0], b[1]]))).abs()
        })
        .filter(|difference| difference.is_finite())
        .fold(0.0, f64::max);
    assert!(max_working < 1e-3, "{max_working}");
    assert!(
        wide_display
            .iter()
            .zip(&narrow_display)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
}

/// Uniform frames: PQ RGBA64 at the 203 cd/m^2 reference-white signal, and
/// RGBA8 sRGB white standing in for SDR graphics at working 1.0.
fn reference_white_frames() -> (Rgba8Frame, Rgba8Frame) {
    let code = (pq_inverse_eotf(203.0) * 65535.0).round() as u16;
    let mut wide = Vec::new();
    for _ in 0..WIDTH * HEIGHT {
        for value in [code, code, code, 65535] {
            wide.extend(value.to_le_bytes());
        }
    }
    let pq = Rgba8Frame::new_rgba16(metadata(Transfer::Pq, WIDTH * 8), wide).unwrap();
    let mut meta = metadata(Transfer::Srgb, WIDTH * 4);
    meta.color.primaries = Primaries::Rec709;
    let white = Rgba8Frame::new(meta, vec![255; (WIDTH * HEIGHT * 4) as usize]).unwrap();
    (pq, white)
}

#[test]
fn reference_white_lands_near_sdr_white_in_export_and_preview() {
    let (device, queue) = device();
    let mut renderer = PictureRenderer::new(&device, &queue);
    let (pq, white) = reference_white_frames();
    let tone_map = ToneMap::default();
    let expected_white = tone_map.map_working(1.0);
    assert!((0.95..1.0).contains(&expected_white));
    let target = renderer.create_target(WIDTH, HEIGHT).unwrap();
    let shown = |renderer: &mut PictureRenderer, source: &Rgba8Frame, pipeline| {
        renderer.set_color_pipeline(pipeline);
        renderer
            .render_composed(source, &target, None, [WIDTH, HEIGHT], FitMode::Fit, &[])
            .unwrap();
        let geometry =
            PictureGeometry::new(source.metadata(), WIDTH, HEIGHT, FitMode::Fit).unwrap();
        let reference = reference_pixel_with_pipeline(source, &geometry, pipeline, 3, 3).unwrap();
        let display = display(&device, &queue, &target);
        let code = display[((3 * WIDTH + 3) * 4) as usize];
        assert!(code.abs_diff(reference[0]) <= 1, "{code} vs {reference:?}");
        (working_pixel(&working(renderer, &target), 3, 3)[0], code)
    };
    // SDR output: HDR reference white is tone-mapped to about 0.95 working,
    // while SDR graphics composed over it stay at 1.0.
    let sdr = ColorPipeline::sdr(tone_map);
    let (hdr_working, hdr_code) = shown(&mut renderer, &pq, sdr);
    assert!((hdr_working - expected_white).abs() < 2e-3, "{hdr_working}");
    let (graphics_working, graphics_code) = shown(&mut renderer, &white, sdr);
    assert_eq!((graphics_working, graphics_code), (1.0, 255));
    // HDR output keeps both at working 1.0 for the encoder; its SDR preview
    // shows them as the same near-white code the SDR export uses for HDR white.
    let hdr = ColorPipeline::hdr(HdrTransfer::Pq, tone_map);
    let (preview_hdr_working, preview_hdr_code) = shown(&mut renderer, &pq, hdr);
    let (preview_graphics_working, preview_graphics_code) = shown(&mut renderer, &white, hdr);
    assert!((preview_hdr_working - 1.0).abs() < 2e-3);
    assert_eq!(preview_graphics_working, 1.0);
    assert!(preview_hdr_code.abs_diff(hdr_code) <= 1);
    assert_eq!(preview_graphics_code, preview_hdr_code);
    println!(
        "reference white: working {expected_white:.6}, SDR export HDR white code {hdr_code}, \
         SDR export graphics code {graphics_code}, HDR preview code {preview_graphics_code}"
    );
}
