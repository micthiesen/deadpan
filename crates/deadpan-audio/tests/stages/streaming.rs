//! Long continuous Preserve history without retaining its complete input PCM.
use super::*;
use deadpan_dsp::{StreamRecipe, StreamingStretch};

const INPUT: u32 = 704 * 8192; // More than two minutes at 48 kHz.
const OUTPUT: u32 = INPUT / 2;
const RESIDENT: u32 = OUTPUT + 2 * deadpan_dsp::MAX_STREAM_WINDOW_FRAMES as u32 + 1024;

fn long_plan() -> Arc<RenderPlan> {
    long_plan_with_output(OUTPUT)
}

fn long_plan_with_output(output: u32) -> Arc<RenderPlan> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut repeated = hold(1);
    repeated.kind = NodeKind::Repeat {
        child: id("source"),
        iterations: IterationOrder::new(RevisionId::new("long-plays").unwrap(), 704).unwrap(),
        gap: None,
        escalation: None,
    };
    plan(
        rate,
        &["stretch"],
        [
            ("source", source(rate, 8192, 0..8192)),
            ("repeat", repeated),
            (
                "stretch",
                retime(
                    "repeat",
                    i64::from(output),
                    0..i64::from(INPUT),
                    PitchPolicy::Preserve,
                ),
            ),
        ],
    )
}

fn reference() -> Vec<[f32; 2]> {
    reference_with_output(OUTPUT)
}

fn reference_with_output(output: u32) -> Vec<[f32; 2]> {
    let mut engine = StreamingStretch::new(
        StreamRecipe::new(
            INPUT,
            output,
            StretchRate::new(u64::from(INPUT), u64::from(output)).unwrap(),
            0,
        )
        .unwrap(),
    )
    .unwrap();
    let mut samples = Vec::new();
    while engine.position() < output {
        let range = engine.required_input(256).unwrap();
        // Independent clock and exact integer WAV values, without a plan query.
        let left: Vec<_> = range
            .clone()
            .map(|at| fixture_sample(i64::from(at % 8192))[0])
            .collect();
        let right: Vec<_> = range
            .clone()
            .map(|at| fixture_sample(i64::from(at % 8192))[1])
            .collect();
        let (mut first, mut second) = ([0.0; 256], [0.0; 256]);
        let count = engine
            .read(
                range.start,
                [&left, &right],
                [&mut first, &mut second],
                &AtomicBool::new(false),
            )
            .unwrap();
        samples.extend((0..count).map(|n| [first[n], second[n]]));
    }
    samples
}

#[test]
fn four_minute_disk_output_matches_canonical_history_under_fixed_memory() {
    let output = INPUT * 2;
    let expected = reference_with_output(output);
    let plan = long_plan_with_output(output);
    let metadata_bound = |plan: &RenderPlan, output: u32| {
        let processing = plan
            .audio_processing(
                AudioSample(0)..AudioSample(i64::from(output)),
                Default::default(),
            )
            .unwrap();
        assert_eq!(processing.spans.len(), 1);
        let deadpan_plan::AudioSignalContent::Stage(stage) = &processing.spans[0].content else {
            panic!("the fixture must exercise one actual Preserve stage");
        };
        let input = stage.input_signal();
        assert_eq!(
            input.sample_count().unwrap(),
            SignalSample(i64::from(INPUT))
        );
        let query = input
            .query(
                SignalSample(0)..SignalSample(i64::from(INPUT)),
                Default::default(),
            )
            .unwrap();
        assert!(query.spans.len() > 1);
        assert!(query.spans.iter().all(|span| matches!(
            span.content,
            deadpan_plan::AudioSignalContent::Leaf(deadpan_plan::AudioContent::Source { .. })
        )));
        drop(query);
        drop(input);
        drop(processing);
        let mut bound = super::flattened_metadata::Observation {
            retained_frames: 0,
            peak_frames: 0,
        };
        let mut observe = |start: u32, count: u32| {
            let observed = super::flattened_metadata::root(
                plan,
                AudioSample(i64::from(start))..AudioSample(i64::from(start + count)),
            );
            bound.retained_frames = bound.retained_frames.max(observed.retained_frames);
            bound.peak_frames = bound.peak_frames.max(observed.peak_frames);
        };
        observe(output - 173, 173);
        observe(output - 73, 73);
        for start in (0..output).step_by(256) {
            observe(start, (output - start).min(256));
        }
        bound
    };
    let metadata = metadata_bound(&plan, output);
    let short = long_plan();
    assert_eq!(
        metadata,
        metadata_bound(&short, OUTPUT),
        "the measured per-request flat bound is the same for short and four-minute histories"
    );
    let pcm_frames = 2 * deadpan_dsp::MAX_STREAM_WINDOW_FRAMES as u32 + 2048;
    // Leaf-only stage input cannot create nested Original carriers. Adding the
    // full measured construction peak bounds the retained outer query as well.
    let resident_bound = pcm_frames
        .checked_add(u32::try_from(metadata.peak_frames).unwrap())
        .unwrap();
    assert!(resident_bound < INPUT && resident_bound < output);
    let limits = StageLimits {
        maximum_resident_frames: resident_bound,
        maximum_disk_bytes: u64::from(output) * 8,
        ..Default::default()
    };
    let mut pcm_only = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: pcm_frames,
            ..limits
        },
    )
    .unwrap();
    let mut refused_provider = FixtureProvider::new();
    assert!(matches!(
        pcm_only.read(
            &mut refused_provider,
            AudioSample(i64::from(output - 173)),
            173,
            Duration::from_secs(60),
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(refused_provider.calls, 0);
    assert_eq!(pcm_only.cached_stage_count(), 0);
    let mut disk = StageAudio::with_limits(Arc::clone(&plan), limits).unwrap();
    let mut provider = FixtureProvider::new();
    assert_eq!(
        read(&mut disk, &mut provider, output - 173, 173),
        expected[expected.len() - 173..]
    );
    assert_eq!(disk.cached_disk_bytes(), u64::from(output) * 8);
    assert_eq!(disk.cached_resident_frames(), 0);
    for start in (0..output).step_by(256) {
        let count = (output - start).min(256);
        assert_eq!(
            read(&mut disk, &mut provider, start, count),
            expected[start as usize..(start + count) as usize]
        );
    }
    // A cold controller recreates identical history and the terminal samples.
    let mut cold = StageAudio::with_limits(plan, limits).unwrap();
    assert_eq!(
        read(&mut cold, &mut provider, output - 73, 73),
        expected[expected.len() - 73..]
    );
}

fn read(
    renderer: &mut StageAudio,
    provider: &mut impl AudioSourceProvider,
    start: u32,
    count: u32,
) -> Vec<[f32; 2]> {
    renderer
        .read(
            provider,
            AudioSample(i64::from(start)),
            count,
            Duration::from_secs(60),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

#[test]
fn two_minute_preserve_streams_under_a_budget_smaller_than_its_input() {
    const { assert!(RESIDENT < INPUT) };
    let plan = long_plan();
    let expected = reference();
    let limits = StageLimits {
        maximum_resident_frames: RESIDENT,
        ..Default::default()
    };
    let mut renderer = StageAudio::with_limits(Arc::clone(&plan), limits).unwrap();
    let mut provider = FixtureProvider::new();
    // A late first request must prepare the full continuous history once.
    for (start, count) in [
        (OUTPUT - 257, 256),
        (1, 193),
        (OUTPUT / 2 + 17, 127),
        (0, 256),
    ] {
        assert_eq!(
            read(&mut renderer, &mut provider, start, count),
            expected[start as usize..(start + count) as usize]
        );
    }
    assert_eq!(renderer.cached_stage_count(), 1);
    for start in (0..OUTPUT).step_by(256) {
        let count = (OUTPUT - start).min(256);
        assert_eq!(
            read(&mut renderer, &mut provider, start, count),
            expected[start as usize..(start + count) as usize]
        );
    }
    let mut cold = StageAudio::with_limits(plan, limits).unwrap();
    assert_eq!(
        read(&mut cold, &mut provider, OUTPUT - 73, 73),
        expected[expected.len() - 73..]
    );
}

#[test]
fn streamed_work_limit_still_admits_the_whole_input_before_media() {
    let mut renderer = StageAudio::with_limits(
        long_plan(),
        StageLimits {
            maximum_prepared_frames: u64::from(INPUT + OUTPUT - 1),
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    assert!(matches!(
        renderer.read(
            &mut provider,
            AudioSample(i64::from(OUTPUT - 1)),
            1,
            Duration::from_secs(60),
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("prepared frames per read"))
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(renderer.cached_stage_count(), 0);
}
