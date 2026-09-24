use super::*;

fn run() -> Run {
    Run::new(
        7,
        3,
        ProjectId::new("project").unwrap(),
        RevisionId::new("revision").unwrap(),
        FrameRate::new(30_000, 1001).unwrap(),
        12,
        AudioSample(0),
    )
}

fn update(run: &Run, sample: i64) -> Update {
    Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
        phase: Phase::Playing,
        sample: Some(AudioSample(sample)),
        generation: run.generation,
        error: None,
    }
}

#[test]
fn pause_resume_keeps_subframe_sample_and_rejects_other_locations() {
    let mut run = run();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let mut current = update(&run, 1700);
    assert!(run.receive(&current).is_err());
    current.generation = Some(feed.restart(0).unwrap());
    let frame = run.receive(&current).unwrap().unwrap();
    assert_eq!(frame, 1);
    assert_eq!(
        run.rate.audio_boundary(ProjectFrame(frame as i64)).unwrap(),
        AudioSample(1602)
    );
    let resume = run.resume(frame);
    assert!(resume.matches(&current));
    current.ticket += 1;
    assert!(!resume.matches(&current));
    assert_eq!(
        resume.sample_for(run.session, &run.project, &run.revision, frame),
        Some(AudioSample(1700))
    );
    assert_eq!(
        resume.sample_for(run.session + 1, &run.project, &run.revision, frame),
        None
    );
    assert_eq!(
        resume.sample_for(
            run.session,
            &run.project,
            &RevisionId::new("new").unwrap(),
            frame
        ),
        None
    );
    assert_eq!(
        resume.sample_for(
            run.session,
            &ProjectId::new("new").unwrap(),
            &run.revision,
            frame
        ),
        None
    );
    assert_eq!(
        resume.sample_for(run.session, &run.project, &run.revision, frame + 1),
        None
    );
}

#[test]
fn ntsc_picture_changes_at_allocated_sample_boundary_once() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    for (sample, frame) in [
        (0, 0),
        (1601, 0),
        (1602, 1),
        (3202, 1),
        (3203, 2),
        (4804, 2),
        (4805, 3),
        (19219, 12),
    ] {
        assert_eq!(
            frame_at_sample(rate, 12, AudioSample(sample)).unwrap(),
            frame
        );
    }
    assert!(frame_at_sample(rate, 12, AudioSample(19220)).is_err());
    assert!(frame_at_sample(rate, 12, AudioSample(-1)).is_err());
    assert!(frame_at_sample(rate, -1, AudioSample(0)).is_err());
    assert_eq!(frame_at_sample(rate, 0, AudioSample(0)).unwrap(), 0);
}

#[test]
fn inverse_matches_every_sample_of_several_rational_clocks() {
    for (num, den) in [
        (24, 1),
        (24_000, 1001),
        (25, 1),
        (60_000, 1001),
        (192_000, 1),
    ] {
        let rate = FrameRate::new(num, den).unwrap();
        let boundaries: Vec<_> = (0..=17)
            .map(|frame| rate.audio_boundary(ProjectFrame(frame)).unwrap().0)
            .collect();
        for sample in 0..=*boundaries.last().unwrap() {
            let expected = boundaries
                .iter()
                .rposition(|boundary| *boundary <= sample)
                .unwrap() as u64;
            assert_eq!(
                frame_at_sample(rate, 17, AudioSample(sample)).unwrap(),
                expected
            );
        }
    }
    assert!(
        frame_at_sample(
            FrameRate::new(1, u32::MAX).unwrap(),
            i64::MAX,
            AudioSample(0)
        )
        .is_err()
    );
}

#[test]
fn stale_request_session_revision_project_and_generation_cannot_advance_cursor() {
    let mut run = run();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    run.generation = Some(feed.restart(0).unwrap());
    let current = update(&run, 1602);
    let mut variants = vec![current.clone(); 5];
    variants[0].ticket += 1;
    variants[1].session += 1;
    variants[2].project_id = ProjectId::new("other-project").unwrap();
    variants[3].revision_id = RevisionId::new("other-revision").unwrap();
    variants[4].generation = Some(feed.restart(0).unwrap());
    for stale in variants {
        assert_eq!(run.receive(&stale).unwrap(), None);
        assert_eq!(run.sample, AudioSample(0));
        assert_eq!(run.phase, Phase::Preparing);
    }
    assert_eq!(run.receive(&current).unwrap(), Some(1));
    assert_eq!(run.sample, AudioSample(1602));
    assert!(run.receive(&update(&run, 1601)).is_err());
    assert_eq!(run.sample, AudioSample(1602));
    assert!(run.receive(&update(&run, 99_999)).is_err());
}

#[test]
fn picture_work_coalesces_while_busy_and_terminal_updates_are_exact() {
    let mut run = run();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    assert_eq!(run.picture(0, false), None);
    let mut current = update(&run, 0);
    current.generation = Some(generation);
    run.receive(&current).unwrap();
    assert_eq!(run.picture(0, false), Some(generation));
    for frame in 1..10 {
        assert_eq!(run.picture(frame, true), None);
    }
    assert_eq!(run.picture(10, false), Some(generation));
    assert_eq!(run.picture(10, false), None);
    assert_eq!(run.picture(11, false), Some(generation));
    current.phase = Phase::Ended;
    assert!(run.receive(&current).is_err());
    current.sample = Some(AudioSample(19219));
    assert_eq!(run.receive(&current).unwrap(), Some(12));
    assert_eq!(run.picture(11, false), None);
}
