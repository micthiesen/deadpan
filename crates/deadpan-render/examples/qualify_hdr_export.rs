//! Real offscreen Metal RGBA64 PQ/HLG source to Rec.2100 10-bit encoder-pixel
//! qualification under the HDR output branch.
//! Usage: qualify_hdr_export REPORT.json NEW_FIXTURE_DIRECTORY
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_core::{SourceTimeBase, SourceTimestamp};
use deadpan_render::{
    ColorPipeline, FitMode, FrameLight, FrameMetadata, HdrTransfer, PictureRenderer, Primaries,
    Rec2100Yuv420P10Frame, RenderError, RenderTarget, Rgba8Frame, Rotation, SampleAspectRatio,
    SourceColor, ToneMap, Transfer, WorkingReadback, WorkingRgba16Frame, Yuv420P10Policy,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HEIGHT: u32 = 180;
const CODE_TOLERANCE: u16 = 1;
const NEUTRAL_NITS: [f64; 7] = [0.0, 0.005, 100.0, 203.0, 1000.0, 4000.0, 10_000.0];

// Independent published constants (BT.2100 Table 4 / ST 2084), not the
// production functions.
const M1: f64 = 0.1593017578125;
const M2: f64 = 78.84375;
const C1: f64 = 0.8359375;
const C2: f64 = 18.8515625;
const C3: f64 = 18.6875;
const HLG_A: f64 = 0.17883277;
const HLG_B: f64 = 0.28466892;
// Published rounded c, deliberately not the production f64 derivation
// 0.5 - a ln(4a), which is 4.7e-10 lower; the shader uses this same value.
const HLG_C: f64 = 0.55991073;
const LUMA: [f64; 3] = [0.2627, 0.6780, 0.0593];

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 2 {
        return Err(
            "usage: qualify_hdr_export REPORT.json (new file) NEW_FIXTURE_DIRECTORY".into(),
        );
    }
    let report_path = PathBuf::from(&arguments[0]);
    let fixture_path = PathBuf::from(&arguments[1]);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&report_path)?;
    let mut report = json!({
        "schema_version": 1, "status": "running", "cases": [], "checks": [],
        "platform": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "crate_version": env!("CARGO_PKG_VERSION"),
        "scope": "offscreen Metal RGBA64 PQ/HLG source, HDR output branch, linear working readback to tight Rec2100 limited-range yuv420p10le",
        "limitations": ["synthetic RGBA64 input already in RGB (no YUV source decode)",
            "identity geometry; no framing/caption/scaling HDR claim beyond the shared pass",
            "no encoded-file or decoder verification", "no display, playback or listening claim",
            "HLG assumes the nominal 1000 cd/m^2 display, gamma 1.2",
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
        "Metal HDR export-pixel qualification passed; report: {}",
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
        label: Some("Deadpan HDR export qualification"),
        ..Default::default()
    }))?;
    let mut renderer = PictureRenderer::new(&device, &queue);
    let deadline = Instant::now() + Duration::from_secs(60);
    report["input"] = json!({"format": "full-range opaque RGBA64 little-endian (Rgba16Uint upload)",
        "primaries": "BT.2020", "transfers": ["PQ", "HLG"],
        "neutral_nits": NEUTRAL_NITS,
        "signal_quantization": "round(E' * 65535)",
        "geometry": "same-size source, canonical canvas and target; identity composed framing",
        "row_padding_bytes": 16,
        "patterns": ["seven neutral luminance patches", "full-signal R/G/B and 100 cd/m^2-signal R/G/B primaries",
            "one-column red/blue alternation", "one-row red/green alternation",
            "two-axis four-color chroma pattern"]});
    report["reference"] = json!({"source": "independent f64 calculation from the known RGBA64 signal codes",
        "pq": "ST 2084 EOTF/inverse with published rational constants; working = cd/m^2 / 203",
        "hlg": "BT.2100 inverse OETF, OOTF Lw 1000 gamma 1.2 on Rec.2020 luminance; inverse OOTF, scene clip [0,1], OETF (c = 0.55991073)",
        "clip": "per-channel display light [0,10000] PQ or [0,1000] HLG after x203",
        "matrix": "BT.2020 NCL, Kr 0.2627 Kb 0.0593",
        "chroma_sampling": "left: horizontal 1/4,1/2,1/4 at even luma column with edge clamp; vertical 1/2,1/2",
        "quantization": "10-bit limited: Y 64+876Y', C 512+896C, nearest ties upward once after filtering",
        "code_tolerance": CODE_TOLERANCE,
        "tolerance_reason": "f32 GPU transfer arithmetic and the binary16 working-texture round trip"});
    reference_anchors(report)?;
    let mut cases = 0;
    for transfer in [HdrTransfer::Pq, HdrTransfer::Hlg] {
        for width in [320, 318] {
            qualify_case(&directory, &mut renderer, transfer, width, deadline, report)?;
            cases += 1;
        }
    }
    report["case_count"] = json!(cases);
    Ok(())
}

fn qualify_case(
    directory: &Path,
    renderer: &mut PictureRenderer,
    transfer: HdrTransfer,
    width: u32,
    deadline: Instant,
    report: &mut Value,
) -> Result<()> {
    renderer.set_color_pipeline(ColorPipeline::hdr(transfer, ToneMap::default()));
    let input = fixture(transfer, width)?;
    let target = renderer.create_target(width, HEIGHT)?;
    let started = Instant::now();
    renderer.render_composed(&input, &target, None, [width, HEIGHT], FitMode::Fit, &[])?;
    let cancelled = AtomicBool::new(false);
    let mut pending = begin_ready(renderer, &target, &cancelled, deadline)?;
    let working = complete(&mut pending, &cancelled, deadline)?;
    let row_bytes = width * 8;
    let stride = row_bytes.div_ceil(256) * 256;
    let (actual, light) = Rec2100Yuv420P10Frame::from_working(&working, transfer)?;
    let luma = usize::try_from(u64::from(width) * u64::from(HEIGHT))?;
    let policy = match transfer {
        HdrTransfer::Pq => Yuv420P10Policy::Rec2100PqLimitedLeft,
        HdrTransfer::Hlg => Yuv420P10Policy::Rec2100HlgLimitedLeft,
    };
    record_check(
        report,
        "tight limited-range left-sited yuv420p10le contract",
        actual.width() == width
            && actual.height() == HEIGHT
            && actual.policy() == policy
            && actual.y_stride_bytes() == width * 2
            && actual.chroma_stride_bytes() == width
            && actual.y_plane().len() == luma * 2
            && actual.cb_plane().len() == luma / 2
            && actual.cr_plane().len() == luma / 2
            && actual.bytes().len() == luma * 3
            && actual.sample_count() == luma * 3 / 2,
        json!({"transfer": format!("{transfer:?}"), "width": width}),
    )?;
    let mut poisoned = working.bytes().to_vec();
    for row in poisoned.chunks_exact_mut(usize::try_from(stride)?) {
        row[usize::try_from(row_bytes)?..].fill(0xff);
    }
    let padded = WorkingRgba16Frame::new(width, HEIGHT, stride, poisoned)?;
    record_check(
        report,
        "nonfinite working row padding is ignored",
        Rec2100Yuv420P10Frame::from_working(&padded, transfer)?.0 == actual,
        json!({"padding_bytes_per_row": stride - row_bytes}),
    )?;
    let (expected, expected_light) = reference_p10(transfer, width)?;
    let actual_codes: Vec<u16> = (0..actual.sample_count())
        .map(|index| actual.code(index).ok_or("missing code"))
        .collect::<std::result::Result<_, _>>()?;
    let comparison = compare_planes(&actual_codes, &expected, luma)?;
    let light_check = compare_light(light, expected_light);
    let name = format!(
        "{}-rec2100-patches-chroma-{width}x{HEIGHT}",
        match transfer {
            HdrTransfer::Pq => "pq",
            HdrTransfer::Hlg => "hlg",
        }
    );
    let raw_path = directory.join(format!("{name}.yuv420p10le"));
    let reference_path = directory.join(format!("{name}-reference.yuv420p10le"));
    let reference_bytes: Vec<u8> = expected
        .iter()
        .flat_map(|code| code.to_le_bytes())
        .collect();
    write_new(&raw_path, actual.bytes())?;
    write_new(&reference_path, &reference_bytes)?;
    let passed = comparison["passed"]
        .as_bool()
        .ok_or("missing comparison result")?;
    report["cases"].as_array_mut().ok_or("missing cases array")?.push(json!({
        "name": name, "raw_path": path_text(&raw_path)?, "reference_path": path_text(&reference_path)?,
        "actual_sha256": sha256(actual.bytes()), "reference_sha256": sha256(&reference_bytes),
        "width": width, "height": HEIGHT, "pixel_format": "yuv420p10le", "plane_order": ["Y", "Cb", "Cr"],
        "frame_count": 1, "byte_count": actual.bytes().len(),
        "working_stride_bytes": working.row_stride_bytes(), "comparison": comparison,
        "frame_light": {"actual_max_nits": light.max_nits, "actual_mean_nits": light.mean_nits,
            "reference_max_nits": expected_light.max_nits, "reference_mean_nits": expected_light.mean_nits,
            "max_relative_difference": light_check},
        "color": {"primaries": "bt2020", "transfer": match transfer {
            HdrTransfer::Pq => "smpte2084", HdrTransfer::Hlg => "arib-std-b67" },
            "matrix": "bt2020nc", "range": "tv", "chroma_location": "left"},
        "elapsed_seconds": started.elapsed().as_secs_f64()
    }));
    record_check(
        report,
        "all P10 codes match the independent reference within tolerance",
        passed,
        json!({"transfer": format!("{transfer:?}"), "width": width}),
    )?;
    record_check(
        report,
        "frame light statistics match the reference within 0.2%",
        light_check < 2e-3,
        json!({"transfer": format!("{transfer:?}"), "width": width, "relative": light_check}),
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

fn pq_inverse(nits: f64) -> f64 {
    let y = (nits.clamp(0.0, 10_000.0) / 10_000.0).powf(M1);
    ((C1 + C2 * y) / (1.0 + C3 * y)).powf(M2)
}

fn pq_eotf(signal: f64) -> f64 {
    let e = signal.clamp(0.0, 1.0).powf(1.0 / M2);
    ((e - C1).max(0.0) / (C2 - C3 * e)).powf(1.0 / M1) * 10_000.0
}

fn hlg_oetf(scene: f64) -> f64 {
    let scene = scene.clamp(0.0, 1.0);
    if scene <= 1.0 / 12.0 {
        (3.0 * scene).sqrt()
    } else {
        HLG_A * (12.0 * scene - HLG_B).ln() + HLG_C
    }
}

fn hlg_inverse_oetf(signal: f64) -> f64 {
    if signal <= 0.5 {
        signal * signal / 3.0
    } else {
        (((signal - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    }
}

/// Neutral signal for a display luminance under the transfer.
fn neutral_signal(transfer: HdrTransfer, nits: f64) -> f64 {
    match transfer {
        HdrTransfer::Pq => pq_inverse(nits),
        HdrTransfer::Hlg => hlg_oetf((nits / 1000.0).min(1.0).powf(1.0 / 1.2)),
    }
}

fn code16(signal: f64) -> u16 {
    // Guarded [0, 1] signal, so the rounded product is within u16.
    (signal.clamp(0.0, 1.0) * 65535.0).round() as u16
}

fn fixture_codes(transfer: HdrTransfer, width: u32, x: u32, y: u32) -> [u16; 3] {
    let full = 65535;
    let mid = code16(neutral_signal(transfer, 100.0));
    let three_quarter = code16(0.75);
    let red = [three_quarter, 0, 0];
    let green = [0, three_quarter, 0];
    let blue = [0, 0, three_quarter];
    match y {
        0..=89 => {
            let index = (u64::from(x) * NEUTRAL_NITS.len() as u64 / u64::from(width)) as usize;
            [code16(neutral_signal(transfer, NEUTRAL_NITS[index])); 3]
        }
        90..=119 => match x * 6 / width {
            0 => [full, 0, 0],
            1 => [0, full, 0],
            2 => [0, 0, full],
            3 => [mid, 0, 0],
            4 => [0, mid, 0],
            _ => [0, 0, mid],
        },
        120..=139 => {
            if x.is_multiple_of(2) {
                red
            } else {
                blue
            }
        }
        140..=159 => {
            if y.is_multiple_of(2) {
                red
            } else {
                green
            }
        }
        _ => match (x % 2, y % 2) {
            (0, 0) => red,
            (1, 0) => green,
            (0, 1) => blue,
            _ => [three_quarter; 3],
        },
    }
}

fn fixture(transfer: HdrTransfer, width: u32) -> Result<Rgba8Frame> {
    let stride = width * 8 + 16;
    let mut bytes = vec![0xa7; usize::try_from(u64::from(stride) * u64::from(HEIGHT))?];
    for y in 0..HEIGHT {
        for x in 0..width {
            let index = usize::try_from(u64::from(y) * u64::from(stride) + u64::from(x) * 8)?;
            let [r, g, b] = fixture_codes(transfer, width, x, y);
            for (channel, value) in [r, g, b, 65535].into_iter().enumerate() {
                bytes[index + channel * 2..index + channel * 2 + 2]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    Ok(Rgba8Frame::new_rgba16(
        FrameMetadata {
            clean_aperture: None,
            width,
            height: HEIGHT,
            row_stride_bytes: stride,
            color: SourceColor {
                transfer: match transfer {
                    HdrTransfer::Pq => Transfer::Pq,
                    HdrTransfer::Hlg => Transfer::Hlg,
                },
                primaries: Primaries::Rec2020,
            },
            rotation: Rotation::None,
            sample_aspect_ratio: SampleAspectRatio::SQUARE,
            pts: SourceTimestamp {
                ticks: -9001,
                time_base: SourceTimeBase::new(1, 90_000)?,
            },
        },
        bytes,
    )?)
}

/// Known signal codes to clipped display light (cd/m^2) and its nonlinear
/// output signal, entirely in f64.
fn reference_pixel(transfer: HdrTransfer, codes: [u16; 3]) -> ([f64; 3], [f64; 3]) {
    let signal = codes.map(|code| f64::from(code) / 65535.0);
    match transfer {
        HdrTransfer::Pq => {
            let light = signal.map(|value| pq_eotf(value).clamp(0.0, 10_000.0));
            (light, light.map(pq_inverse))
        }
        HdrTransfer::Hlg => {
            let scene = signal.map(hlg_inverse_oetf);
            let ys = LUMA[0] * scene[0] + LUMA[1] * scene[1] + LUMA[2] * scene[2];
            let light = scene.map(|value| (1000.0 * ys.powf(0.2) * value).clamp(0.0, 1000.0));
            let yd = LUMA[0] * light[0] + LUMA[1] * light[1] + LUMA[2] * light[2];
            let out = light.map(|fd| {
                if yd <= 0.0 {
                    0.0
                } else {
                    hlg_oetf((fd / 1000.0 * (yd / 1000.0).powf(-0.2 / 1.2)).min(1.0))
                }
            });
            (light, out)
        }
    }
}

fn reference_ycbcr(rgb: [f64; 3]) -> [f64; 3] {
    let y = LUMA[0] * rgb[0] + LUMA[1] * rgb[1] + LUMA[2] * rgb[2];
    [
        y,
        (rgb[2] - y) / (2.0 * (1.0 - LUMA[2])),
        (rgb[0] - y) / (2.0 * (1.0 - LUMA[0])),
    ]
}

fn reference_code(value: f64, minimum: f64, maximum: f64) -> Result<u16> {
    let code = (value.clamp(minimum, maximum) + 0.5).floor();
    if !code.is_finite() || !(0.0..=1023.0).contains(&code) {
        return Err("nonfinite or out-of-range reference code".into());
    }
    Ok(code as u16)
}

fn reference_p10(transfer: HdrTransfer, width: u32) -> Result<(Vec<u16>, FrameLight)> {
    let count = usize::try_from(u64::from(width) * u64::from(HEIGHT))?;
    let mut output = vec![0; count * 3 / 2];
    let mut ycbcr = vec![[0.0; 3]; count];
    let (mut maximum, mut total) = (0.0_f64, 0.0_f64);
    for y in 0..HEIGHT {
        for x in 0..width {
            let (light, signal) = reference_pixel(transfer, fixture_codes(transfer, width, x, y));
            let brightest = light[0].max(light[1]).max(light[2]);
            maximum = maximum.max(brightest);
            total += brightest;
            let index = usize::try_from(u64::from(y) * u64::from(width) + u64::from(x))?;
            ycbcr[index] = reference_ycbcr(signal);
            output[index] = reference_code(64.0 + 876.0 * ycbcr[index][0], 64.0, 940.0)?;
        }
    }
    let width = usize::try_from(width)?;
    for cy in 0..usize::try_from(HEIGHT)? / 2 {
        for cx in 0..width / 2 {
            let x = cx * 2;
            let mut chroma = [0.0; 2];
            for y in [cy * 2, cy * 2 + 1] {
                for (column, weight) in [
                    (x.saturating_sub(1), 0.125),
                    (x, 0.25),
                    ((x + 1).min(width - 1), 0.125),
                ] {
                    let value = ycbcr[y * width + column];
                    for (destination, component) in chroma.iter_mut().zip(&value[1..]) {
                        *destination += component * weight;
                    }
                }
            }
            let offset = cy * (width / 2) + cx;
            for (channel, component) in chroma.into_iter().enumerate() {
                output[count + channel * (count / 4) + offset] =
                    reference_code(512.0 + 896.0 * component, 64.0, 960.0)?;
            }
        }
    }
    Ok((
        output,
        FrameLight {
            max_nits: maximum,
            mean_nits: total / count as f64,
        },
    ))
}

fn reference_anchors(report: &mut Value) -> Result<()> {
    for (signal, nits, tolerance) in [
        (0.5081, 100.0, 0.05),
        (0.58069, 203.0, 0.02),
        (0.7518, 1000.0, 0.5),
        (1.0, 10_000.0, 1e-9),
    ] {
        let actual = pq_eotf(signal);
        record_check(
            report,
            "independent published PQ anchor",
            (actual - nits).abs() < tolerance,
            json!({"signal": signal, "expected_nits": nits, "actual_nits": actual}),
        )?;
    }
    let hlg = 1000.0 * hlg_inverse_oetf(0.75).powf(1.2);
    record_check(
        report,
        "independent BT.2408 HLG 75% anchor (~203 cd/m^2)",
        (hlg - 203.0).abs() < 0.5,
        json!({"signal": 0.75, "actual_nits": hlg}),
    )?;
    let renderer = deadpan_render::pq_eotf(0.58069);
    record_check(
        report,
        "production PQ EOTF agrees with the independent reference",
        (renderer - pq_eotf(0.58069)).abs() < 1e-9,
        json!({"production_nits": renderer}),
    )?;
    tone_map_anchors(report)
}

/// Independent closed form of the reference-white-preserving tone map:
/// identity to the knee k = 0.9, then k + (1-k) y (Y^2 + y) / (Y^2 (1 + y)).
fn tone_map_reference(working: f64, peak_nits: f64) -> f64 {
    const KNEE: f64 = 0.9;
    if working <= KNEE {
        return working;
    }
    let limit = (peak_nits / 203.0 - KNEE) / (1.0 - KNEE);
    let y = ((working - KNEE) / (1.0 - KNEE)).min(limit);
    KNEE + (1.0 - KNEE) * y * (limit * limit + y) / (limit * limit * (1.0 + y))
}

fn tone_map_anchors(report: &mut Value) -> Result<()> {
    for peak in [203.0, 400.0, 1000.0, 4000.0, 10_000.0] {
        let tone_map = ToneMap::new(peak)?;
        let white = tone_map.map_working(1.0);
        let reference = tone_map_reference(1.0, peak);
        let samples = [0.5, 0.9, 0.95, 1.0, 1.5, 2.0, 4.0, peak / 203.0, 60.0];
        let worst = samples
            .iter()
            .map(|&x| (tone_map.map_working(x) - tone_map_reference(x, peak)).abs())
            .fold(0.0, f64::max);
        record_check(
            report,
            "tone map keeps reference white at or near SDR white and agrees with the independent form",
            worst < 1e-12
                && if peak == 203.0 {
                    white == 1.0
                } else {
                    (0.95..1.0).contains(&white)
                }
                && tone_map.map_working(0.9) == 0.9
                && tone_map.map_working(peak / 203.0) > 1.0 - 1e-12,
            json!({"source_peak_nits": peak, "reference_white_working": white,
                "independent_reference_white_working": reference,
                "max_abs_difference": worst}),
        )?;
    }
    Ok(())
}

fn compare_planes(actual: &[u16], expected: &[u16], luma: usize) -> Result<Value> {
    if actual.len() != expected.len() || actual.len() != luma * 3 / 2 {
        return Err("P10 reference length mismatch".into());
    }
    let mut maximum = [0_u16; 3];
    let mut histogram = [[0_u64; 3]; 3];
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
        histogram[plane][usize::from(difference.min(2))] += 1;
        if difference > CODE_TOLERANCE {
            failures += 1;
            if examples.len() < 12 {
                examples.push(json!({"sample": index, "plane": (["Y", "Cb", "Cr"][plane]),
                    "actual": value, "reference": reference, "difference": difference}));
            }
        }
    }
    let planes = ["Y", "Cb", "Cr"];
    let counts: Vec<Value> = histogram
        .iter()
        .zip(planes)
        .map(|(counts, plane)| {
            json!({"plane": plane, "exact": counts[0], "one_code": counts[1], "over_one_code": counts[2]})
        })
        .collect();
    Ok(
        json!({"passed": failures == 0, "tolerance_codes": CODE_TOLERANCE,
        "maximum_plane_difference": maximum, "compared_codes": actual.len(),
        "difference_counts": counts,
        "out_of_tolerance_codes": failures, "first_failures": examples}),
    )
}

fn compare_light(actual: FrameLight, expected: FrameLight) -> f64 {
    let relative = |a: f64, b: f64| (a - b).abs() / b.abs().max(1e-6);
    relative(actual.max_nits, expected.max_nits).max(relative(actual.mean_nits, expected.mean_nits))
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

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
