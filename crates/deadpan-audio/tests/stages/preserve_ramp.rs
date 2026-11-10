//! A pitch-preserving continuous ramp through `StageAudio`: one stage whose
//! stretch schedule is the exact quadratic, independent of read sizes.
use super::*;
use deadpan_dsp::{RampSchedule, StreamRecipe, StreamingStretch};

const RATE: (u32, u32) = (48_000, 1);

fn ramp(pitch: fn(SpeedRamp) -> PitchPolicy) -> PitchPolicy {
    pitch(SpeedRamp::new(ratio(1, 2), ratio(2, 1)).unwrap())
}

/// 4,000 Original samples ramped from 1/2x to 2x: the mean speed 5/4 gives
/// exactly 3,200 output frames (one sample each at 48,000 fps).
fn ramp_plan(pitch: PitchPolicy) -> Arc<RenderPlan> {
    let rate = FrameRate::new(RATE.0, RATE.1).unwrap();
    plan(
        rate,
        &["ramp"],
        [
            ("speech", source(rate, 4000, 512..4512)),
            ("ramp", retime("speech", 3200, 0..4000, pitch)),
        ],
    )
}

/// The canonical stretcher driven directly by the schedule derived here
/// from the ramp's definition `q(j) = L(2A·D·j + (B − A)·j²)/((A + B)·D²)`
/// with `L = 4000`, `D = 3200`, `A:B = 1:4`, never from the plan.
fn reference() -> Vec<[f32; 2]> {
    let (l, d) = (4000_i128, 3200_i128);
    let schedule = RampSchedule::new(l * 2 * d, l * 3, 5 * d * d, 3200).unwrap();
    let mut stream = StreamingStretch::new(StreamRecipe::ramp(4000, schedule).unwrap()).unwrap();
    let mut output = Vec::new();
    while stream.position() < 3200 {
        let count = (3200 - stream.position()).min(256) as usize;
        let range = stream.required_input(count).unwrap();
        let input: Vec<_> = range
            .clone()
            .map(|at| fixture_sample(512 + i64::from(at)))
            .collect();
        let left: Vec<_> = input.iter().map(|frame| frame[0]).collect();
        let right: Vec<_> = input.iter().map(|frame| frame[1]).collect();
        let (mut first, mut second) = ([0.0; 256], [0.0; 256]);
        stream
            .read(
                range.start,
                [&left, &right],
                [&mut first[..count], &mut second[..count]],
                &AtomicBool::new(false),
            )
            .unwrap();
        output.extend(
            first[..count]
                .iter()
                .zip(&second[..count])
                .map(|(&l, &r)| [l, r]),
        );
    }
    output
}

#[test]
fn a_preserve_ramp_stage_is_the_exact_schedule_for_any_reads_and_seeks() {
    let expected = reference();
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(ramp_plan(ramp(|ramp| PitchPolicy::PreserveRamp { ramp })));
    assert_eq!(renderer.plan().audio_duration().unwrap().0, 3200);
    // Consumer block sizes never change the preparation schedule.
    assert_eq!(
        read_all(&mut renderer, &mut provider, &[1, 73, 256, 3, 117]),
        expected
    );
    assert_eq!(renderer.cached_stage_count(), 1);
    // A cold seek prepares the whole stage canonically from its start.
    for (start, count) in [(3011, 189), (1600, 256), (0, 31)] {
        let mut cold = StageAudio::new(ramp_plan(ramp(|ramp| PitchPolicy::PreserveRamp { ramp })));
        assert_eq!(
            read_block(&mut cold, &mut provider, start, count).samples,
            expected[start as usize..(start + i64::from(count)) as usize]
        );
    }
    // The same clock with tape pitch is a different stage.
    let mut tape = StageAudio::new(ramp_plan(ramp(|ramp| PitchPolicy::TapeRamp { ramp })));
    assert_ne!(read_all(&mut tape, &mut provider, &[256]), expected);
}

#[test]
fn a_preserve_ramp_refuses_speeds_beyond_the_stretcher() {
    let rate = FrameRate::new(RATE.0, RATE.1).unwrap();
    // 1/2x to 16x is a valid tape ramp but not a pitch-preserving one.
    let fast = SpeedRamp::new(ratio(1, 2), ratio(16, 1)).unwrap();
    let nodes = |pitch| {
        [
            ("speech", source(rate, 4000, 512..4512)),
            ("ramp", retime("speech", 485, 0..4000, pitch)),
        ]
    };
    let document = |pitch| {
        let mut wire = serde_json::to_value(document_with_asset(
            rate,
            &["ramp"],
            nodes(PitchPolicy::TapeRamp { ramp: fast }),
            BTreeMap::new(),
            audio(0, 8197).span,
        ))
        .unwrap();
        wire["nodes"]["ramp"]["kind"]["pitch"] = serde_json::to_value(pitch).unwrap();
        ProjectDocument::from_json(&wire.to_string())
    };
    assert!(document(PitchPolicy::TapeRamp { ramp: fast }).is_ok());
    let error = document(PitchPolicy::PreserveRamp { ramp: fast }).unwrap_err();
    assert!(error.to_string().contains("1/8x to 8x"), "{error}");
}
