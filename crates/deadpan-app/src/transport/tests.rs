use super::*;

mod original;
mod sound;

fn identity() -> Identity {
    Identity {
        ticket: 7,
        session: 3,
        project: ProjectId::new("project").unwrap(),
        revision: RevisionId::new("revision").unwrap(),
    }
}

fn sequence_window(start: i64, end: i64, looping: bool) -> Run {
    let domain = Domain::Sequence {
        rate: FrameRate::new(30, 1).unwrap(),
        frames: 60,
    };
    let start = domain.sample_at_boundary(start as u64).unwrap();
    let end = domain.sample_at_boundary(end as u64).unwrap();
    Run::with_domain(
        identity(),
        domain,
        Window::new(start, end, looping).unwrap(),
        start,
    )
    .unwrap()
}

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
        run.domain().sample_at_boundary(frame).unwrap(),
        AudioSample(1602)
    );
    let resume = run.resume(frame);
    assert!(resume.matches(&current));
    current.ticket += 1;
    assert!(!resume.matches(&current));
    current.ticket -= 1;
    current.generation = Some(feed.restart(0).unwrap());
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
    assert_eq!(
        run.position().unwrap(),
        Position {
            cursor: 12,
            picture: 11
        }
    );
    assert_eq!(run.picture(11, false), None);
}

#[test]
fn a_window_ends_at_its_own_boundary_and_keeps_out_out_of_the_picture() {
    let mut run = sequence_window(10, 24, false);
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    run.generation = Some(feed.restart(run.sample.0).unwrap());
    let mut terminal = update(&run, run.window().end().0 - 1);
    terminal.phase = Phase::Ended;
    assert!(run.receive(&terminal).is_err());
    assert_eq!(
        run.position().unwrap(),
        Position {
            cursor: 10,
            picture: 10
        }
    );
    terminal.sample = Some(run.window().end());
    assert_eq!(run.receive(&terminal).unwrap(), Some(24));
    assert_eq!(
        run.position().unwrap(),
        Position {
            cursor: 24,
            picture: 23
        }
    );
    assert_eq!(run.picture_frame().unwrap(), 23);
    assert_eq!(run.content_sample().unwrap(), AudioSample(38_400));
    assert_eq!(run.lap().unwrap(), 0);
    terminal.sample = None;
    assert!(run.receive(&terminal).is_err());
}

#[test]
fn loop_laps_map_content_backwards_without_accepting_backwards_delivery() {
    let mut run = sequence_window(10, 24, true);
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    run.generation = Some(feed.restart(run.sample.0).unwrap());
    let start = run.window().start().0;
    let end = run.window().end().0;
    let length = end - start;
    assert_eq!(run.receive(&update(&run, end - 1)).unwrap(), Some(23));
    assert_eq!(run.lap().unwrap(), 0);
    assert_eq!(run.receive(&update(&run, end)).unwrap(), Some(10));
    assert_eq!(run.content_sample().unwrap(), AudioSample(start));
    assert_eq!(run.lap().unwrap(), 1);
    assert!(run.receive(&update(&run, start)).is_err());
    assert_eq!(run.sample, AudioSample(end));
    assert_eq!(
        run.receive(&update(&run, end + length + 199)).unwrap(),
        Some(10)
    );
    assert_eq!(run.lap().unwrap(), 2);
    assert_eq!(run.content_sample().unwrap(), AudioSample(start + 199));

    let resume = run.resume(10);
    assert_eq!(
        resume.sample_for_domain(
            run.session,
            &run.project,
            &run.revision,
            run.domain(),
            run.window(),
            10,
        ),
        Some(AudioSample(end + length + 199))
    );
    let resumed =
        Run::with_domain(identity(), run.domain().clone(), *run.window(), run.sample).unwrap();
    assert_eq!(resumed.position().unwrap(), run.position().unwrap());
    assert_eq!(resumed.lap().unwrap(), 2);
    assert_eq!(
        resume.sample_for(run.session, &run.project, &run.revision, 10),
        None
    );

    let mut terminal = update(&run, end + length + 199);
    terminal.phase = Phase::Ended;
    assert!(run.receive(&terminal).unwrap_err().contains("unexpectedly"));
    assert_eq!(run.phase, Phase::Playing);
    // The final representable delivery sample still maps exactly. Any attempt
    // to wrap the delivery integer is rejected instead of starting a new lap.
    assert!(run.receive(&update(&run, i64::MAX)).is_ok());
    assert_eq!(run.lap().unwrap(), ((i64::MAX - start) / length) as u64);
    assert_eq!(
        run.content_sample().unwrap(),
        AudioSample(start + (i64::MAX - start) % length)
    );
    assert!(run.receive(&update(&run, i64::MIN)).is_err());
    assert_eq!(run.sample, AudioSample(i64::MAX));
}

#[test]
fn resume_checks_window_domain_rate_duration_and_actual_position() {
    let mut run = sequence_window(10, 24, false);
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    run.generation = Some(feed.restart(run.sample.0).unwrap());
    run.receive(&update(&run, 16_099)).unwrap();
    let resume = run.resume(10);
    let sample = |domain: &Domain, window: &Window, frame| {
        resume.sample_for_domain(
            run.session,
            &run.project,
            &run.revision,
            domain,
            window,
            frame,
        )
    };
    assert_eq!(
        sample(run.domain(), run.window(), 10),
        Some(AudioSample(16_099))
    );
    for window in [
        Window::new(AudioSample(16_000), AudioSample(38_400), true).unwrap(),
        Window::new(AudioSample(0), AudioSample(38_400), false).unwrap(),
        Window::new(AudioSample(16_000), AudioSample(40_000), false).unwrap(),
    ] {
        assert_eq!(sample(run.domain(), &window, 10), None);
    }
    for domain in [
        Domain::Sequence {
            rate: FrameRate::new(24, 1).unwrap(),
            frames: 60,
        },
        Domain::Sequence {
            rate: FrameRate::new(30, 1).unwrap(),
            frames: 61,
        },
    ] {
        assert_eq!(sample(&domain, run.window(), 10), None);
    }
    assert_eq!(sample(run.domain(), run.window(), 11), None);
    assert_eq!(
        run.resume(11).sample_for_domain(
            run.session,
            &run.project,
            &run.revision,
            run.domain(),
            run.window(),
            11,
        ),
        None
    );
}

#[test]
fn invalid_windows_and_domain_bounds_fail_before_starting_transport() {
    let domain = Domain::Sequence {
        rate: FrameRate::new(30, 1).unwrap(),
        frames: 12,
    };
    for (start, end, delivery) in [(0, 0, 0), (0, 20_000, 0), (1, 100, 0), (1, 100, 101)] {
        let window = Window::new(AudioSample(start), AudioSample(end), false).unwrap();
        assert!(
            Run::with_domain(identity(), domain.clone(), window, AudioSample(delivery)).is_err()
        );
    }
    for domain in [
        Domain::Sequence {
            rate: FrameRate::new(30, 1).unwrap(),
            frames: -1,
        },
        Domain::Sequence {
            rate: FrameRate::new(1, u32::MAX).unwrap(),
            frames: i64::MAX,
        },
    ] {
        let window = Window::new(AudioSample(0), AudioSample(1), false).unwrap();
        assert!(Run::with_domain(identity(), domain, window, AudioSample(0)).is_err());
    }
    assert!(domain.sample_at_boundary(u64::MAX).is_err());
    assert!(domain.sample_at_boundary(13).is_err());
}
