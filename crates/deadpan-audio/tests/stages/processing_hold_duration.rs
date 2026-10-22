//! Canonical PCM regressions inspect the actual processor input tape.
use super::*;
use deadpan_plan::{
    AudioQueryLimits, AudioSignalContent, AudioSignalTape, AudioSignalTapeRun, AudioStage,
};

#[path = "processing_hold_duration_output.rs"]
mod output;

#[path = "processing_hold_duration_repeat.rs"]
mod repeat;

#[path = "processing_hold_duration_output_sound.rs"]
mod output_sound;

fn voice() -> BeatNode {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut node = source(rate, 4, 0..1);
    let NodeKind::Source { source } = &mut node.kind else {
        panic!()
    };
    let audio = audio_at_rate(3100, 23_100, 44_100);
    source.audio_mapping = SourceAudioMapping::natural_rate(audio.span, rate).unwrap();
    source.audio = Some(audio);
    node
}

fn fixture(pitch: PitchPolicy, crop: i64, prebound: bool) -> ProjectDocument {
    let output = if pitch == PitchPolicy::Preserve {
        4
    } else {
        8 - crop
    };
    let document = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["processor"],
        [
            ("processor", retime("body", output, crop..8, pitch)),
            (
                "body",
                BeatNode::sequence(
                    "Processor operand",
                    vec![id("lead"), id("pause"), id("speech")],
                ),
            ),
            ("lead", hold(3)),
            ("pause", hold(2)),
            ("speech", voice()),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    if !prebound {
        assert!(document.audio_bindings().is_empty());
        return document;
    }
    let bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("processing-old-input-clock").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn processing_stage(plan: &RenderPlan) -> AudioStage<'_> {
    let query = plan
        .audio_signal()
        .query(
            SignalSample(0)..SignalSample(1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    match query.spans.into_iter().next().unwrap().content {
        AudioSignalContent::Stage(stage) => stage,
        // Capturing the old fixture also retains the processing output. This
        // asks that live bound owner for its actual intrinsic processing stage;
        // descendants keep their bindings. No descriptor is reconstructed.
        AudioSignalContent::Bound(bound) => bound.intrinsic_stage().unwrap().expect("processor"),
        other => panic!("expected actual processing stage, got {other:?}"),
    }
}

fn input_pcm(
    plan: &Arc<RenderPlan>,
    provider: &mut FixtureProvider,
    crop: i64,
    start: i64,
    chunks: &[u32],
) -> Vec<[f32; 2]> {
    let result = input_record(
        plan,
        provider,
        ratio(crop.into(), 1)..ratio(8, 1),
        start..start + 256,
        chunks,
    );
    assert!(
        result.suppressed.iter().all(|value| !value),
        "the interior Source window must remain audible"
    );
    result.samples
}

fn input_record(
    plan: &Arc<RenderPlan>,
    provider: &mut FixtureProvider,
    support: Range<ExactRatio>,
    range: Range<i64>,
    chunks: &[u32],
) -> Recorded {
    let stage = processing_stage(plan);
    input_record_for_stage(plan, provider, stage, support, range, chunks)
}

fn input_record_for_stage(
    plan: &Arc<RenderPlan>,
    provider: &mut FixtureProvider,
    stage: AudioStage<'_>,
    support: Range<ExactRatio>,
    range: Range<i64>,
    chunks: &[u32],
) -> Recorded {
    provider
        .revisions
        .insert(plan.metadata().revision_id.clone());
    let signal = stage.input_signal();
    assert_eq!(signal.support(), support);
    let tape = AudioSignalTape::new(
        plan,
        support.clone(),
        vec![AudioSignalTapeRun::new(support.clone(), support, signal)],
    )
    .unwrap();
    assert_eq!(
        tape.sample_count().unwrap().0,
        ratio(8008, 5)
            .checked_mul(
                tape.support()
                    .end
                    .checked_sub(tape.support().start)
                    .unwrap()
            )
            .unwrap()
            .ceil()
            .unwrap() as i64
    );
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut result = Recorded {
        samples: Vec::new(),
        suppressed: Vec::new(),
    };
    for count in chunks.iter().copied().cycle() {
        if result.samples.len() == (range.end - range.start) as usize {
            break;
        }
        let start = range.start + result.samples.len() as i64;
        let count = count.min((range.end - start) as u32);
        let block = renderer
            .read_tape(
                provider,
                &tape,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(&block.revision_id, &plan.metadata().revision_id);
        assert_eq!(block.stage, "projected_preparation_pcm_before_effects");
        result
            .suppressed
            .extend((start..start + i64::from(count)).map(|sample| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&SignalSample(sample)))
            }));
        result.samples.extend(block.samples);
    }
    result
}

fn literal_pcm(provider: &FixtureProvider, phase: ExactRatio, origin: i64) -> Vec<[f32; 2]> {
    finite_pcm(
        provider,
        3100..23_100,
        phase,
        origin,
        ratio(147, 160),
        origin + 256,
    )
}

fn finite_pcm(
    provider: &FixtureProvider,
    selection: Range<i64>,
    phase: ExactRatio,
    origin: i64,
    step: ExactRatio,
    end: i64,
) -> Vec<[f32; 2]> {
    let recipe = ResampleRecipe::new(
        selection,
        phase,
        AudioSample(origin),
        step,
        AudioSample(origin)..AudioSample(end),
    )
    .unwrap();
    let mut samples = Vec::new();
    while samples.len() < (end - origin) as usize {
        let count = ((end - origin) as usize - samples.len()).min(251) as u32;
        samples.extend(
            provider
                .source
                .prepare(
                    recipe.clone(),
                    AudioSample(origin + samples.len() as i64),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    samples
}

fn resize(document: &ProjectDocument, frames: i64, revision: &str) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command: Command::SetHoldDuration {
                node: id("pause"),
                duration: duration(frames),
            },
        },
    )
    .unwrap();
    assert_eq!(transaction.duration_delta, 0);
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(after.duration().unwrap(), document.duration().unwrap());
    assert_eq!(
        after.nodes()[&id("processor")],
        document.nodes()[&id("processor")]
    );
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn same_recorded(actual: &Recorded, expected: &Recorded, start: i64, label: &str) {
    same_pcm(&actual.samples, &expected.samples, start, label);
    assert_eq!(
        actual.suppressed.len(),
        expected.suppressed.len(),
        "{label} mask length"
    );
    for (offset, (actual, expected)) in actual
        .suppressed
        .iter()
        .zip(&expected.suppressed)
        .enumerate()
    {
        assert_eq!(
            actual,
            expected,
            "{label} suppression at {}",
            start + offset as i64
        );
    }
}

fn same_pcm(actual: &[[f32; 2]], expected: &[[f32; 2]], start: i64, label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label} length");
    for (offset, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..2 {
            assert_eq!(
                actual[channel],
                expected[channel],
                "{label} input sample {} channel {channel}",
                start + offset as i64
            );
        }
    }
}

fn witness(pitch: PitchPolicy, crop: i64, prebound: bool) {
    let before = fixture(pitch, crop, prebound);
    before.validate().unwrap();
    let before_plan = Arc::new(RenderPlan::compile(&before).unwrap());
    let (_, mut provider) = mono_original(2);
    // PointCeil uses the selected input's start as its grid origin.
    // crop0: old frame5 ->8008, phase3100; new frame6 ->9610,
    // freshly sampled phase3100+147/400.
    // crop1: old frame5 ->ceil(4*1601.6)=6407, phase3100+441/800;
    // new frame6 ->8008, fresh phase3100. All constants are literal, and
    // the Source windows are far from either old/current finite crop end.
    let (old_start, new_start, old_phase, fresh_phase) = if crop == 0 {
        (8008, 9610, ratio(3100, 1), ratio(1_240_147, 400))
    } else {
        (6407, 8008, ratio(2_480_441, 800), ratio(3100, 1))
    };
    let label = format!("pitch={pitch:?} crop={crop} prebound={prebound}");
    let expected = literal_pcm(&provider, old_phase, old_start);
    let fresh = literal_pcm(&provider, fresh_phase, new_start);
    assert!(expected.iter().flatten().any(|value| value.abs() > 0.01));
    assert!(
        expected != fresh,
        "the chosen canonical PCM detects this phase difference"
    );
    let old = input_pcm(&before_plan, &mut provider, crop, old_start, &[256]);
    same_pcm(
        &old,
        &expected,
        old_start,
        &format!("old literal input {label}"),
    );
    same_pcm(
        &input_pcm(&before_plan, &mut provider, crop, old_start, &[1, 127, 3]),
        &old,
        old_start,
        &format!("old chunk parity {label}"),
    );
    let transaction = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new("processing-hold-duration").unwrap(),
            command: Command::SetHoldDuration {
                node: id("pause"),
                duration: duration(3),
            },
        },
    )
    .unwrap();
    assert_eq!(transaction.duration_delta, 0);
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(
        after.nodes()[&id("processor")],
        before.nodes()[&id("processor")]
    );
    assert_eq!(after.nodes()[&id("speech")], before.nodes()[&id("speech")]);
    let after = ProjectDocument::from_json(&after.to_json().unwrap()).unwrap();
    let after_plan = Arc::new(RenderPlan::compile(&after).unwrap());
    let actual = input_pcm(&after_plan, &mut provider, crop, new_start, &[127, 3, 251]);
    // If the regression is present, this diagnostic proves it is exactly the
    // independently predicted fresh phase, not just an arbitrary difference.
    let reset_to_fresh_phase = actual == fresh;
    same_pcm(
        &actual,
        &expected,
        new_start,
        &format!("retained OLD input phase {label}; reset_to_fresh_phase={reset_to_fresh_phase}"),
    );
    same_pcm(
        &input_pcm(&after_plan, &mut provider, crop, new_start, &[1, 239]),
        &actual,
        new_start,
        &format!("new chunk parity {label}"),
    );
}

#[test]
fn ordinary_hold_growth_under_preserve_retains_44100_phase_on_actual_input_tape() {
    witness(PitchPolicy::Preserve, 0, false);
}

#[test]
fn ordinary_hold_growth_under_shift_retains_44100_phase_on_actual_input_tape() {
    witness(PitchPolicy::Shift { semitones: 3 }, 0, false);
}

macro_rules! input_control {
    ($name:ident, $pitch:expr, $crop:expr, $bound:expr) => {
        #[test]
        fn $name() {
            witness($pitch, $crop, $bound);
        }
    };
}
input_control!(
    preserve_prebound_uncropped_input_clock,
    PitchPolicy::Preserve,
    0,
    true
);
input_control!(
    preserve_prebound_cropped_input_clock,
    PitchPolicy::Preserve,
    1,
    true
);
input_control!(
    preserve_unbound_cropped_input_clock,
    PitchPolicy::Preserve,
    1,
    false
);
input_control!(
    shift_prebound_uncropped_input_clock,
    PitchPolicy::Shift { semitones: 3 },
    0,
    true
);
input_control!(
    shift_prebound_cropped_input_clock,
    PitchPolicy::Shift { semitones: 3 },
    1,
    true
);
input_control!(
    shift_unbound_cropped_input_clock,
    PitchPolicy::Shift { semitones: 3 },
    1,
    false
);

// Whole CURRENT selected operand, with literal old PointCeil phase and the
// current native ceil source crop. Historical allocations are also PointCeil.
fn direct_input_reference(provider: &FixtureProvider, crop: i64, frames: i64) -> Recorded {
    let (start, end, historical_length, phase) = match (crop, frames) {
        (0, 1) => (6407, 12813, 6407, ratio(3100, 1)),
        (0, 2) => (8008, 12813, 4805, ratio(3100, 1)),
        (0, 3) => (9610, 12813, 3204, ratio(3100, 1)),
        (1, 1) => (4805, 11212, 6406, ratio(2_480_441, 800)),
        (1, 2) => (6407, 11212, 4805, ratio(2_480_441, 800)),
        (1, 3) => (8008, 11212, 3203, ratio(2_480_441, 800)),
        _ => panic!("literal fixture"),
    };
    let native_end = match frames {
        1 => 8986,
        2 => 7515,
        3 => 6043,
        _ => unreachable!(),
    };
    let admitted = (end - start).min(historical_length);
    let source = finite_pcm(
        provider,
        3100..native_end,
        phase,
        start,
        ratio(147, 160),
        start + admitted,
    );
    let mut result = Recorded {
        samples: vec![[0.; 2]; end as usize],
        suppressed: vec![true; end as usize],
    };
    result.samples[start as usize..(start + admitted) as usize].copy_from_slice(&source);
    result.suppressed[start as usize..(start + admitted) as usize].fill(false);
    // Cropped growth: old local2 allocation6407..9610 has3203 points;
    // live8008..11212 has3204. Cropped shrink: old local4 allocation
    //6407..12813 has6406; live4805..11212 has6407. Both exhaust one slot.
    assert_eq!(
        end - start - admitted,
        if crop == 1 && frames != 2 { 1 } else { 0 }
    );
    result
}

fn complete_inputs(pitch: PitchPolicy) {
    for crop in [0, 1] {
        for prebound in [false, true] {
            let before = fixture(pitch, crop, prebound);
            let (_, mut provider) = mono_original(2);
            let total = if crop == 0 { 12813 } else { 11212 };
            let support = ratio(crop.into(), 1)..ratio(8, 1);
            let old_plan = Arc::new(RenderPlan::compile(&before).unwrap());
            let old = input_record(
                &old_plan,
                &mut provider,
                support.clone(),
                0..total,
                &[251, 3],
            );
            same_recorded(
                &old,
                &direct_input_reference(&provider, crop, 2),
                0,
                "complete old input",
            );
            for frames in [1, 3] {
                let after = resize(&before, frames, "processing-full-input-duration");
                let plan = Arc::new(RenderPlan::compile(&after).unwrap());
                let actual = input_record(
                    &plan,
                    &mut provider,
                    support.clone(),
                    0..total,
                    &[127, 3, 251],
                );
                let label = format!(
                    "complete current input pitch={pitch:?} crop={crop} bound={prebound} frames={frames}"
                );
                same_recorded(
                    &actual,
                    &direct_input_reference(&provider, crop, frames),
                    0,
                    &label,
                );
                same_recorded(
                    &input_record(&plan, &mut provider, support.clone(), 0..total, &[1, 239]),
                    &actual,
                    0,
                    &label,
                );
                if crop == 1 {
                    assert_eq!(actual.samples[11211], [0.; 2]);
                    assert!(actual.suppressed[11211]);
                }
            }
        }
    }
}

#[test]
fn preserve_complete_grown_and_shrunk_input_keeps_current_crop_and_point_exhaustion() {
    complete_inputs(PitchPolicy::Preserve);
}
#[test]
fn shift_complete_grown_and_shrunk_input_keeps_current_crop_and_point_exhaustion() {
    complete_inputs(PitchPolicy::Shift { semitones: 3 });
}

fn affine_input(pitch: PitchPolicy) {
    let document = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["processor"],
        [
            (
                "processor",
                retime(
                    "affine",
                    if pitch == PitchPolicy::Preserve { 3 } else { 6 },
                    0..6,
                    pitch,
                ),
            ),
            ("affine", retime("body", 6, 0..8, PitchPolicy::FollowSpeed)),
            (
                "body",
                BeatNode::sequence("Affine input", vec![id("lead"), id("pause"), id("speech")]),
            ),
            ("lead", hold(3)),
            ("pause", hold(2)),
            ("speech", voice()),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let (_, mut provider) = mono_original(2);
    let before = Arc::new(RenderPlan::compile(&document).unwrap());
    let old = input_record(
        &before,
        &mut provider,
        ratio(0, 1)..ratio(6, 1),
        6006..6262,
        &[256],
    );
    let expected = finite_pcm(
        &provider,
        3100..23_100,
        ratio(3100, 1),
        6006,
        ratio(49, 40),
        6262,
    );
    same_pcm(&old.samples, &expected, 6006, "old affine PointCeil phase");
    let after = resize(&document, 3, "affine-processing-duration");
    let plan = Arc::new(RenderPlan::compile(&after).unwrap());
    let actual = input_record(
        &plan,
        &mut provider,
        ratio(0, 1)..ratio(6, 1),
        0..9610,
        &[127, 3, 251],
    );
    // Inner slope3/4: old entry15/4 ->6006, new9/2 ->ceil7207.2=7208.
    // PointCeil is essential:7207 is the ROOT grid, not this input's grid.
    // Current local2 support on old lattice is[15/4,21/4), ceil6006..8409
    // length2403; live7208..9610 has2402 points, so no terminal exhaustion.
    let source = finite_pcm(
        &provider,
        3100..6043,
        ratio(3100, 1),
        7208,
        ratio(49, 40),
        9610,
    );
    let mut reference = Recorded {
        samples: vec![[0.; 2]; 9610],
        suppressed: vec![true; 9610],
    };
    reference.samples[7208..].copy_from_slice(&source);
    reference.suppressed[7208..].fill(false);
    same_recorded(&actual, &reference, 0, "processing above affine input");
    same_recorded(
        &input_record(
            &plan,
            &mut provider,
            ratio(0, 1)..ratio(6, 1),
            0..9610,
            &[1, 239],
        ),
        &actual,
        0,
        "affine input chunks",
    );
}

#[test]
fn preserve_above_affine_uses_point_7208_and_literal_step_49_over_40() {
    affine_input(PitchPolicy::Preserve);
}
#[test]
fn shift_above_affine_uses_point_7208_and_literal_step_49_over_40() {
    affine_input(PitchPolicy::Shift { semitones: 3 });
}

#[test]
fn prior_cropped_point_resume_composes_two_hold_resizes_from_the_current_entry() {
    let document = fixture(PitchPolicy::Preserve, 1, true);
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"]["bindings"]["speech"]["resume"] = serde_json::to_value(AudioResume {
        local_boundary: ExactRatio::ZERO,
        phase: AudioLocalPhase {
            constant: ratio(5, 8008),
            terms: vec![],
        },
    })
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let initial = &before.audio_bindings().bindings()[&id("speech")];
    let (_, mut provider) = mono_original(2);
    // Old cropped first phase3100+441/800, plus one retained mix point147/160
    // =3101.47 exactly. A second capture must not restart at its old lattice.
    let expected = literal_pcm(&provider, ratio(310_147, 100), 6407);
    same_pcm(
        &input_pcm(
            &Arc::new(RenderPlan::compile(&before).unwrap()),
            &mut provider,
            1,
            6407,
            &[256],
        ),
        &expected,
        6407,
        "old resumed input",
    );
    let first = resize(&before, 1, "first-resumed-input-duration");
    let second = resize(&first, 3, "second-resumed-input-duration");
    for (document, entry) in [(&first, 4805), (&second, 8008)] {
        let binding = &document.audio_bindings().bindings()[&id("speech")];
        assert_eq!(binding.lattice, initial.lattice);
        assert_eq!(binding.resume, initial.resume);
        assert!(binding.reanchors.starts_with(&initial.reanchors));
        same_pcm(
            &input_pcm(
                &Arc::new(RenderPlan::compile(document).unwrap()),
                &mut provider,
                1,
                entry,
                &[127, 3],
            ),
            &expected,
            entry,
            "composed current input entry",
        );
    }
    assert!(
        second.audio_bindings().bindings()[&id("speech")]
            .reanchors
            .starts_with(&first.audio_bindings().bindings()[&id("speech")].reanchors)
    );
}
