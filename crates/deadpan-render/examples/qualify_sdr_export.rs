//! Real offscreen Metal working-picture to SDR encoder-pixel qualification.
//! Usage: qualify_sdr_export REPORT.json NEW_FIXTURE_DIRECTORY
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deadpan_core::{SourceTimeBase, SourceTimestamp};
use deadpan_render::{
    FitMode, FrameMetadata, PictureRenderer, Primaries, Rec709Yuv420Frame, RenderError,
    RenderTarget, Rgba8Frame, Rotation, SampleAspectRatio, SourceColor, Transfer, WorkingReadback,
    WorkingRgba16Frame, Yuv420Policy,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HEIGHT: u32 = 180;
const CODE_TOLERANCE: u8 = 1;
const PALETTE: [[u8; 4]; 10] = [
    [0, 0, 0, 255],
    [255, 255, 255, 255],
    [255, 0, 0, 255],
    [0, 255, 0, 255],
    [0, 0, 255, 255],
    [1, 1, 1, 255],
    [4, 4, 4, 255],
    [5, 5, 5, 255],
    [46, 46, 46, 255],
    [128, 128, 128, 255],
];

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 2 {
        return Err(
            "usage: qualify_sdr_export REPORT.json (new file) NEW_FIXTURE_DIRECTORY".into(),
        );
    }
    let report_path = PathBuf::from(&arguments[0]);
    let fixture_path = PathBuf::from(&arguments[1]);
    // Reserve first so an ordinary qualification failure retains its report.
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&report_path)?;
    let mut report = json!({
        "schema_version": 1, "status": "running", "cases": [], "checks": [],
        "platform": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "crate_version": env!("CARGO_PKG_VERSION"),
        "scope": "offscreen Metal canonical working picture to tight Rec709 limited-range I420",
        "limitations": ["synthetic SDR input", "no encoded-file or decoder verification",
            "no timeline/export-product integration", "no HDR, display, playback or listening claim",
            "cooperative GPU cancellation/deadline, no preemptive driver timeout",
            "external host records source/build/library hashes and process timeout"]
    });
    let started = Instant::now();
    let result = qualify(&fixture_path, &mut report);
    report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    match &result {
        Ok(()) => report["status"] = json!("passed"),
        Err(error) => {
            report["status"] = json!("failed");
            report["error"] = json!(error.to_string());
        }
    }
    serde_json::to_writer_pretty(&mut file, &report)?;
    writeln!(file)?;
    file.sync_all()?;
    result?;
    println!(
        "Metal SDR export-pixel qualification passed; report: {}",
        report_path.display()
    );
    Ok(())
}

fn qualify(directory: &Path, report: &mut Value) -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Err("this qualification requires macOS Metal without a fallback backend".into());
    }
    fs::create_dir(directory)?;
    let directory = fs::canonicalize(directory)?;
    report["fixture_directory"] = json!(path_text(&directory)?);
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
    report["adapter"] = json!({"name": info.name, "backend": format!("{:?}", info.backend),
        "driver": info.driver, "driver_info": info.driver_info, "vendor": info.vendor,
        "device": info.device, "device_type": format!("{:?}", info.device_type)});
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Deadpan SDR export qualification"),
        ..Default::default()
    }))?;
    let mut renderer = PictureRenderer::new(&device, &queue);
    let deadline = Instant::now() + Duration::from_secs(30);
    report["input"] = json!({"format": "full-range opaque RGBA8", "transfer": "linear",
        "primaries": "Rec709 D65", "palette_rgba8": PALETTE,
        "neutral_levels": ["1/255", "4/255", "5/255", "46/255", "128/255"],
        "geometry": "same-size source, canonical canvas and target; identity composed framing",
        "patterns": ["ten broad color/neutral patches", "one-column red/blue alternation",
            "one-row red/green alternation", "two-axis four-color chroma pattern"]});
    report["reference"] = json!({"source": "independent f64 calculation from known linear Rec709 RGBA8",
        "oetf": "4.5L below0.018;1.099*L^0.45-0.099 otherwise",
        "luma_weights": [0.2126, 0.7152, 0.0722],
        "chroma_sampling": "left: horizontal1/4,1/2,1/4 at even luma column with edge clamp; vertical1/2,1/2",
        "quantization": "nearest nonnegative code, ties upward, after unquantized chroma filtering",
        "code_tolerance": CODE_TOLERANCE,
        "tolerance_reason": "one code for GPU matrix arithmetic and the binary16 working-texture round trip"});
    reference_anchors(report)?;
    for width in [320, 318] {
        qualify_case(&directory, &mut renderer, width, deadline, report)?;
    }
    report["case_count"] = json!(2);
    Ok(())
}

fn qualify_case(
    directory: &Path,
    renderer: &mut PictureRenderer,
    width: u32,
    deadline: Instant,
    report: &mut Value,
) -> Result<()> {
    let input = fixture(width)?;
    let target = renderer.create_target(width, HEIGHT)?;
    let started = Instant::now();
    renderer.render_composed(&input, &target, None, [width, HEIGHT], FitMode::Fit, &[])?;
    let cancelled = AtomicBool::new(false);
    let mut pending = begin_ready(renderer, &target, &cancelled, deadline)?;
    record_check(
        report,
        "second simultaneous readback rejected",
        matches!(
            renderer.begin_working_readback(&target, &cancelled, deadline),
            Err(RenderError::ReadbackBusy)
        ),
        json!({"width": width}),
    )?;
    let working = complete(&mut pending, &cancelled, deadline)?;
    record_check(
        report,
        "completed ticket cannot be reused",
        matches!(pending.poll(&cancelled), Err(RenderError::ReadbackFinished)),
        json!({"width": width}),
    )?;
    let row_bytes = width.checked_mul(8).ok_or("working row overflow")?;
    let expected_stride = row_bytes
        .div_ceil(256)
        .checked_mul(256)
        .ok_or("stride overflow")?;
    record_check(
        report,
        "working readback dimensions and padded rows",
        working.width() == width
            && working.height() == HEIGHT
            && working.row_stride_bytes() == expected_stride
            && u64::try_from(working.bytes().len())?
                == u64::from(expected_stride) * u64::from(HEIGHT),
        json!({"width": width, "row_bytes": row_bytes, "stride_bytes": working.row_stride_bytes()}),
    )?;
    let actual = Rec709Yuv420Frame::from_working(&working)?;
    let expected = reference_i420(width)?;
    let luma = usize::try_from(u64::from(width) * u64::from(HEIGHT))?;
    record_check(
        report,
        "tight limited-range left-sited I420 contract",
        actual.width() == width
            && actual.height() == HEIGHT
            && actual.policy() == Yuv420Policy::Rec709LimitedLeft
            && actual.y_stride_bytes() == width
            && actual.chroma_stride_bytes() == width / 2
            && actual.y_plane().len() == luma
            && actual.cb_plane().len() == luma / 4
            && actual.cr_plane().len() == luma / 4
            && actual.bytes().len() == luma * 3 / 2,
        json!({"width": width, "byte_count": actual.bytes().len()}),
    )?;
    if width == 318 {
        let mut poisoned = working.bytes().to_vec();
        for row in poisoned.chunks_exact_mut(usize::try_from(expected_stride)?) {
            // Signalling/nonfinite half patterns in padding must never be read.
            row[usize::try_from(row_bytes)?..].fill(0xff);
        }
        let padded = WorkingRgba16Frame::new(width, HEIGHT, expected_stride, poisoned)?;
        record_check(
            report,
            "nonfinite row padding is ignored",
            Rec709Yuv420Frame::from_working(&padded)? == actual,
            json!({"padding_bytes_per_row": expected_stride - row_bytes}),
        )?;
    }
    let name = format!("linear-rec709-patches-chroma-{width}x{HEIGHT}");
    let raw_path = directory.join(format!("{name}.i420"));
    let reference_path = directory.join(format!("{name}-reference.i420"));
    write_new(&raw_path, actual.bytes())?;
    write_new(&reference_path, &expected)?;
    let actual_sha256: String = Sha256::digest(actual.bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let reference_sha256: String = Sha256::digest(&expected)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let comparison = compare_planes(actual.bytes(), &expected, luma)?;
    let passed = comparison["passed"]
        .as_bool()
        .ok_or("missing comparison result")?;
    report["cases"].as_array_mut().ok_or("missing cases array")?.push(json!({
        "name": name, "raw_path": path_text(&raw_path)?, "reference_path": path_text(&reference_path)?,
        "actual_sha256": actual_sha256, "reference_sha256": reference_sha256,
        "width": width, "height": HEIGHT, "pixel_format": "yuv420p", "plane_order": ["Y", "Cb", "Cr"],
        "frame_rate": [30, 1], "frame_count": 1, "byte_count": actual.bytes().len(),
        "y_stride_bytes": actual.y_stride_bytes(), "chroma_stride_bytes": actual.chroma_stride_bytes(),
        "working_stride_bytes": working.row_stride_bytes(), "comparison": comparison,
        "color": {"primaries": "bt709", "transfer": "bt709", "matrix": "bt709", "range": "tv", "chroma_location": "left"},
        "elapsed_seconds": started.elapsed().as_secs_f64()
    }));
    record_check(
        report,
        "all I420 codes match independent pixel reference",
        passed,
        json!({"width": width}),
    )?;
    if width == 320 {
        qualify_readback_controls(renderer, &target, deadline, report)?;
    }
    Ok(())
}

fn qualify_readback_controls(
    renderer: &mut PictureRenderer,
    target: &RenderTarget,
    deadline: Instant,
    report: &mut Value,
) -> Result<()> {
    let cancelled = AtomicBool::new(true);
    record_check(
        report,
        "cancelled readback rejected before admission",
        matches!(
            renderer.begin_working_readback(target, &cancelled, deadline),
            Err(RenderError::ReadbackCancelled)
        ),
        json!({}),
    )?;
    cancelled.store(false, Ordering::Release);
    record_check(
        report,
        "expired monotonic deadline rejected before admission",
        matches!(
            renderer.begin_working_readback(target, &cancelled, Instant::now()),
            Err(RenderError::ReadbackDeadline)
        ),
        json!({}),
    )?;
    let mut pending = begin_ready(renderer, target, &cancelled, deadline)?;
    cancelled.store(true, Ordering::Release);
    record_check(
        report,
        "in-flight cancellation publishes no frame",
        matches!(
            pending.poll(&cancelled),
            Err(RenderError::ReadbackCancelled)
        ),
        json!({}),
    )?;
    cancelled.store(false, Ordering::Release);
    record_check(
        report,
        "cancelled ticket cannot be reused",
        matches!(pending.poll(&cancelled), Err(RenderError::ReadbackFinished)),
        json!({}),
    )?;
    drop(pending);
    let mut recovered = begin_ready(renderer, target, &cancelled, deadline)?;
    let recovered_frame = complete(&mut recovered, &cancelled, deadline)?;
    record_check(
        report,
        "cancelled GPU work drains before successful recovery",
        recovered_frame.width() == target.width() && recovered_frame.height() == target.height(),
        json!({}),
    )?;
    let abandoned = begin_ready(renderer, target, &cancelled, deadline)?;
    drop(abandoned);
    let mut final_read = begin_ready(renderer, target, &cancelled, deadline)?;
    let final_frame = complete(&mut final_read, &cancelled, deadline)?;
    record_check(
        report,
        "dropped ticket releases allocation after GPU callbacks drain",
        final_frame.bytes() == recovered_frame.bytes(),
        json!({}),
    )?;
    Ok(())
}

fn begin_ready(
    renderer: &mut PictureRenderer,
    target: &RenderTarget,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<WorkingReadback> {
    loop {
        match renderer.begin_working_readback(target, cancelled, deadline) {
            Ok(pending) => return Ok(pending),
            Err(RenderError::ReadbackBusy) if Instant::now() < deadline => std::thread::yield_now(),
            Err(error) => return Err(error.into()),
        }
    }
}

fn complete(
    pending: &mut WorkingReadback,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<WorkingRgba16Frame> {
    loop {
        if Instant::now() >= deadline {
            return Err("qualification readback polling deadline exceeded".into());
        }
        if let Some(frame) = pending.poll(cancelled)? {
            return Ok(frame);
        }
        std::thread::yield_now();
    }
}

fn fixture(width: u32) -> Result<Rgba8Frame> {
    let stride = width
        .checked_mul(4)
        .and_then(|value| value.checked_add(12))
        .ok_or("input stride overflow")?;
    let mut bytes = vec![0xa7; usize::try_from(u64::from(stride) * u64::from(HEIGHT))?];
    for y in 0..HEIGHT {
        for x in 0..width {
            let index = usize::try_from(u64::from(y) * u64::from(stride) + u64::from(x) * 4)?;
            bytes[index..index + 4].copy_from_slice(&fixture_pixel(width, x, y)?);
        }
    }
    Ok(Rgba8Frame::new(
        FrameMetadata {
            clean_aperture: None,
            width,
            height: HEIGHT,
            row_stride_bytes: stride,
            color: SourceColor {
                transfer: Transfer::Linear,
                primaries: Primaries::Rec709,
            },
            rotation: Rotation::None,
            sample_aspect_ratio: SampleAspectRatio::SQUARE,
            // Export frame timestamps below belong to the new 30fps fixture, not this source PTS.
            pts: SourceTimestamp {
                ticks: -9001,
                time_base: SourceTimeBase::new(1, 90_000)?,
            },
        },
        bytes,
    )?)
}

fn fixture_pixel(width: u32, x: u32, y: u32) -> Result<[u8; 4]> {
    if x >= width || y >= HEIGHT {
        return Err("fixture coordinate out of bounds".into());
    }
    let pixel = match y {
        0..=95 => {
            PALETTE
                [usize::try_from(u64::from(x) * u64::try_from(PALETTE.len())? / u64::from(width))?]
        }
        96..=123 => {
            if x.is_multiple_of(2) {
                PALETTE[2]
            } else {
                PALETTE[4]
            }
        }
        124..=151 => {
            if y.is_multiple_of(2) {
                PALETTE[2]
            } else {
                PALETTE[3]
            }
        }
        _ => match (x % 2, y % 2) {
            (0, 0) => PALETTE[2],
            (1, 0) => PALETTE[3],
            (0, 1) => PALETTE[4],
            _ => PALETTE[1],
        },
    };
    Ok(pixel)
}

// Deliberately independent of the production matrix, OETF, working-pixel decoder
// and chroma converter: source Rec709 values are known exactly as code/255.
fn reference_ycbcr(rgba: [u8; 4]) -> [f64; 3] {
    let [r, g, b] = [rgba[0], rgba[1], rgba[2]].map(|code| {
        let linear = f64::from(code) / 255.0;
        if linear < 0.018 {
            linear * 4.5
        } else {
            1.099 * linear.powf(0.45) - 0.099
        }
    });
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [
        y,
        (b - y) / (2.0 * (1.0 - 0.0722)),
        (r - y) / (2.0 * (1.0 - 0.2126)),
    ]
}

fn reference_code(value: f64, minimum: f64, maximum: f64) -> Result<u8> {
    let code = (value.clamp(minimum, maximum) + 0.5).floor();
    if !code.is_finite() || !(0.0..=255.0).contains(&code) {
        return Err("nonfinite or out-of-range reference code".into());
    }
    // Guarded finite integral 0..255 value, so this conversion is exact.
    Ok(code as u8)
}

fn reference_i420(width: u32) -> Result<Vec<u8>> {
    let count = usize::try_from(u64::from(width) * u64::from(HEIGHT))?;
    let mut output = vec![0; count * 3 / 2];
    for y in 0..HEIGHT {
        for x in 0..width {
            let ycbcr = reference_ycbcr(fixture_pixel(width, x, y)?);
            output[usize::try_from(u64::from(y) * u64::from(width) + u64::from(x))?] =
                reference_code(16.0 + 219.0 * ycbcr[0], 16.0, 235.0)?;
        }
    }
    for cy in 0..HEIGHT / 2 {
        for cx in 0..width / 2 {
            let x = cx * 2;
            let mut chroma = [0.0; 2];
            for y in [cy * 2, cy * 2 + 1] {
                for (column, weight) in [
                    (x.saturating_sub(1), 0.125),
                    (x, 0.25),
                    ((x + 1).min(width - 1), 0.125),
                ] {
                    let value = reference_ycbcr(fixture_pixel(width, column, y)?);
                    for (destination, component) in chroma.iter_mut().zip(&value[1..]) {
                        *destination += component * weight;
                    }
                }
            }
            let offset = usize::try_from(u64::from(cy) * u64::from(width / 2) + u64::from(cx))?;
            for (channel, component) in chroma.into_iter().enumerate() {
                output[count + channel * (count / 4) + offset] =
                    reference_code(128.0 + 224.0 * component, 16.0, 240.0)?;
            }
        }
    }
    Ok(output)
}

fn reference_anchors(report: &mut Value) -> Result<()> {
    for (index, expected) in [
        [16, 128, 128],
        [235, 128, 128],
        [63, 102, 240],
        [173, 42, 26],
        [32, 240, 118],
    ]
    .into_iter()
    .enumerate()
    {
        let value = reference_ycbcr(PALETTE[index]);
        let actual = [
            reference_code(16.0 + 219.0 * value[0], 16.0, 235.0)?,
            reference_code(128.0 + 224.0 * value[1], 16.0, 240.0)?,
            reference_code(128.0 + 224.0 * value[2], 16.0, 240.0)?,
        ];
        record_check(
            report,
            "independent known Rec709 color anchor",
            actual == expected,
            json!({"rgba8": PALETTE[index], "expected_ycbcr": expected, "actual_ycbcr": actual}),
        )?;
    }
    Ok(())
}

fn compare_planes(actual: &[u8], expected: &[u8], luma: usize) -> Result<Value> {
    if actual.len() != expected.len() || actual.len() != luma * 3 / 2 {
        return Err("I420 reference length mismatch".into());
    }
    let mut maximum = [0_u8; 3];
    let mut failures = 0_u64;
    let mut examples = Vec::new();
    for (index, (&value, &reference)) in actual.iter().zip(expected).enumerate() {
        let plane = if index < luma {
            0
        } else if index < luma + luma / 4 {
            1
        } else {
            2
        };
        let difference = value.abs_diff(reference);
        maximum[plane] = maximum[plane].max(difference);
        if difference > CODE_TOLERANCE {
            failures += 1;
            if examples.len() < 12 {
                examples.push(
                    json!({"byte_offset": index, "plane": (["Y", "Cb", "Cr"][plane]),
                    "actual": value, "reference": reference, "difference": difference}),
                );
            }
        }
    }
    Ok(
        json!({"passed": failures == 0, "tolerance_codes": CODE_TOLERANCE,
        "maximum_plane_difference": maximum, "compared_codes": actual.len(),
        "out_of_tolerance_codes": failures, "first_failures": examples}),
    )
}

fn record_check(report: &mut Value, label: &str, passed: bool, details: Value) -> Result<()> {
    report["checks"]
        .as_array_mut()
        .ok_or("missing checks array")?
        .push(json!({"label": label, "passed": passed, "details": details}));
    if !passed {
        return Err(format!("qualification failed: {label}").into());
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| "qualification fixture paths must be UTF-8".into())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
