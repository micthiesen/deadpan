//! Development harness for trusted generated f32le stereo / 48 kHz fixtures.
//! No decode, downmix, gain change or claim that the input is a final master.

use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use deadpan_audio::{
    LOUDNESS_ID, LoudnessMeter, LoudnessReport, MAX_LOUDNESS_FRAMES, TruePeakMeter, TruePeakReport,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    input_format: &'static str,
    input_sha256: String,
    loudness_algorithm: &'static str,
    loudness: LoudnessReport,
    peaks: TruePeakReport,
    processing_seconds: f64,
    elapsed_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: measure_pcm INPUT.f32le NEW_REPORT.json (stereo, 48000 Hz)".into());
    };
    let metadata = std::fs::metadata(input)?;
    if !metadata.is_file() || !metadata.len().is_multiple_of(8) {
        return Err("input must be a regular file containing complete f32le stereo frames".into());
    }
    let frames = metadata.len() / 8;
    if frames > MAX_LOUDNESS_FRAMES {
        return Err("input exceeds the 24-hour analysis budget".into());
    }
    let started = Instant::now();
    let mut processing = std::time::Duration::ZERO;
    let mut file = BufReader::new(File::open(input)?);
    let mut loudness = LoudnessMeter::new(frames.max(1))?;
    let mut peaks = TruePeakMeter::new(frames.max(1))?;
    let cancelled = AtomicBool::new(false);
    let mut hash = Sha256::new();
    let mut bytes = [0_u8; 256 * 8];
    let mut pcm = [[0.0; 2]; 256];
    let mut remaining = frames;
    while remaining > 0 {
        let count = usize::try_from(remaining.min(256))?;
        let bytes = &mut bytes[..count * 8];
        file.read_exact(bytes)?;
        hash.update(&*bytes);
        for (frame, raw) in pcm.iter_mut().zip(bytes.chunks_exact(8)) {
            frame[0] = f32::from_le_bytes(raw[..4].try_into()?);
            frame[1] = f32::from_le_bytes(raw[4..].try_into()?);
        }
        let tick = Instant::now();
        loudness.push(&pcm[..count], &cancelled)?;
        peaks.push(&pcm[..count], &cancelled)?;
        processing += tick.elapsed();
        remaining -= count as u64;
    }
    if file.read(&mut bytes[..1])? != 0 {
        return Err("input grew during measurement".into());
    }
    let tick = Instant::now();
    let loudness = loudness.finish();
    let peaks = peaks.finish();
    processing += tick.elapsed();
    let report = Report {
        schema_version: 1,
        input_format: "f32le_interleaved_stereo_48000_hz",
        input_sha256: hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        loudness_algorithm: LOUDNESS_ID,
        loudness,
        peaks,
        processing_seconds: processing.as_secs_f64(),
        elapsed_seconds: started.elapsed().as_secs_f64(),
    };
    let encoded = serde_json::to_vec_pretty(&report)?;
    let output = Path::new(output);
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut destination = tempfile::NamedTempFile::new_in(parent)?;
    destination.write_all(&encoded)?;
    destination.write_all(b"\n")?;
    destination.as_file().sync_all()?;
    destination.persist_noclobber(output)?;
    Ok(())
}
