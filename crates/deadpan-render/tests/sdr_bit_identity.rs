//! The SDR picture path must stay bit-identical across HDR pipeline changes.
//! The pinned SHA-256 values were captured from the renderer before the HDR
//! branch existed (same Metal device class), for both working and display
//! targets of RGBA8 SDR sources under the default SDR color pipeline.
#![cfg(target_os = "macos")]

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{CaptionPlacement, SourceTimeBase, SourceTimestamp};
use deadpan_render::{
    CaptionLine, CaptionOverlay, FitMode, FrameMetadata, PictureRenderer, Primaries, RenderError,
    RenderTarget, Rgba8Frame, Rotation, SampleAspectRatio, SourceColor, Transfer,
};
use sha2::{Digest, Sha256};

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
        label: Some("Deadpan SDR bit identity test"),
        ..Default::default()
    }))
    .expect("a Metal device")
}

fn frame(width: u32, height: u32, color: SourceColor, seed: u32) -> Rgba8Frame {
    let stride = width * 4 + 8;
    let mut state = seed;
    let bytes = (0..stride * height)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect();
    Rgba8Frame::new(
        FrameMetadata {
            width,
            height,
            row_stride_bytes: stride,
            sample_aspect_ratio: SampleAspectRatio::SQUARE,
            rotation: Rotation::Clockwise90,
            color,
            pts: SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 90_000).unwrap(),
            },
        },
        bytes,
    )
    .unwrap()
}

fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_pixel = texture.format().block_copy_size(None).unwrap();
    let row = texture.width() * bytes_per_pixel;
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

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn wait_idle(renderer: &PictureRenderer) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !renderer.is_idle().unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

fn hashes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut PictureRenderer,
    target: &RenderTarget,
) -> (String, String) {
    wait_idle(renderer);
    let working = read_texture(device, queue, target.working_texture());
    let display = read_texture(device, queue, target.display_texture());
    (digest(&working), digest(&display))
}

#[test]
fn sdr_rgba8_working_and_display_bytes_are_unchanged() {
    let (device, queue) = device();
    let mut renderer = PictureRenderer::new(&device, &queue);
    let cases = [
        (
            Transfer::Srgb,
            Primaries::Rec709,
            [37, 23],
            [64, 48],
            FitMode::Fit,
        ),
        (
            Transfer::Rec709,
            Primaries::DisplayP3D65,
            [40, 30],
            [32, 32],
            FitMode::Fill,
        ),
        (
            Transfer::Linear,
            Primaries::Rec2020,
            [16, 16],
            [16, 16],
            FitMode::Fit,
        ),
    ];
    let mut actual = Vec::new();
    for (seed, (transfer, primaries, size, raster, mode)) in cases.into_iter().enumerate() {
        let source = frame(
            size[0],
            size[1],
            SourceColor {
                transfer,
                primaries,
            },
            seed as u32 + 7,
        );
        let target = renderer.create_target(raster[0], raster[1]).unwrap();
        renderer
            .render_composed(&source, &target, None, raster, mode, &[])
            .unwrap();
        actual.push(hashes(&device, &queue, &mut renderer, &target));
    }
    let captioned = renderer.create_target(64, 48).unwrap();
    let overlay = CaptionOverlay::rasterize(
        &[CaptionLine {
            text: "HI".into(),
            placement: CaptionPlacement::Center,
        }],
        [64, 48],
        [64, 48],
    )
    .unwrap()
    .unwrap();
    renderer
        .render_composed_captioned(
            &frame(
                37,
                23,
                SourceColor {
                    transfer: Transfer::Srgb,
                    primaries: Primaries::Rec709,
                },
                3,
            ),
            &captioned,
            None,
            [64, 48],
            FitMode::Fit,
            &[],
            Some(&overlay),
        )
        .unwrap();
    actual.push(hashes(&device, &queue, &mut renderer, &captioned));
    // The working readback API must still agree with the raw texture copy.
    let target = renderer.create_target(16, 16).unwrap();
    renderer.render_background(&target).unwrap();
    wait_idle(&renderer);
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut pending = loop {
        match renderer.begin_working_readback(&target, &cancelled, deadline) {
            Ok(pending) => break pending,
            Err(RenderError::ReadbackBusy) => std::thread::yield_now(),
            Err(error) => panic!("{error}"),
        }
    };
    let working = loop {
        if let Some(frame) = pending.poll(&cancelled).unwrap() {
            break frame;
        }
        std::thread::yield_now();
    };
    let stride = working.row_stride_bytes() as usize;
    assert!(working.bytes().chunks(stride).all(|row| {
        row[..128]
            .chunks(8)
            .all(|pixel| pixel == [0, 0, 0, 0, 0, 0, 0, 0x3c])
    }));
    for (index, (working, display)) in actual.iter().enumerate() {
        println!("case {index}: working {working} display {display}");
    }
    let expected: [(&str, &str); 4] = PINNED;
    for (index, ((working, display), (pinned_working, pinned_display))) in
        actual.iter().zip(expected).enumerate()
    {
        assert_eq!(
            working, pinned_working,
            "case {index} working bytes changed"
        );
        assert_eq!(
            display, pinned_display,
            "case {index} display bytes changed"
        );
    }
}

const PINNED: [(&str, &str); 4] = [
    (
        "ca674b5ca88ccecd81adba7363879db391a051d62966c04d49c9ad6589ba1696",
        "1d58265e36e86d2049bb875d79b6c77550e29ff68e29bf8f2706ee5ad3979543",
    ),
    (
        "192eb41a62e3c8c1bddea5ebb8c8a835479678029ebcb8cc8b405611c731390c",
        "0411b8152e4b510682ad1d41b45b831c895caa666cf61e1d3972fe26871ff7e9",
    ),
    (
        "39d6985a7bd8e4c24b8f8932400d247a1e8afd83adf96ff2f1e08def737c76db",
        "2b7b6531cae968e471f30982edcbabc74124d4fd882306034c854742d8a07e70",
    ),
    (
        "cbdb3377b5316c2ebcd70639986e1cb0eeb35944fe982be4ab5254c319fbbf8e",
        "8071eb9c4b7a5fbf469a8333382e9f5d9d7e7346f42d6d84452cfd4d1d30a40a",
    ),
];
