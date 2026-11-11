//! Evaluates the automatic phoneme refinement of transcript words: runs
//! `refine_transcript` over the words of a `word_eval` run and prints the
//! refined words. `native/deadpan-transcribe/examples/word_eval.py` scores it.
//!
//! `word_refine_eval ALIGN_WORKER PACK_DIR RUN.jsonl OUT.jsonl`

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_analysis::{AnalysedAudio, Transcript, Word};
use deadpan_cli::models::RefinementModels;
use deadpan_cli::refinement::{RefinementRuntime, TranscriptRefiner, refine_transcript};
use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::ModelInput;
use sha2::Digest;

fn input(dir: &std::path::Path, name: &str) -> Result<ModelInput, String> {
    let path = dir.join(name);
    let bytes = std::fs::read(&path).map_err(|e| format!("{name}: {e}"))?;
    let hash: String = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(ModelInput {
        path: path.canonicalize().map_err(|e| e.to_string())?,
        sha256: Sha256::new(hash).map_err(|e| e.to_string())?,
        byte_length: bytes.len() as u64,
    })
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [worker, pack, run, out] = &args[..] else {
        return Err("usage: word_refine_eval ALIGN_WORKER PACK_DIR RUN.jsonl OUT.jsonl".into());
    };
    let pack = std::path::Path::new(pack);
    let lexicon = deadpan_analysis::refinement::PronunciationLexicon::parse(
        &std::fs::read_to_string(pack.join("cmudict.dict")).map_err(|e| e.to_string())?,
        &std::fs::read_to_string(pack.join("cmudict.phones")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let refiner = TranscriptRefiner {
        runtime: RefinementRuntime {
            executable: worker.into(),
            environment: BTreeMap::new(),
        },
        models: RefinementModels {
            checkpoint: input(pack, "model-0.6.ckpt")?,
            statistics: input(pack, "global_stats.json")?,
            lexicon,
            dictionary_sha256: String::new(),
            phones_sha256: String::new(),
            pack_id: "phoneme-ctc-en".into(),
            pack_version: "1".into(),
        },
    };
    let cancelled = AtomicBool::new(false);
    let mut output = std::fs::File::create(out).map_err(|e| e.to_string())?;
    for line in std::fs::read_to_string(run)
        .map_err(|e| e.to_string())?
        .lines()
    {
        let record: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let file = record["file"].as_str().ok_or("file")?;
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        let pcm: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let words: Vec<Word> = record["words"]
            .as_array()
            .ok_or("words")?
            .iter()
            .map(|w| Word {
                text: w[0].as_str().unwrap_or_default().to_owned(),
                start_cs: w[1].as_u64().unwrap_or(0) as u32,
                end_cs: w[2].as_u64().unwrap_or(0) as u32,
                probability: 0.9,
                segment: 0,
            })
            .collect();
        let audio = AnalysedAudio {
            origin: 0,
            sample_rate: 48_000,
            duration_cs: (pcm.len() / 160) as u32,
        };
        let transcript = Transcript::new(audio, words).map_err(|e| e.to_string())?;
        let started = Instant::now();
        let refined = refine_transcript(
            &refiner,
            &transcript,
            &pcm,
            &cancelled,
            Instant::now() + Duration::from_secs(300),
        )
        .map_err(|e| e.to_string())?;
        if refined.failed_windows > 0 {
            return Err(format!("aligner windows failed: {:?}", refined.first_error));
        }
        let refined = refined.transcript;
        let result = serde_json::json!({
            "file": file,
            "audio_seconds": pcm.len() as f64 / 16000.0,
            "seconds": started.elapsed().as_secs_f64(),
            "words": refined.words().iter().map(|w| serde_json::json!([w.text, w.start_cs, w.end_cs])).collect::<Vec<_>>(),
        });
        writeln!(output, "{result}").map_err(|e| e.to_string())?;
    }
    Ok(())
}
