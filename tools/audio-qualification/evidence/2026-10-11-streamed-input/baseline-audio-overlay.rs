
mod streamed_witness {
use super::*;
const INPUT: u32 = 704 * 8192;
const OUTPUT: u32 = INPUT / 2;
fn long_plan() -> Arc<RenderPlan> {
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
                    i64::from(OUTPUT),
                    0..i64::from(INPUT),
                    PitchPolicy::Preserve,
                ),
            ),
        ],
    )
}


#[test]
fn streamed_input_supports_two_minutes_before_a_late_first_read() {
    let mut renderer = StageAudio::new(long_plan());
    let mut provider = FixtureProvider::new();
    let result = renderer.read(&mut provider, AudioSample(i64::from(OUTPUT - 1)),
        1, Duration::from_secs(60), &AtomicBool::new(false));
    assert!(result.is_ok(), "{result:?}");
}
}
