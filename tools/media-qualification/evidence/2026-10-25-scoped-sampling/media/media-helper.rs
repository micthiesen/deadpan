//! Read-only qualification of two retained revisions. No authored-state APIs.
use std::{
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_cli::{audio::ProjectAudioSession, export_verification::reference_pictures};
use deadpan_core::{AudioSample, ProjectFrame, RevisionId};
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::json;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
const SAMPLES: i64 = 384_384;
const FRAMES: i64 = 240;

fn pcm(package: &Path, revision: &RevisionId, output: &Path, partitions: &[u32]) -> Result {
    let mut session = ProjectAudioSession::open_revision(package, revision)?;
    assert_eq!(session.revision(), revision);
    assert_eq!(session.plan().audio_duration()?.0, SAMPLES);
    let cancelled = AtomicBool::new(false);
    let mut bytes = BufWriter::new(File::create_new(output)?);
    let mut at = 0;
    let mut partition = 0;
    while at < SAMPLES {
        let count = u32::try_from((SAMPLES - at).min(i64::from(partitions[partition])))?;
        let authored = session.read_authored_bus(AudioSample(at), count, &cancelled)?;
        let limited = session.read_limited(AudioSample(at), count, &cancelled)?;
        assert_eq!(authored.start, AudioSample(at));
        assert_eq!(limited.start, AudioSample(at));
        assert_eq!(authored.samples.len(), count as usize);
        assert_eq!(limited.samples.len(), count as usize);
        assert_eq!(limited.gain.len(), count as usize);
        for ((authored, limited), gain) in authored
            .samples
            .iter()
            .zip(&limited.samples)
            .zip(&limited.gain)
        {
            for sample in authored.iter().chain(limited) {
                assert!(sample.is_finite());
                bytes.write_all(&sample.to_bits().to_le_bytes())?;
            }
            assert!(gain.is_finite());
            bytes.write_all(&gain.to_bits().to_le_bytes())?;
        }
        at += i64::from(count);
        partition = (partition + 1) % partitions.len();
    }
    bytes.flush()?;
    Ok(())
}

fn collect(
    package: &Path,
    revision: &RevisionId,
    output: &Path,
    label: &str,
    partitions: &[u32],
) -> Result {
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let document = store.snapshot_at(revision)?;
    let plan = RenderPlan::compile(&document)?;
    assert_eq!(plan.duration().frames(), FRAMES);
    assert_eq!(plan.audio_duration()?.0, SAMPLES);
    let pictures = (0..FRAMES)
        .map(|frame| plan.picture(ProjectFrame(frame)))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let audio = plan.audio(AudioSample(0)..AudioSample(SAMPLES), Default::default())?;
    fs::write(
        output.join(format!("{label}-document.json")),
        serde_json::to_vec_pretty(&document)?,
    )?;
    fs::write(
        output.join(format!("{label}-plan.json")),
        serde_json::to_vec(&json!({"pictures":pictures,"audio":audio}))?,
    )?;
    drop(store);
    pcm(
        package,
        revision,
        &output.join(format!("{label}-pcm.bin")),
        partitions,
    )?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(600);
    // 240 * 320 * 180 * 3/2 * sizeof(u16) = 41.5 MB per call.
    let references = reference_pictures(
        package,
        revision,
        &(0..FRAMES as u64).collect::<Vec<_>>(),
        &cancelled,
        deadline,
    )?;
    assert_eq!(references.len(), FRAMES as usize);
    let mut pixels = BufWriter::new(File::create_new(
        output.join(format!("{label}-reference.yuv")),
    )?);
    let mut provenance = Vec::new();
    for (ordinal, frame) in references.into_iter().enumerate() {
        assert_eq!(
            (frame.planes.width, frame.planes.height, frame.planes.bits),
            (320, 180, 8)
        );
        for sample in frame
            .planes
            .y
            .into_iter()
            .chain(frame.planes.cb)
            .chain(frame.planes.cr)
        {
            pixels.write_all(&[u8::try_from(sample)?])?;
        }
        provenance.push(json!({"frame":ordinal,"provenance":frame.provenance,"output_color":frame.output_color}));
    }
    pixels.flush()?;
    fs::write(
        output.join(format!("{label}-reference-provenance.json")),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    Ok(())
}

fn main() -> Result {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 {
        return Err("media-helper PACKAGE BASELINE_REVISION FINAL_REVISION OUTPUT".into());
    }
    let package = Path::new(&args[0]);
    let baseline = RevisionId::new(args[1].to_str().ok_or("UTF-8 baseline")?)?;
    let final_revision = RevisionId::new(args[2].to_str().ok_or("UTF-8 final revision")?)?;
    let output = Path::new(&args[3]);
    assert!(output.is_dir());
    collect(package, &baseline, output, "baseline", &[256])?;
    collect(package, &final_revision, output, "final", &[37, 251, 173])?;
    let before = fs::read(output.join("baseline-pcm.bin"))?;
    let after = fs::read(output.join("final-pcm.bin"))?;
    assert_eq!(before.len(), SAMPLES as usize * 24);
    assert_eq!(
        after, before,
        "complete authored stereo/limited stereo/limiter-gain bits changed"
    );
    fs::write(
        output.join("helper-result.json"),
        serde_json::to_vec_pretty(&json!({
            "baseline_revision":baseline,"final_revision":final_revision,
            "sample_frames":SAMPLES,"channels":2,"frames":FRAMES,
            "pcm_record":"LE authored f32x2 bits, limited f32x2 bits, gain f64 bits; 24 bytes/sample-frame",
            "baseline_partitions":[256],"final_partitions":[37,251,173],
            "canonical_pcm_bits_equal":true,"read_only":true,
            "independent_DSP":false,"independent_renderer":false
        }))?,
    )?;
    Ok(())
}
