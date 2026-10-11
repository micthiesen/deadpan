use super::*;
use deadpan_audio::{PreparationActivity, PreparationMonitor};

struct Observed {
    source: FixtureProvider,
    monitor: Arc<PreparationMonitor>,
    activities: Vec<PreparationActivity>,
}

impl AudioSourceProvider for Observed {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        self.activities.push(self.monitor.activity());
        self.source.source(project, revision, asset, cancelled)
    }
}

#[test]
fn preparation_progress_reports_real_frames_without_changing_pcm_or_rebuilding_hits() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut repeated = hold(1);
    repeated.kind = NodeKind::Repeat {
        child: id("source"),
        iterations: IterationOrder::new(RevisionId::new("progress-plays").unwrap(), 12).unwrap(),
        gap: None,
        escalation: None,
    };
    let plan = plan(
        rate,
        &["stretch"],
        [
            ("source", source(rate, 8192, 0..8192)),
            ("repeat", repeated),
            (
                "stretch",
                retime("repeat", 196_608, 0..98_304, PitchPolicy::Preserve),
            ),
        ],
    );
    let mut expected = StageAudio::new(Arc::clone(&plan));
    let expected = read_block(&mut expected, &mut FixtureProvider::new(), 170_000, 200).samples;
    let monitor = Arc::new(PreparationMonitor::new(
        PreparationActivity::PreparingOutput,
    ));
    let mut source = Observed {
        source: FixtureProvider::new(),
        monitor: Arc::clone(&monitor),
        activities: Vec::new(),
    };
    let mut audio = StageAudio::with_limits(
        plan,
        StageLimits {
            maximum_memory_stage_frames: 0,
            ..Default::default()
        },
    )
    .unwrap();
    audio.set_preparation_monitor(Some(Arc::clone(&monitor)));
    let actual = audio
        .read(
            &mut source,
            AudioSample(170_000),
            200,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, expected);
    let completed: Vec<_> = source
        .activities
        .iter()
        .filter_map(|activity| match activity {
            PreparationActivity::Stretching {
                completed, total, ..
            } => {
                assert_eq!(*total, 196_608);
                assert!(*completed < *total);
                Some(*completed)
            }
            _ => None,
        })
        .collect();
    assert!(!completed.is_empty());
    assert!(completed.iter().any(|completed| *completed > 0));
    assert!(completed.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(monitor.activity(), PreparationActivity::PreparingOutput);
    source.activities.clear();
    let replacement = Arc::new(PreparationMonitor::new(PreparationActivity::CheckingPlan));
    source.monitor = Arc::clone(&replacement);
    audio.set_preparation_monitor(Some(Arc::clone(&replacement)));
    assert_eq!(
        audio
            .read(
                &mut source,
                AudioSample(170_000),
                200,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap()
            .samples,
        expected
    );
    assert!(
        !source.activities.is_empty(),
        "warm dependencies remain admitted"
    );
    assert!(
        source
            .activities
            .iter()
            .all(|activity| *activity == PreparationActivity::CheckingPlan)
    );
    assert_eq!(
        monitor.activity(),
        PreparationActivity::PreparingOutput,
        "new request never uses the old monitor"
    );
}

#[test]
fn cancelled_preparation_restores_activity_without_publishing_a_stage() {
    let monitor = Arc::new(PreparationMonitor::new(
        PreparationActivity::PreparingOutput,
    ));
    let mut source = Observed {
        source: FixtureProvider::new(),
        monitor: Arc::clone(&monitor),
        activities: Vec::new(),
    };
    source.source.cancel_on_call = true;
    let mut audio = StageAudio::new(cut_stage_plan());
    audio.set_preparation_monitor(Some(Arc::clone(&monitor)));
    assert!(
        audio
            .read(
                &mut source,
                AudioSample(5900),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(audio.cached_stage_count(), 0);
    assert_eq!(monitor.activity(), PreparationActivity::PreparingOutput);
    assert!(
        source
            .activities
            .iter()
            .any(|activity| matches!(activity, PreparationActivity::Stretching { .. }))
    );
}
