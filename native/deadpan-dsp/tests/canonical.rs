use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_dsp::{
    CanonicalRecipe, CanonicalStretch, DspError, ENGINE_ID, MAX_INPUT_FRAMES, MAX_INPUT_PEAK,
    MAX_OUTPUT_FRAMES, QUANTUM, StereoPcm,
};
use sha2::{Digest, Sha256};

// The mixed fixture retains the original measured bytes, independently of the
// compiler/libm used for Rust tests. Tiny impulses are exactly representable.
// Check the mixed input hash before using previously measured output hashes.
fn fixture(frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    if frames == 192_192 {
        let bytes = include_bytes!("fixtures/mixed.f32");
        assert_eq!(bytes.len(), frames * 8);
        for (n, frame) in bytes.chunks_exact(8).enumerate() {
            left[n] = f32::from_le_bytes(frame[..4].try_into().unwrap());
            right[n] = f32::from_le_bytes(frame[4..].try_into().unwrap());
        }
    } else {
        left[frames / 3] = 0.8;
        right[frames / 3] = -0.2;
    }
    (left, right)
}

fn pcm(frames: u32) -> StereoPcm {
    let (left, right) = fixture(frames as usize);
    StereoPcm::new(left, right).unwrap()
}

fn hash(left: &[f32], right: &[f32]) -> String {
    let mut hash = Sha256::new();
    for (&left, &right) in left.iter().zip(right) {
        hash.update(left.to_le_bytes());
        hash.update(right.to_le_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn render(recipe: CanonicalRecipe, pattern: &[usize]) -> (Vec<f32>, Vec<f32>) {
    let mut engine = CanonicalStretch::new(recipe, pcm(recipe.input_frames())).unwrap();
    let cancelled = AtomicBool::new(false);
    let mut left = vec![0.0; recipe.output_frames() as usize];
    let mut right = left.clone();
    let mut at = 0;
    for requested in pattern.iter().cycle() {
        let count = (*requested).min(left.len() - at);
        assert_eq!(
            engine
                .read(
                    &mut left[at..at + count],
                    &mut right[at..at + count],
                    &cancelled,
                )
                .unwrap(),
            count
        );
        at += count;
        if at == left.len() {
            break;
        }
    }
    assert_eq!(engine.position(), recipe.output_frames());
    let mut left_tail = [123.0; QUANTUM];
    let mut right_tail = [456.0; QUANTUM];
    assert_eq!(
        engine
            .read(&mut left_tail, &mut right_tail, &cancelled)
            .unwrap(),
        0
    );
    assert_eq!(left_tail, [123.0; QUANTUM]);
    assert_eq!(right_tail, [456.0; QUANTUM]);
    (left, right)
}

#[test]
fn matches_all_fifty_previously_measured_canonical_output_hashes() {
    let (left, right) = fixture(192_192);
    assert_eq!(
        hash(&left, &right),
        "80838601094aef41de8d08c40081baa302bc24fff0ac3b96dc7cc5fb98c0a224"
    );
    let mut cases = 0;
    for line in include_str!("canonical-sha256.txt").lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 4);
        let recipe = CanonicalRecipe::new(
            fields[0].parse().unwrap(),
            fields[1].parse().unwrap(),
            fields[2].parse().unwrap(),
        )
        .unwrap();
        let (left, right) = render(recipe, &[QUANTUM]);
        assert_eq!(hash(&left, &right), fields[3], "{recipe:?}");
        cases += 1;
    }
    assert_eq!(cases, 50);
}

#[test]
fn partition_and_exact_replay_preserve_samples_at_unaligned_positions() {
    let recipe = CanonicalRecipe::new(10_003, 13_337, 7).unwrap();
    let (left, right) = render(recipe, &[QUANTUM]);
    let (irregular_left, irregular_right) = render(recipe, &[1, 17, 253, 127, 256]);
    assert_eq!(hash(&left, &right), hash(&irregular_left, &irregular_right));
    for target in [10_669, 1_905, 13_336, 6_668] {
        let mut engine = CanonicalStretch::new(recipe, pcm(10_003)).unwrap();
        let cancelled = AtomicBool::new(false);
        engine.replay_to(target, &cancelled).unwrap();
        let mut actual_left = [123.0; QUANTUM];
        let mut actual_right = [456.0; QUANTUM];
        let count = engine
            .read(&mut actual_left, &mut actual_right, &cancelled)
            .unwrap();
        let start = target as usize;
        assert_eq!(
            hash(&actual_left[..count], &actual_right[..count]),
            hash(&left[start..start + count], &right[start..start + count])
        );
        assert!(actual_left[count..].iter().all(|sample| *sample == 123.0));
        assert!(actual_right[count..].iter().all(|sample| *sample == 456.0));
    }
}

#[test]
fn cancellation_and_invalid_requests_preserve_completed_progress() {
    let recipe = CanonicalRecipe::new(1_003, 1_337, -7).unwrap();
    let (expected_left, expected_right) = render(recipe, &[QUANTUM]);
    let mut engine = CanonicalStretch::new(recipe, pcm(1_003)).unwrap();
    let cancelled = AtomicBool::new(false);
    let mut left = [123.0; QUANTUM + 1];
    let mut right = [456.0; QUANTUM + 1];
    assert_eq!(
        engine.read(&mut left, &mut right, &cancelled),
        Err(DspError::OutputLength)
    );
    assert_eq!(
        engine.read(&mut left[..1], &mut right[..2], &cancelled),
        Err(DspError::OutputLength)
    );
    assert_eq!(engine.position(), 0);
    assert_eq!(left, [123.0; QUANTUM + 1]);
    engine.replay_to(17, &cancelled).unwrap();
    cancelled.store(true, Ordering::Relaxed);
    assert_eq!(engine.replay_to(500, &cancelled), Err(DspError::Cancelled));
    assert_eq!(
        engine.read(&mut left[..QUANTUM], &mut right[..QUANTUM], &cancelled),
        Err(DspError::Cancelled)
    );
    assert_eq!(engine.position(), 17);
    assert_eq!(left, [123.0; QUANTUM + 1]);
    assert_eq!(right, [456.0; QUANTUM + 1]);
    cancelled.store(false, Ordering::Relaxed);
    assert_eq!(engine.replay_to(16, &cancelled), Err(DspError::ReplayRange));
    assert_eq!(
        engine.replay_to(1_338, &cancelled),
        Err(DspError::ReplayRange)
    );
    assert_eq!(engine.position(), 17);
    engine.replay_to(500, &cancelled).unwrap();
    engine
        .read(&mut left[..QUANTUM], &mut right[..QUANTUM], &cancelled)
        .unwrap();
    assert_eq!(
        hash(&left[..QUANTUM], &right[..QUANTUM]),
        hash(&expected_left[500..756], &expected_right[500..756])
    );
    assert_eq!(engine.position(), 756);
}

#[test]
fn owns_input_across_moves_and_can_be_constructed_on_a_preparation_worker() {
    let source = pcm(31);
    let recipe = CanonicalRecipe::new(31, 62, 7).unwrap();
    let actual = std::thread::spawn(move || {
        let mut moved = vec![CanonicalStretch::new(recipe, source).unwrap()];
        let mut engine = moved.pop().unwrap();
        drop(moved);
        assert_eq!(engine.recipe(), recipe);
        let mut left = [0.0; 62];
        let mut right = [0.0; 62];
        engine
            .read(&mut left, &mut right, &AtomicBool::new(false))
            .unwrap();
        hash(&left, &right)
    })
    .join()
    .unwrap();
    assert_eq!(
        actual,
        "bea293ee4eced342662653dc87323a44282f915b2a20b4708a7c89341ed635b6"
    );
}

#[test]
fn validates_bounds_before_native_work_without_changing_levels() {
    for (input, output, pitch, error) in [
        (0, 1, 0, DspError::InputLength),
        (MAX_INPUT_FRAMES + 1, 1, 0, DspError::InputLength),
        (1, 0, 0, DspError::Rate),
        (1, 9, 0, DspError::Rate),
        (9, 1, 0, DspError::Rate),
        (1, u32::MAX, 0, DspError::Rate),
        (1, 1, -25, DspError::Pitch),
        (1, 1, 25, DspError::Pitch),
    ] {
        assert_eq!(CanonicalRecipe::new(input, output, pitch), Err(error));
    }
    assert!(CanonicalRecipe::new(MAX_INPUT_FRAMES, MAX_OUTPUT_FRAMES, -24).is_ok());
    assert!(CanonicalRecipe::new(MAX_INPUT_FRAMES, MAX_INPUT_FRAMES / 8, 24).is_ok());
    assert_eq!(
        StereoPcm::new(vec![], vec![]).unwrap_err(),
        DspError::InputLength
    );
    assert_eq!(
        StereoPcm::new(vec![0.0], vec![0.0; 2]).unwrap_err(),
        DspError::InputLength
    );
    assert_eq!(
        StereoPcm::new(vec![0.0; MAX_INPUT_FRAMES as usize + 1], vec![0.0]).unwrap_err(),
        DspError::InputLength
    );
    for sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            StereoPcm::new(vec![sample], vec![0.0]).unwrap_err(),
            DspError::NonFiniteInput
        );
        assert_eq!(
            StereoPcm::new(vec![0.0], vec![sample]).unwrap_err(),
            DspError::NonFiniteInput
        );
    }
    for sample in [MAX_INPUT_PEAK + 0.01, -MAX_INPUT_PEAK - 0.01, f32::MAX] {
        assert_eq!(
            StereoPcm::new(vec![sample], vec![0.0]).unwrap_err(),
            DspError::InputPeak
        );
    }
    let recipe = CanonicalRecipe::new(1, 1, 0).unwrap();
    assert_eq!(recipe.engine_id(), ENGINE_ID);
    assert_eq!(recipe.pitch_semitones(), 0);
    assert!(matches!(
        CanonicalStretch::new(recipe, pcm(2)),
        Err(DspError::InputLength)
    ));
    for peak in [0.0, MAX_INPUT_PEAK, -MAX_INPUT_PEAK] {
        let source = StereoPcm::new(vec![peak], vec![-peak]).unwrap();
        assert_eq!(source.frames(), 1);
        let mut engine = CanonicalStretch::new(recipe, source).unwrap();
        let mut left = [0.0];
        let mut right = [0.0];
        engine
            .read(&mut left, &mut right, &AtomicBool::new(false))
            .unwrap();
        assert!((left[0] - peak).abs() < 0.0001);
        assert!((right[0] + peak).abs() < 0.0001);
    }
}

#[test]
fn vendored_headers_and_notices_match_the_qualified_sources() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut count = 0;
    for line in include_str!("../vendor/SHA256SUMS").lines() {
        let (expected, path) = line.split_once("  ").unwrap();
        let bytes = std::fs::read(root.join(path)).unwrap();
        let actual: String = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(actual, expected, "{path}");
        count += 1;
    }
    assert_eq!(count, 10);
}
