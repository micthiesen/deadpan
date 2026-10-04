//! Run the real transcription worker on a 16 kHz mono PCM16 WAV.
//!
//! cargo build -p deadpan-transcribe
//! cargo run -p deadpan-cli --example qualify_transcription -- \
//!     MODEL.bin MODEL_SHA256 SPEECH.wav REPORT.json [phrase]
//!
//! The worker is found beside this example's parent target directory. The
//! report records the runtime, timing, words and an optional phrase search.
//! Set `DEADPAN_QUALIFY_CANCEL_MS` to cancel after that many milliseconds and
//! report the cancellation outcome instead.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::transcription::{AnalysisInput, TranscriptionRuntime, transcribe};
use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::{Language, ModelInput};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        return Err("usage: MODEL.bin MODEL_SHA256 SPEECH.wav REPORT.json [phrase]".into());
    }
    let model_path = std::fs::canonicalize(&args[1])?;
    let model = ModelInput {
        byte_length: std::fs::metadata(&model_path)?.len(),
        path: model_path,
        sha256: Sha256::new(args[2].clone())?,
    };
    let wav = std::fs::read(&args[3])?;
    let samples = pcm16_mono_16k(&wav)?;
    let current = std::env::current_exe()?;
    let target = current
        .parent()
        .and_then(|examples| examples.parent())
        .ok_or("example has no target directory")?;
    let runtime = TranscriptionRuntime {
        executable: PathBuf::from(target).join("deadpan-transcribe"),
        environment: Default::default(),
    };
    let input = AnalysisInput {
        samples,
        origin: 0,
        source_rate: 16_000,
    };
    let started = Instant::now();
    let mut progress = Vec::new();
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    if let Ok(delay) = std::env::var("DEADPAN_QUALIFY_CANCEL_MS") {
        let delay = Duration::from_millis(delay.parse()?);
        let trigger = std::sync::Arc::clone(&cancelled);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            trigger.store(true, std::sync::atomic::Ordering::Release);
        });
        let outcome = transcribe(
            &runtime,
            &model,
            &AnalysisInput {
                samples: samples_for_cancel(&wav)?,
                origin: 0,
                source_rate: 16_000,
            },
            Language::Code("en".into()),
            "qualify-cancel",
            &cancelled,
            Instant::now() + Duration::from_secs(600),
            |_| {},
        );
        let report = serde_json::json!({
            "cancel_after_ms": delay.as_millis() as u64,
            "outcome": match outcome {
                Ok(_) => "completed before cancellation".to_owned(),
                Err(error) => error.to_string(),
            },
            "host_elapsed_ms": started.elapsed().as_millis() as u64,
        });
        std::fs::write(&args[4], serde_json::to_vec_pretty(&report)?)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    let result = transcribe(
        &runtime,
        &model,
        &input,
        Language::Code("en".into()),
        "qualify-1",
        &cancelled,
        Instant::now() + Duration::from_secs(600),
        |percent| progress.push(percent),
    )?;
    let phrase = args.get(5).cloned().unwrap_or_default();
    let report = serde_json::json!({
        "runtime": result.runtime,
        "worker_elapsed_ms": result.elapsed.as_millis() as u64,
        "host_elapsed_ms": started.elapsed().as_millis() as u64,
        "audio_seconds": input.samples.len() as f64 / 16_000.0,
        "progress": progress,
        "words": result.transcript.words(),
        "approximate_words": result.transcript.words().iter().filter(|w| w.approximate()).count(),
        "phrase": phrase,
        "phrase_matches": result.transcript.search(&phrase),
    });
    std::fs::write(&args[4], serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Minimal canonical WAV reader: PCM16, mono, 16 kHz, with a `data` chunk.
fn pcm16_mono_16k(wav: &[u8]) -> Result<Vec<f32>, String> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err("not a RIFF WAVE file".into());
    }
    let mut offset = 12;
    let mut format = None;
    while offset + 8 <= wav.len() {
        let id = &wav[offset..offset + 4];
        let size = u32::from_le_bytes(wav[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let body = offset + 8;
        let end = body
            .checked_add(size)
            .filter(|end| *end <= wav.len())
            .ok_or("truncated chunk")?;
        if id == b"fmt " && size >= 16 {
            let tag = u16::from_le_bytes([wav[body], wav[body + 1]]);
            let channels = u16::from_le_bytes([wav[body + 2], wav[body + 3]]);
            let rate = u32::from_le_bytes(wav[body + 4..body + 8].try_into().unwrap());
            let bits = u16::from_le_bytes([wav[body + 14], wav[body + 15]]);
            format = Some((tag, channels, rate, bits));
        } else if id == b"data" {
            if format != Some((1, 1, 16_000, 16)) {
                return Err(format!(
                    "unsupported format {format:?}; need PCM16 mono 16 kHz"
                ));
            }
            return Ok(wav[body..end]
                .chunks_exact(2)
                .map(|bytes| f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0)
                .collect());
        }
        offset = end + (size & 1);
    }
    Err("no data chunk".into())
}

/// Repeat the speech so cancellation lands during recognition.
fn samples_for_cancel(wav: &[u8]) -> Result<Vec<f32>, String> {
    let once = pcm16_mono_16k(wav)?;
    Ok(once.iter().copied().cycle().take(once.len() * 40).collect())
}
