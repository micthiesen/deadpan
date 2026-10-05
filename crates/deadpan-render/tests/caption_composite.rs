//! Captions composite in the shared Metal picture pass: white fill over a
//! dark outline in linear working light, before encoder readback, over a
//! source picture and over the authored black Background alike.
#![cfg(target_os = "macos")]

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{CaptionPlacement, SourceTimeBase, SourceTimestamp};
use deadpan_render::{
    CaptionLine, CaptionOverlay, FitMode, FrameMetadata, PictureRenderer, Primaries,
    Rec709Yuv420Frame, RenderError, Rgba8Frame, Rotation, SampleAspectRatio, SourceColor, Transfer,
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

fn renderer() -> PictureRenderer {
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
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Deadpan caption composite test"),
        ..Default::default()
    }))
    .expect("a Metal device");
    PictureRenderer::new(&device, &queue)
}

/// Uniform mid-gray, linear Rec.709.
fn gray() -> Rgba8Frame {
    Rgba8Frame::new(
        FrameMetadata {
            width: WIDTH,
            height: HEIGHT,
            row_stride_bytes: WIDTH * 4,
            color: SourceColor {
                transfer: Transfer::Linear,
                primaries: Primaries::Rec709,
            },
            rotation: Rotation::None,
            sample_aspect_ratio: SampleAspectRatio::SQUARE,
            pts: SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 90_000).unwrap(),
            },
        },
        [64, 64, 64, 255].repeat((WIDTH * HEIGHT) as usize),
    )
    .unwrap()
}

fn luma(renderer: &mut PictureRenderer, target: &deadpan_render::RenderTarget) -> Vec<u8> {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut pending = loop {
        match renderer.begin_working_readback(target, &cancelled, deadline) {
            Ok(pending) => break pending,
            Err(RenderError::ReadbackBusy) => std::thread::yield_now(),
            Err(error) => panic!("{error}"),
        }
    };
    let working = loop {
        if let Some(frame) = pending.poll(&cancelled).unwrap() {
            break frame;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    Rec709Yuv420Frame::from_working(&working)
        .unwrap()
        .y_plane()
        .to_vec()
}

#[test]
fn captions_composite_white_fill_and_dark_outline_before_encoder_readback() {
    let mut renderer = renderer();
    let target = renderer.create_target(WIDTH, HEIGHT).unwrap();
    let overlay = CaptionOverlay::rasterize(
        &[CaptionLine {
            text: "HI".into(),
            placement: CaptionPlacement::Center,
        }],
        [WIDTH, HEIGHT],
        [WIDTH, HEIGHT],
    )
    .unwrap()
    .unwrap();
    let pixels = |predicate: &dyn Fn((u8, u8)) -> bool| -> Vec<usize> {
        (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|(x, y)| predicate(overlay.coverage(*x, *y)))
            .map(|(x, y)| (y * WIDTH + x) as usize)
            .collect()
    };
    let (fill, outline_only, clear) = (
        pixels(&|(fill, _)| fill == 255),
        pixels(&|(fill, outline)| fill == 0 && outline == 255),
        pixels(&|(fill, outline)| fill == 0 && outline == 0),
    );
    assert!(!fill.is_empty() && !outline_only.is_empty() && !clear.is_empty());

    renderer
        .render_composed_captioned(
            &gray(),
            &target,
            None,
            [WIDTH, HEIGHT],
            FitMode::Fit,
            &[],
            Some(&overlay),
        )
        .unwrap();
    let captioned = luma(&mut renderer, &target);
    renderer
        .render_composed(&gray(), &target, None, [WIDTH, HEIGHT], FitMode::Fit, &[])
        .unwrap();
    let plain = luma(&mut renderer, &target);
    // Limited-range Rec.709: white is code 235 and black 16.
    assert!(
        fill.iter().all(|index| captioned[*index] >= 233),
        "white fill"
    );
    assert!(
        outline_only.iter().all(|index| captioned[*index] <= 18),
        "dark outline"
    );
    assert!(
        clear.iter().all(|index| captioned[*index] == plain[*index]),
        "the picture is untouched outside the caption"
    );
    assert!(
        plain.iter().all(|value| (100..200).contains(value)),
        "mid-gray source"
    );

    // The authored black picture takes the same caption.
    renderer
        .render_background_captioned(&target, Some(&overlay))
        .unwrap();
    let black = luma(&mut renderer, &target);
    assert!(fill.iter().all(|index| black[*index] >= 233));
    assert!(clear.iter().all(|index| black[*index] == 16));
    // A mismatched overlay raster is refused before any GPU work.
    let small = CaptionOverlay::rasterize(
        &[CaptionLine {
            text: "HI".into(),
            placement: CaptionPlacement::Center,
        }],
        [WIDTH, HEIGHT],
        [WIDTH / 2, HEIGHT / 2],
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        renderer.render_background_captioned(&target, Some(&small)),
        Err(RenderError::CaptionRaster)
    ));
}
