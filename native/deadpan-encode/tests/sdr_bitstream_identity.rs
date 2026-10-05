//! The HDR additions must not change SDR v1 encoding. These hashes were
//! captured from the pre-HDR source (`af4677a3`) with this same test file in a
//! separate Cargo target; the encoder's output must stay byte-identical.

use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_encode::probe::EncoderProbe;
use deadpan_encode::{BFramePolicy, EncodeLimits, EncoderMode, EncoderSession, NextInput};

fn encode(mode: EncoderMode) -> String {
    let probe = EncoderProbe::new([64, 64], [30, 1]).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sdr.mp4");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut session = EncoderSession::open(
        file,
        probe.contract(mode, BFramePolicy::None).unwrap(),
        EncodeLimits::default(),
        &cancelled,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let mut picture = vec![0; usize::try_from(probe.config().picture_bytes).unwrap()];
    let (mut left, mut right) = ([0.0; 1024], [0.0; 1024]);
    loop {
        match session.next_input().unwrap() {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                probe.fill_picture(ordinal, &mut picture).unwrap();
                session
                    .push_picture(ordinal, pts, duration, &picture)
                    .unwrap();
            }
            NextInput::Audio {
                first_sample,
                samples,
            } => {
                let count = usize::try_from(samples).unwrap();
                probe
                    .fill_audio(first_sample, &mut left[..count], &mut right[..count])
                    .unwrap();
                session
                    .push_audio(first_sample, &left[..count], &right[..count])
                    .unwrap();
            }
            NextInput::Finish => break,
        }
    }
    let (mut file, _) = session.finish().unwrap().into_parts();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    // FNV-1a 64 plus length: a regression identity, not a security hash.
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{}:{hash:016x}", bytes.len())
}

/// Pre-HDR (`af4677a3`) OS software H.264 output of this exact fixture on the
/// measured M5 Max (macOS 26.5.2). Hardware H.264 differs by one SEI byte
/// between identical runs (measured at file offset 3520), so it cannot be
/// pinned; software is repeatable. Another OS build may legitimately change
/// the encoder's bytes; the repeatability assertion still runs everywhere.
const PRE_HDR_SOFTWARE: &str = "12919:185b6115290d95f6";

#[test]
fn sdr_v1_software_output_is_byte_identical_to_the_pre_hdr_encoder() {
    let first = encode(EncoderMode::Software);
    assert_eq!(
        first,
        encode(EncoderMode::Software),
        "software SDR output is not repeatable"
    );
    if std::env::var_os("DEADPAN_SKIP_PINNED_SDR_IDENTITY").is_none() {
        assert_eq!(first, PRE_HDR_SOFTWARE);
    }
}
