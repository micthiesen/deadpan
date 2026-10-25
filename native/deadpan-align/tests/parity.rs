//! Explicit, offline development qualification. No model or private recording
//! is retained in this crate; absent opt-in paths are reported as unexercised.

use deadpan_align::{AlignError, Control, Features, Normalization, Session, extract_features};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn control() -> Control {
    Control::new(
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(300),
    )
}

fn read_entry(root: &Path, entry: &Value) -> Vec<u8> {
    let file = entry["file"].as_str().unwrap();
    assert!(
        Path::new(file).is_relative()
            && !Path::new(file)
                .components()
                .any(|c| c == std::path::Component::ParentDir)
    );
    let bytes = fs::read(root.join(file)).unwrap();
    assert_eq!(bytes.len() as u64, entry["bytes"].as_u64().unwrap());
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    assert_eq!(digest, entry["sha256"].as_str().unwrap());
    bytes
}

fn f32s(bytes: &[u8]) -> Vec<f32> {
    assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect()
}

fn f64s(bytes: &[u8]) -> Vec<f64> {
    assert_eq!(bytes.len() % 8, 0);
    bytes
        .chunks_exact(8)
        .map(|v| f64::from_le_bytes(v.try_into().unwrap()))
        .collect()
}

#[test]
fn offline_native_reference_feature_and_emission_parity() {
    let Some(root) = std::env::var_os("DEADPAN_ALIGN_REFERENCE_ROOT").map(PathBuf::from) else {
        eprintln!(
            "UNEXERCISED: set DEADPAN_ALIGN_REFERENCE_ROOT for retained offline feature goldens"
        );
        return;
    };
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let stats = Normalization::from_f32le(
        &read_entry(&root, &manifest["constants"]["mean"]),
        &read_entry(&root, &manifest["constants"]["inverse_stddev"]),
    )
    .unwrap();
    let stats_json = fs::read(root.parent().unwrap().join("upstream/global_stats.json")).unwrap();
    Normalization::from_json(&stats_json).unwrap();
    let mut session = std::env::var_os("DEADPAN_ALIGN_CHECKPOINT")
        .map(|path| Session::from_checkpoint(fs::read(path).unwrap(), &control()).unwrap());
    let output = std::env::var_os("DEADPAN_ALIGN_PARITY_OUTPUT").map(PathBuf::from);
    if let Some(output) = &output {
        fs::create_dir_all(output).unwrap();
    }
    let records = manifest["records"].as_array().unwrap();
    assert_eq!(records.len(), 24);
    let mut metrics = Vec::new();
    for record in records {
        let case = record["case"].as_str().unwrap();
        let pcm = f32s(&read_entry(&root, &record["pcm"]));
        let expected = f32s(&read_entry(&root, &record["features"]));
        let features = extract_features(&pcm, &stats, &control()).unwrap();
        assert_eq!(
            features.frames() as u64,
            record["feature_length"].as_u64().unwrap()
        );
        assert_eq!(features.values().len(), expected.len());
        let feature_max = features
            .values()
            .iter()
            .zip(&expected)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
            .fold(0_f64, f64::max);
        let feature_mean = features
            .values()
            .iter()
            .zip(&expected)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
            .sum::<f64>()
            / expected.len() as f64;
        // This numerical comparison is separate from downstream exact path,
        // interval and guard checks, owned by deadpan-analysis qualification.
        assert!(
            feature_max < 0.001,
            "{case}: feature max error {feature_max}"
        );
        let mut emission_max = None;
        let mut emission_mean = None;
        if let Some(session) = session.as_mut() {
            let emissions = session.infer(&features, &control()).unwrap();
            let expected = f64s(&read_entry(&root, &record["emissions"]));
            assert_eq!(emissions.log_probs.len(), expected.len());
            let widened = emissions.widened();
            let maximum = widened
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0_f64, f64::max);
            let mean = widened
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .sum::<f64>()
                / expected.len() as f64;
            assert!(maximum < 0.001, "{case}: emission max error {maximum}");
            emission_max = Some(maximum);
            emission_mean = Some(mean);
            if let Some(output) = &output {
                let bytes: Vec<u8> = widened.iter().flat_map(|v| v.to_le_bytes()).collect();
                fs::write(output.join(format!("{case}.emissions.f64le")), bytes).unwrap();
                let bytes: Vec<u8> = features
                    .values()
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                fs::write(output.join(format!("{case}.features.f32le")), bytes).unwrap();
            }
        }
        eprintln!(
            "{case}: feature max={feature_max:.9} mean={feature_mean:.9}; emission max={emission_max:?}"
        );
        metrics.push(serde_json::json!({"case":case,"feature_max_abs":feature_max,"feature_mean_abs":feature_mean,
            "emission_max_abs":emission_max,"emission_mean_abs":emission_mean}));
    }
    if session.is_none() {
        eprintln!("UNEXERCISED: set DEADPAN_ALIGN_CHECKPOINT for native emission comparison");
    }
    if let Some(output) = output {
        fs::write(
            output.join("metrics.json"),
            serde_json::to_vec_pretty(&metrics).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn cancellation_and_deadline_do_not_return_model_output() {
    let cancelled = Arc::new(AtomicBool::new(true));
    let stopped = Control::new(cancelled, Instant::now() + Duration::from_secs(5));
    assert!(matches!(
        Session::from_checkpoint(Vec::new(), &stopped),
        Err(AlignError::Cancelled)
    ));
    let expired = Control::new(Arc::new(AtomicBool::new(false)), Instant::now());
    assert!(matches!(
        Session::from_checkpoint(Vec::new(), &expired),
        Err(AlignError::Deadline)
    ));

    let Some(path) = std::env::var_os("DEADPAN_ALIGN_CHECKPOINT") else {
        eprintln!("UNEXERCISED: set DEADPAN_ALIGN_CHECKPOINT for active native Run cancellation");
        return;
    };
    let mut session = Session::from_checkpoint(fs::read(path).unwrap(), &control()).unwrap();
    let features = Features::from_values(3001, vec![0.; 3001 * 80]).unwrap();
    let flag = Arc::new(AtomicBool::new(false));
    let run_control = Control::new(flag.clone(), Instant::now() + Duration::from_secs(30));
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5));
        flag.store(true, Ordering::Relaxed);
    });
    let result = session.infer(&features, &run_control);
    thread.join().unwrap();
    assert!(matches!(result, Err(AlignError::Cancelled)), "{result:?}");
    let short_deadline = Control::new(
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_millis(5),
    );
    assert!(matches!(
        session.infer(&features, &short_deadline),
        Err(AlignError::Deadline)
    ));
    // A new RunOptions must permit reuse after a terminated run.
    let tiny = Features::from_values(3, vec![0.; 3 * 80]).unwrap();
    assert_eq!(session.infer(&tiny, &control()).unwrap().frames, 2);
}
