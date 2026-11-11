//! Word-timing evaluation helper: transcribes raw 16 kHz mono f32le files and
//! prints one JSON line per file with every token's heuristic and DTW times.
//! `examples/word_eval.py` prepares the speech and scores the output.
//!
//! `cargo run --release -p deadpan-transcribe --example word_eval -- MODEL FILE...`

#[path = "../src/engine.rs"]
mod engine;

use std::time::Instant;

fn main() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let model = arguments.next().ok_or("usage: word_eval MODEL FILE...")?;
    let context = engine::load(&model, std::env::var_os("WORD_EVAL_NO_DTW").is_none())?;
    for file in arguments {
        let bytes = std::fs::read(&file).map_err(|error| format!("{file}: {error}"))?;
        let pcm: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let started = Instant::now();
        let segments = engine::recognize(&context, &pcm, "en", |_| {})?;
        let seconds = started.elapsed().as_secs_f64();
        let worker = engine::to_raw(&segments);
        let duration_cs = u32::try_from(pcm.len() / 160).map_err(|_| "too long")?;
        let audio = deadpan_analysis::AnalysedAudio {
            origin: 0,
            sample_rate: 48_000,
            duration_cs,
        };
        let transcript = deadpan_analysis::Transcript::from_segments(audio, &worker)
            .map_err(|error| error.to_string())?;
        let non_speech = transcript.non_speech();
        let words: Vec<_> = transcript
            .words()
            .iter()
            .zip(&non_speech)
            .map(|(w, n)| serde_json::json!([w.text, w.start_cs, w.end_cs, n]))
            .collect();
        let segments: Vec<_> = segments
            .iter()
            .map(|s| {
                let tokens: Vec<_> = s
                    .tokens
                    .iter()
                    .map(|t| {
                        serde_json::json!({"text": t.text, "t0": t.t0, "t1": t.t1,
                            "dtw": t.t_dtw, "p": t.p})
                    })
                    .collect();
                serde_json::json!({"t0": s.t0, "t1": s.t1, "tokens": tokens})
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({"file": file, "seconds": seconds, "segments": segments,
                "worker": worker, "words": words})
        );
    }
    Ok(())
}
