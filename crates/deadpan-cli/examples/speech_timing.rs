//! Time transcript and pause projection onto a project's current edit.
//!
//! `cargo run --release -p deadpan-cli --example speech_timing -- PROJECT`

use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: speech_timing PROJECT")?;
    let store =
        deadpan_store::ProjectStore::open(path.as_ref(), deadpan_store::AccessMode::ReadOnly)?;
    let started = Instant::now();
    let speech = deadpan_cli::speech::StoredSpeech::load(&store).map_err(|e| e.message)?;
    let loaded = started.elapsed();
    let document = store.snapshot()?;
    let plan = deadpan_plan::RenderPlan::compile(&document)?;
    let frames = plan.duration().frames();
    let started = Instant::now();
    let rounds = 5;
    let mut runs = 0;
    for _ in 0..rounds {
        runs = speech
            .project(&document)
            .map_err(|e| e.message)?
            .runs()
            .len();
    }
    let each = started.elapsed() / rounds;
    println!(
        "load {loaded:?}; {frames} frames, {runs} runs: {each:?} per projection, {:.2} µs per frame",
        each.as_secs_f64() * 1e6 / frames as f64
    );
    Ok(())
}
