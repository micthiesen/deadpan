//! Decode a changing grain/text-edge fixture after real hardware and software
//! encoding. This checks pixels rather than trusting rate-control setters.

use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_encode::{
    BFramePolicy, EncodeContract, EncodeError, EncodeLimits, EncoderMode, EncoderSession, NextInput,
};
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;
const FRAMES: u64 = 48;
static CANCELLED: AtomicBool = AtomicBool::new(false);

fn picture(ordinal: u64, bytes: &mut [u8]) {
    bytes[WIDTH * HEIGHT..].fill(128);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            // Independent four-pixel cells stress high detail without reading
            // external assets. Frame 24 is a hard cut with a different phase.
            let cell = u64::try_from((y / 4) * (WIDTH / 4) + x / 4).unwrap();
            let mut state = cell ^ ordinal.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            state = (state ^ (state >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            state = (state ^ (state >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            let grain = u8::try_from((state ^ (state >> 31)) % 160).unwrap();
            bytes[y * WIDTH + x] = if (x + 3 * y) % 97 < 2 {
                235
            } else if ordinal < 24 {
                40 + grain
            } else {
                215 - grain
            };
        }
    }
}

fn measure(mode: EncoderMode) -> Result<f64, EncodeError> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("quality.mp4");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let contract = EncodeContract::new_v2(
        [1280, 720],
        [24, 1],
        FRAMES,
        FRAMES * 2000,
        mode,
        BFramePolicy::None,
    )?;
    let mut session = EncoderSession::open(
        file,
        contract,
        EncodeLimits::default(),
        &CANCELLED,
        Instant::now() + Duration::from_secs(60),
    )?;
    let mut reference = vec![0; WIDTH * HEIGHT * 3 / 2];
    let silence = [0.0; 1024];
    loop {
        match session.next_input().unwrap() {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                picture(ordinal, &mut reference);
                session
                    .push_picture(ordinal, pts, duration, &reference)
                    .unwrap();
            }
            NextInput::Audio {
                first_sample,
                samples,
            } => {
                let n = usize::try_from(samples).unwrap();
                session
                    .push_audio(first_sample, &silence[..n], &silence[..n])
                    .unwrap();
            }
            NextInput::Finish => break,
        }
    }
    session.finish().unwrap();
    let control = DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    };
    let mut decoder =
        SourceDecoder::open(File::open(path).unwrap(), DecodeLimits::default(), control).unwrap();
    let mut minimum = f64::INFINITY;
    for ordinal in 0..FRAMES {
        let frame = decoder.next_i420(control).unwrap().unwrap();
        picture(ordinal, &mut reference);
        let sum: f64 = frame.i420[..WIDTH * HEIGHT]
            .iter()
            .zip(&reference)
            .map(|(got, expected)| (f64::from(*got) - f64::from(*expected)).powi(2))
            .sum();
        let mse = sum / f64::from(1280 * 720);
        let psnr = 10.0 * (255.0 * 255.0 / mse).log10();
        if psnr < 36.0 {
            eprintln!("{mode:?} frame {ordinal}: {psnr:.6} dB");
        }
        minimum = minimum.min(psnr);
    }
    assert!(decoder.next_i420(control).unwrap().is_none());
    eprintln!("{mode:?}: minimum luma PSNR {minimum:.6} dB");
    if minimum < 36.0 {
        eprintln!(
            "Retained failed quality fixture: {}",
            directory.keep().display()
        );
    }
    Ok(minimum)
}

#[test]
fn sdr_quality_preserves_grain_and_edges_with_concurrent_hardware_sessions() {
    std::thread::scope(|scope| {
        let first = scope.spawn(|| measure(EncoderMode::Hardware).unwrap());
        let second = scope.spawn(|| measure(EncoderMode::Hardware).unwrap());
        for minimum in [first.join().unwrap(), second.join().unwrap()] {
            assert!(
                minimum >= 36.0,
                "hardware detail degraded to {minimum:.6} dB"
            );
        }
    });
}

#[test]
fn sdr_quality_never_admits_an_unsupported_software_qp_request() {
    match measure(EncoderMode::Software) {
        Ok(minimum) => assert!(
            minimum >= 36.0,
            "software detail degraded to {minimum:.6} dB"
        ),
        Err(EncodeError::Native { code, message }) if code == "video_quality_unsupported" => {
            // Measured OS software H.264 lacks MaxAllowedFrameQP. Refuse it
            // before feeding pixels instead of accepting FFmpeg's ignored setter.
            eprintln!("Software quality policy rejected: {message}");
        }
        Err(error) => panic!("unexpected software quality failure: {error}"),
    }
}
