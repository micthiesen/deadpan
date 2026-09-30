//! Bounded developer fixture for the production encoder boundary. The Python
//! qualification runner independently decodes every output; this is not Render.
use std::{
    error::Error,
    fs::OpenOptions,
    os::unix::fs::OpenOptionsExt,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deadpan_encode::{
    BFramePolicy, EncodeContract, EncodeLimits, EncoderMode, EncoderSession, NextInput,
};
use serde_json::json;

const WIDTH: usize = 320;
const HEIGHT: usize = 180;
const COLORS: [[u8; 3]; 5] = [
    [16, 128, 128],
    [235, 128, 128],
    [63, 102, 240],
    [173, 42, 26],
    [32, 240, 118],
];
const NEUTRAL_LEVELS: [&str; 5] = ["0.001", "0.01", "0.018", "0.18", "0.5"];
const DIGITS: [[u8; 7]; 10] = [
    [14, 17, 19, 21, 25, 17, 14],
    [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2],
    [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14],
    [31, 1, 2, 4, 8, 8, 8],
    [14, 17, 17, 14, 17, 17, 14],
    [14, 17, 17, 15, 1, 1, 14],
];

fn boundary(frame: u64, numerator: u32, denominator: u32) -> u64 {
    let scaled = u128::from(frame) * 48_000 * u128::from(denominator);
    let divisor = u128::from(numerator);
    let quotient = scaled / divisor;
    let remainder = scaled % divisor;
    u64::try_from(
        quotient
            + u128::from(
                remainder * 2 > divisor || remainder * 2 == divisor && !quotient.is_multiple_of(2),
            ),
    )
    .expect("bounded fixture clock")
}

fn rectangle(bytes: &mut [u8], origin: [usize; 2], size: [usize; 2], color: [u8; 3]) {
    for (plane, code) in color.into_iter().enumerate() {
        let divisor = if plane == 0 { 1 } else { 2 };
        let stride = WIDTH / divisor;
        let offset = match plane {
            0 => 0,
            1 => WIDTH * HEIGHT,
            _ => WIDTH * HEIGHT * 5 / 4,
        };
        for row in origin[1] / divisor..(origin[1] + size[1]) / divisor {
            let start = offset + row * stride + origin[0] / divisor;
            bytes[start..start + size[0] / divisor].fill(code);
        }
    }
}

fn neutral_colors() -> [[u8; 3]; 5] {
    NEUTRAL_LEVELS.map(|text| {
        let linear: f64 = text.parse().expect("literal linear level");
        let encoded = if linear < 0.018 {
            4.5 * linear
        } else {
            1.099 * linear.powf(0.45) - 0.099
        };
        [(16.0 + 219.0 * encoded).round() as u8, 128, 128]
    })
}

fn picture(number: usize) -> Vec<u8> {
    let mut bytes = vec![0; WIDTH * HEIGHT * 3 / 2];
    rectangle(
        &mut bytes,
        [0, 0],
        [WIDTH, HEIGHT],
        [if number < 60 { 40 } else { 90 }, 128, 128],
    );
    for (patch, color) in COLORS.into_iter().enumerate() {
        rectangle(&mut bytes, [patch * 64, 120], [64, 60], color);
    }
    rectangle(
        &mut bytes,
        [(number * 2) % (WIDTH - 24), 88],
        [24, 24],
        [210, 70, 200],
    );
    for (place, divisor) in [100, 10, 1].into_iter().enumerate() {
        let digit = (number / divisor) % 10;
        for (row, bits) in DIGITS[digit].into_iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    rectangle(
                        &mut bytes,
                        [96 + place * 42 + column * 6, 16 + row * 6],
                        [6, 6],
                        [235, 128, 128],
                    );
                }
            }
        }
    }
    for (patch, color) in neutral_colors().into_iter().enumerate() {
        rectangle(&mut bytes, [patch * 64, 64], [64, 16], color);
    }
    bytes
}

fn sample(position: u64, channel: u32, total: u64, edges: bool) -> f32 {
    let mut value = 0.0;
    if edges && (position < 2048 || position >= total.saturating_sub(2048)) {
        let bits = u32::try_from(position)
            .expect("bounded fixture sample")
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223)
            .wrapping_add(channel.wrapping_mul(2_246_822_519));
        value = (i32::try_from((bits >> 24) & 63).expect("six bits") - 32) as f32 / 1024.0;
        if position == 0 {
            value = if channel == 0 { 0.3125 } else { -0.28125 };
        }
        if position == total - 1 {
            value = if channel == 0 { 0.28125 } else { -0.25 };
        }
    }
    if [100, 48_000.min(total / 2), total - 200].contains(&position) {
        value = if channel == 0 { 0.75 } else { -0.65 };
    }
    value
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !(8..=9).contains(&args.len()) {
        return Err("usage: qualify OUTPUT hardware|software two|none FPS_NUM FPS_DEN FRAMES PROJECT_START impulses|edges [FAULT]".into());
    }
    let mode = match args[1].as_str() {
        "hardware" => EncoderMode::Hardware,
        "software" => EncoderMode::Software,
        _ => return Err("unknown mode".into()),
    };
    let b_frames = match args[2].as_str() {
        "two" => BFramePolicy::TargetTwo,
        "none" => BFramePolicy::None,
        _ => return Err("unknown B policy".into()),
    };
    let numerator: u32 = args[3].parse()?;
    let denominator: u32 = args[4].parse()?;
    let frames: u64 = args[5].parse()?;
    let project_start: u64 = args[6].parse()?;
    let edges = match args[7].as_str() {
        "impulses" => false,
        "edges" => true,
        _ => return Err("unknown PCM fixture".into()),
    };
    if !(1..=240).contains(&frames)
        || project_start > 240
        || numerator == 0
        || denominator == 0
        || u64::from(numerator) < u64::from(denominator)
        || u64::from(numerator) > 60 * u64::from(denominator)
    {
        return Err("fixture exceeds bounded 1..60 fps / 240 frames".into());
    }
    let samples = boundary(project_start + frames, numerator, denominator)
        - boundary(project_start, numerator, denominator);
    if !(800..=120 * 48_000).contains(&samples) {
        return Err("fixture duration outside bounded sample range".into());
    }
    let contract = EncodeContract::new(
        [320, 180],
        [numerator, denominator],
        frames,
        samples,
        mode,
        b_frames,
    )?;
    let fault = args.get(8).map(String::as_str).unwrap_or("none");
    if ![
        "none",
        "byte-limit",
        "cancel",
        "ordinal",
        "audio-hole",
        "nonfinite",
        "incomplete",
    ]
    .contains(&fault)
    {
        return Err("unknown fault".into());
    }
    let limits = EncodeLimits {
        maximum_output_bytes: if fault == "byte-limit" {
            128
        } else {
            64 * 1024 * 1024
        },
        ..EncodeLimits::default()
    };
    let file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(&args[0])?;
    let cancelled = AtomicBool::new(false);
    let mut session = EncoderSession::open(
        file,
        contract.clone(),
        limits,
        &cancelled,
        Instant::now() + Duration::from_secs(120),
    )?;
    let mut inputs = 0;
    loop {
        if fault == "cancel" && inputs == 3 {
            cancelled.store(true, Ordering::Relaxed);
        }
        if fault == "incomplete" && inputs == 3 {
            break;
        }
        match session.next_input()? {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                let bytes = picture(usize::try_from(ordinal)?);
                session.push_picture(
                    if fault == "ordinal" {
                        ordinal + 1
                    } else {
                        ordinal
                    },
                    pts,
                    duration,
                    &bytes,
                )?;
            }
            NextInput::Audio {
                first_sample,
                samples: count,
            } => {
                let mut left: Vec<f32> = (0..count)
                    .map(|i| sample(first_sample + u64::from(i), 0, samples, edges))
                    .collect();
                let right: Vec<f32> = (0..count)
                    .map(|i| sample(first_sample + u64::from(i), 1, samples, edges))
                    .collect();
                if fault == "nonfinite" {
                    left[0] = f32::NAN;
                }
                session.push_audio(
                    if fault == "audio-hole" {
                        first_sample + 1
                    } else {
                        first_sample
                    },
                    &left,
                    &right,
                )?;
            }
            NextInput::Finish => break,
        }
        inputs += 1;
    }
    let output = session.finish()?;
    let (_, report) = output.into_parts();
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema_version": 1, "scope": "production native adapter synthetic fixture, no product publication",
            "contract": contract, "report": report, "project_start_frame": project_start,
            "source": {
                "frame_count": frames, "start_pts": 0, "duration_ticks": frames * u64::from(denominator),
                "audio_samples": samples, "audio_offset_samples": 0,
                "movie_timescale": contract.policy().movie_timescale, "width": 320, "height": 180,
                "requested_b_frames": contract.policy().b_frames, "time_base": [1, numerator],
                "frame_rate": [numerator, denominator], "pcm_kind": args[7],
                "impulses": [100, 48_000.min(samples / 2), samples - 200],
                "neutral_linear_levels": NEUTRAL_LEVELS, "neutral_yuv_patches": neutral_colors(), "color_yuv_patches": COLORS
            }
        }))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        println!(
            "{}",
            json!({
                "status": "failed",
                "kind": error.downcast_ref::<deadpan_encode::EncodeError>().map(|error| error.kind()),
                "diagnostic": error.to_string(),
            })
        );
        eprintln!("{error}");
        std::process::exit(1);
    }
}
