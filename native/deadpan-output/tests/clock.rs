use deadpan_output::{
    ClockError, ClockPosition, DELIVERY_CLOCK_INTERVALS, DeliveryClock, DeviceReport, RenderReport,
    RenderStatus, channel,
};

fn report(render: RenderReport, callback_ns: u64, playback_ns: u64) -> DeviceReport {
    DeviceReport {
        render,
        callback_ns,
        playback_ns,
        render_cost_ns: 0,
    }
}

#[test]
fn future_reports_preserve_current_coverage_and_never_extrapolate_silent_time() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(100).unwrap();
    // Authored silence is real submitted content and must advance the clock.
    feed.submit(generation, &[[0.0; 2]; 192]).unwrap();
    feed.activate(generation).unwrap();
    let mut clock = DeliveryClock::new(generation, 100, 292).unwrap();
    clock
        .observe(report(callback.render(&mut [0.0; 96]), 0, 1_000_000))
        .unwrap();
    clock
        .observe(report(
            callback.render(&mut [0.0; 96]),
            1_000_000,
            2_000_000,
        ))
        .unwrap();
    assert_eq!(clock.position(999_999), ClockPosition::Pending);
    // The newest report is still in the future; the previous one owns now.
    assert_eq!(
        clock.position(1_500_000),
        ClockPosition::Content { sample: 124 }
    );
    assert_eq!(
        clock.position(2_500_000),
        ClockPosition::Content { sample: 172 }
    );
    assert_eq!(
        clock.position(3_000_000),
        ClockPosition::Gap {
            last_sample: Some(195)
        }
    );
    assert_eq!(
        clock.position(u64::MAX),
        ClockPosition::Gap {
            last_sample: Some(195)
        }
    );
}

#[test]
fn ended_and_starved_prefixes_finish_at_playback_deadline_not_report_arrival() {
    for status in [RenderStatus::Ended, RenderStatus::Starved] {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(17).unwrap();
        feed.submit(generation, &[[0.25, -0.25]; 3]).unwrap();
        let end = if status == RenderStatus::Ended {
            feed.finish(generation).unwrap();
            20
        } else {
            100
        };
        feed.activate(generation).unwrap();
        let rendered = callback.render(&mut [0.0; 16]);
        assert_eq!(rendered.status, status);
        assert_eq!(rendered.rendered_frames, 3);
        assert_eq!(rendered.silent_frames, 5);
        let report = report(rendered, 10, 100);
        let mut clock = DeliveryClock::new(generation, 17, end).unwrap();
        clock.observe(report).unwrap();
        assert_eq!(clock.position(10), ClockPosition::Pending);
        assert_eq!(clock.position(100), ClockPosition::Content { sample: 17 });
        assert_eq!(
            clock.position(20_934),
            ClockPosition::Content { sample: 18 }
        );
        assert_eq!(
            clock.position(62_599),
            ClockPosition::Content { sample: 19 }
        );
        assert_eq!(report.sample_at(62_600), None);
        let terminal = ClockPosition::Terminal {
            status,
            end_sample: 20,
        };
        assert_eq!(clock.position(62_600), terminal);
        // A repeated terminal callback must not postpone the first deadline.
        clock
            .observe(super_report(callback.render(&mut [0.0; 16]), 200_000))
            .unwrap();
        assert_eq!(clock.position(62_600), terminal);
    }
}

fn super_report(render: RenderReport, playback_ns: u64) -> DeviceReport {
    report(render, playback_ns, playback_ns)
}

#[test]
fn exact_buffer_end_and_empty_eos_have_explicit_exclusive_terminal_positions() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(0).unwrap();
    feed.submit(generation, &[[0.1; 2]; 48]).unwrap();
    feed.finish(generation).unwrap();
    feed.activate(generation).unwrap();
    let mut clock = DeliveryClock::new(generation, 0, 48).unwrap();
    clock
        .observe(super_report(callback.render(&mut [0.0; 96]), 1_000_000))
        .unwrap();
    clock
        .observe(super_report(callback.render(&mut [0.0; 96]), 2_000_000))
        .unwrap();
    assert_eq!(
        clock.position(1_999_999),
        ClockPosition::Content { sample: 47 }
    );
    assert_eq!(
        clock.position(2_000_000),
        ClockPosition::Terminal {
            status: RenderStatus::Ended,
            end_sample: 48
        }
    );

    let generation = feed.restart(48).unwrap();
    feed.finish(generation).unwrap();
    feed.activate(generation).unwrap();
    let mut empty = DeliveryClock::new(generation, 48, 48).unwrap();
    empty
        .observe(super_report(callback.render(&mut [0.0; 16]), 3_000_000))
        .unwrap();
    assert_eq!(empty.position(2_999_999), ClockPosition::Pending);
    assert_eq!(
        empty.position(3_000_000),
        ClockPosition::Terminal {
            status: RenderStatus::Ended,
            end_sample: 48
        }
    );
}

#[test]
fn empty_terminal_jitter_cannot_end_the_previous_submitted_prefix_early() {
    for status in [RenderStatus::Ended, RenderStatus::Starved] {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(0).unwrap();
        for _ in 0..2 {
            feed.submit(generation, &[[0.1; 2]; 256]).unwrap();
        }
        let end = if status == RenderStatus::Ended {
            feed.finish(generation).unwrap();
            512
        } else {
            1024
        };
        feed.activate(generation).unwrap();
        let mut clock = DeliveryClock::new(generation, 0, end).unwrap();
        let prefix = callback.render(&mut [0.0; 1024]);
        assert_eq!(prefix.rendered_frames, 512);
        assert_eq!(prefix.status, RenderStatus::Playing);
        clock.observe(super_report(prefix, 0)).unwrap();

        // A valid sub-sample overlap puts this empty terminal report before
        // the 512-frame prefix's independently reported end at 10,666,667 ns.
        let terminal = callback.render(&mut [0.0; 1024]);
        assert_eq!(terminal.rendered_frames, 0);
        assert_eq!(terminal.status, status);
        clock.observe(super_report(terminal, 10_650_000)).unwrap();
        for now in [10_650_000, 10_666_666] {
            assert_eq!(clock.position(now), ClockPosition::Content { sample: 511 });
        }
        let expected = ClockPosition::Terminal {
            status,
            end_sample: 512,
        };
        assert_eq!(clock.position(10_666_667), expected);

        // Later terminal callbacks must retain that first conservative end.
        clock
            .observe(super_report(callback.render(&mut [0.0; 1024]), 21_316_667))
            .unwrap();
        assert_eq!(
            clock.position(10_666_666),
            ClockPosition::Content { sample: 511 }
        );
        assert_eq!(clock.position(10_666_667), expected);
    }
}

#[test]
fn invalid_reports_leave_clock_unchanged_and_cannot_bridge_missing_content() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(0).unwrap();
    feed.submit(generation, &[[0.1; 2]; 96]).unwrap();
    feed.activate(generation).unwrap();
    let first = super_report(callback.render(&mut [0.0; 96]), 1_000_000);
    let second = super_report(callback.render(&mut [0.0; 96]), 2_000_000);
    let mut clock = DeliveryClock::new(generation, 0, 96).unwrap();
    clock.observe(first).unwrap();

    let mut invalid = second;
    invalid.render.first_sample = Some(49);
    assert_eq!(
        clock.observe(invalid),
        Err(ClockError::DiscontinuousContent {
            expected: 48,
            actual: 49
        })
    );
    invalid = second;
    invalid.callback_ns = 999_999;
    assert_eq!(
        clock.observe(invalid),
        Err(ClockError::NonMonotonicTimestamp)
    );
    invalid = second;
    invalid.callback_ns = 1_000_001;
    invalid.playback_ns = 1_000_001;
    assert_eq!(clock.observe(invalid), Err(ClockError::OverlappingReports));
    invalid = second;
    invalid.render.rendered_frames = 49;
    assert_eq!(clock.observe(invalid), Err(ClockError::BeyondEnd));
    invalid = second;
    invalid.render.rendered_frames = 1;
    invalid.render.status = RenderStatus::Ended;
    assert_eq!(clock.observe(invalid), Err(ClockError::PrematureEnd));
    invalid = second;
    invalid.render.first_sample = None;
    assert_eq!(clock.observe(invalid), Err(ClockError::InvalidReport));
    let (mut foreign, _) = channel().unwrap();
    invalid = second;
    invalid.render.generation = foreign.restart(0).unwrap();
    assert_eq!(clock.observe(invalid), Err(ClockError::ForeignGeneration));
    invalid = second;
    invalid.render.status = RenderStatus::Fault;
    assert_eq!(clock.observe(invalid), Err(ClockError::OutputFault));
    assert_eq!(
        clock.position(1_500_000),
        ClockPosition::Content { sample: 24 }
    );
    assert_eq!(
        clock.position(2_500_000),
        ClockPosition::Gap {
            last_sample: Some(47)
        }
    );
    clock.observe(second).unwrap();
    assert_eq!(
        clock.position(2_500_000),
        ClockPosition::Content { sample: 72 }
    );
}

#[test]
fn fixed_history_exposes_evicted_coverage_as_a_gap() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(0).unwrap();
    feed.submit(generation, &[[0.1; 2]; 128]).unwrap();
    feed.activate(generation).unwrap();
    let mut clock = DeliveryClock::new(generation, 0, 128).unwrap();
    for index in 0..DELIVERY_CLOCK_INTERVALS + 2 {
        clock
            .observe(super_report(
                callback.render(&mut [0.0; 2]),
                index as u64 * 1_000_000,
            ))
            .unwrap();
    }
    assert_eq!(clock.position(0), ClockPosition::Gap { last_sample: None });
    assert_eq!(
        clock.position(2_000_000),
        ClockPosition::Content { sample: 2 }
    );
    assert_eq!(
        clock.position(2_500_000),
        ClockPosition::Gap {
            last_sample: Some(2)
        }
    );
    assert_eq!(
        clock.position(65_000_000),
        ClockPosition::Content { sample: 65 }
    );
}

#[test]
fn rational_sample_deadlines_allow_nanosecond_timestamp_quantization() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(0).unwrap();
    feed.submit(generation, &[[0.1; 2]; 2]).unwrap();
    feed.activate(generation).unwrap();
    let mut clock = DeliveryClock::new(generation, 0, 2).unwrap();
    clock
        .observe(super_report(callback.render(&mut [0.0; 2]), 100))
        .unwrap();
    clock
        .observe(super_report(callback.render(&mut [0.0; 2]), 20_933))
        .unwrap();
    assert_eq!(clock.position(20_932), ClockPosition::Content { sample: 0 });
    assert_eq!(clock.position(20_933), ClockPosition::Content { sample: 1 });
    assert_eq!(
        clock.position(41_767),
        ClockPosition::Gap {
            last_sample: Some(1)
        }
    );
}

#[test]
fn sub_sample_host_jitter_keeps_reported_overlap_and_gap_explicit() {
    for second_start in [1_999_000, 2_001_000] {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(0).unwrap();
        feed.submit(generation, &[[0.1; 2]; 96]).unwrap();
        feed.activate(generation).unwrap();
        let mut clock = DeliveryClock::new(generation, 0, 96).unwrap();
        clock
            .observe(super_report(callback.render(&mut [0.0; 96]), 1_000_000))
            .unwrap();
        clock
            .observe(super_report(callback.render(&mut [0.0; 96]), second_start))
            .unwrap();
        assert_eq!(
            clock.position(second_start),
            ClockPosition::Content { sample: 48 }
        );
        if second_start > 2_000_000 {
            assert_eq!(
                clock.position(2_000_500),
                ClockPosition::Gap {
                    last_sample: Some(47)
                }
            );
        }
    }
}

#[test]
fn retained_device_reports_preserve_contiguous_samples_despite_host_timestamp_jitter() {
    // Replay previously qualified telemetry, not a live device. Its measured
    // sub-sample overlap exposed an overly strict nominal-period assertion.
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/audio-qualification/evidence/2026-09-21-output");
    for name in [
        "hardware-probe-1.json",
        "hardware-probe-2.json",
        "hardware-probe-3.json",
    ] {
        let evidence: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(directory.join(name)).unwrap()).unwrap();
        let records = evidence["records"].as_array().unwrap();
        let retained_generation = records[0]["generation"].as_u64().unwrap();
        let (mut feed, _) = channel().unwrap();
        let generation = feed.restart(0).unwrap();
        let mut clock = DeliveryClock::new(generation, 0, 96_000).unwrap();
        let mut content = 0;
        for record in records
            .iter()
            .take_while(|record| record["generation"].as_u64() == Some(retained_generation))
        {
            let status = match record["status"].as_str().unwrap() {
                "Paused" => RenderStatus::Paused,
                "Playing" => RenderStatus::Playing,
                "Ended" => RenderStatus::Ended,
                other => panic!("unexpected steady qualification status {other}"),
            };
            let first = record["first_sample"].as_i64();
            let frames = usize::try_from(record["rendered_frames"].as_u64().unwrap()).unwrap();
            let report = DeviceReport {
                render: RenderReport {
                    generation,
                    status,
                    first_sample: first,
                    rendered_frames: frames,
                    silent_frames: usize::try_from(record["silent_frames"].as_u64().unwrap())
                        .unwrap(),
                    discarded_packets: usize::try_from(
                        record["discarded_packets"].as_u64().unwrap(),
                    )
                    .unwrap(),
                },
                callback_ns: record["callback_ns"].as_u64().unwrap(),
                playback_ns: record["playback_ns"].as_u64().unwrap(),
                render_cost_ns: 0,
            };
            clock
                .observe(report)
                .unwrap_or_else(|error| panic!("{name}: {error:?} at {record}"));
            if let Some(sample) = first {
                assert_eq!(
                    clock.position(report.playback_ns),
                    ClockPosition::Content { sample }
                );
            }
            content += frames;
        }
        assert_eq!(content, 96_000);
        assert_eq!(
            clock.position(u64::MAX),
            ClockPosition::Terminal {
                status: RenderStatus::Ended,
                end_sample: 96_000
            }
        );
    }
}

#[test]
fn range_and_deadline_overflow_are_explicit() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(0).unwrap();
    assert!(matches!(
        DeliveryClock::new(generation, -1, 0),
        Err(ClockError::InvalidRange)
    ));
    assert!(matches!(
        DeliveryClock::new(generation, 1, 0),
        Err(ClockError::InvalidRange)
    ));
    feed.submit(generation, &[[0.1; 2]]).unwrap();
    feed.activate(generation).unwrap();
    let mut clock = DeliveryClock::new(generation, 0, 1).unwrap();
    let report = super_report(callback.render(&mut [0.0; 2]), u64::MAX);
    assert_eq!(clock.observe(report), Err(ClockError::TimestampOverflow));
    assert_eq!(clock.position(0), ClockPosition::Pending);
}
