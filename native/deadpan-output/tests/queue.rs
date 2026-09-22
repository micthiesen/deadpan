use std::sync::mpsc;
use std::thread;

use deadpan_output::{
    FeedError, MAX_CALLBACK_FRAMES, PACKET_FRAMES, QUEUE_PACKETS, RenderStatus, channel,
};

fn frame(at: usize) -> [f32; 2] {
    let value = (at % 511) as f32 / 512.0;
    [value, -value]
}

#[test]
fn consumer_partitions_preserve_samples_positions_and_explicit_end() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(900).unwrap();
    let expected: Vec<_> = (0..777).map(frame).collect();
    for chunk in expected.chunks(PACKET_FRAMES) {
        feed.submit(generation, chunk).unwrap();
    }
    feed.finish(generation).unwrap();
    feed.activate(generation).unwrap();
    let mut rendered = Vec::new();
    for count in [1, 73, 301, 17, 511] {
        let mut output = vec![9.0; count * 2];
        let report = callback.render(&mut output);
        assert_eq!(report.generation, generation);
        assert_eq!(report.first_sample, Some(900 + rendered.len() as i64));
        rendered.extend(
            output[..report.rendered_frames * 2]
                .chunks_exact(2)
                .map(|pair| [pair[0], pair[1]]),
        );
        assert!(
            output[report.rendered_frames * 2..]
                .iter()
                .all(|sample| *sample == 0.0)
        );
        assert_eq!(report.rendered_frames + report.silent_frames, count);
        if report.status == RenderStatus::Ended {
            break;
        }
    }
    assert_eq!(rendered, expected);
    let mut output = [1.0; 14];
    let report = callback.render(&mut output);
    assert_eq!(report.status, RenderStatus::Ended);
    assert_eq!(report.first_sample, None);
    assert_eq!(report.rendered_frames, 0);
    assert_eq!(output, [0.0; 14]);
    assert_eq!(feed.submit(generation, &[[0.1; 2]]), Err(FeedError::Ended));
}

#[test]
fn seek_discards_partial_and_queued_old_pcm_without_replaying_either() {
    let (mut feed, mut callback) = channel().unwrap();
    let old = feed.restart(100).unwrap();
    feed.submit(old, &[[0.25, -0.25]; 20]).unwrap();
    feed.submit(old, &[[0.5, -0.5]; 20]).unwrap();
    feed.activate(old).unwrap();
    assert_eq!(callback.render(&mut [0.0; 6]).first_sample, Some(100));
    let current = feed.restart(5000).unwrap();
    assert!(current.get() > old.get());
    feed.submit(current, &[[0.75, -0.75]; 4]).unwrap();
    feed.finish(current).unwrap();
    feed.activate(current).unwrap();
    let mut output = [1.0; 12];
    let report = callback.render(&mut output);
    assert_eq!(report.generation, current);
    assert_eq!(report.status, RenderStatus::Ended);
    assert_eq!(report.first_sample, Some(5000));
    assert_eq!(report.rendered_frames, 4);
    assert_eq!(report.discarded_packets, 2);
    assert_eq!(
        &output[..8],
        &[0.75, -0.75, 0.75, -0.75, 0.75, -0.75, 0.75, -0.75]
    );
    assert_eq!(&output[8..], &[0.0; 4]);
}

#[test]
fn admission_failures_leave_cursor_unchanged_and_full_end_can_be_retried() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(7).unwrap();
    for _ in 0..QUEUE_PACKETS {
        feed.submit(generation, &[[0.1; 2]]).unwrap();
    }
    let next = feed.next_sample();
    assert_eq!(feed.submit(generation, &[[0.2; 2]]), Err(FeedError::Full));
    assert_eq!(feed.finish(generation), Err(FeedError::Full));
    assert_eq!(feed.next_sample(), next);
    feed.activate(generation).unwrap();
    let first = callback.render(&mut [0.0; 2]);
    assert_eq!(first.first_sample, Some(7));
    feed.finish(generation).unwrap();
    let mut output = [0.0; QUEUE_PACKETS * 2];
    let report = callback.render(&mut output);
    assert_eq!(report.status, RenderStatus::Ended);
    assert_eq!(report.rendered_frames, QUEUE_PACKETS - 1);
    assert_eq!(feed.next_sample(), next);
}

#[test]
fn stale_cleanup_is_bounded_and_does_not_latch_starvation_before_matching_pcm() {
    let (mut feed, mut callback) = channel().unwrap();
    let old = feed.restart(0).unwrap();
    feed.submit(old, &[[0.1; 2]; 2]).unwrap();
    feed.activate(old).unwrap();
    callback.render(&mut [0.0; 2]); // one old frame remains pending
    for _ in 0..QUEUE_PACKETS {
        feed.submit(old, &[[0.2; 2]]).unwrap();
    }
    let new = feed.restart(1000).unwrap();
    let mut output = [7.0; 4];
    let report = callback.render(&mut output);
    assert_eq!(report.status, RenderStatus::Paused);
    assert_eq!(report.discarded_packets, QUEUE_PACKETS);
    assert_eq!(report.rendered_frames, 0);
    assert_eq!(output, [0.0; 4]);
    feed.submit(new, &[[0.8, -0.8]; 2]).unwrap();
    feed.activate(new).unwrap();
    let report = callback.render(&mut output);
    assert_eq!(report.status, RenderStatus::Playing);
    assert_eq!(report.discarded_packets, 1);
    assert_eq!(report.first_sample, Some(1000));
    assert_eq!(output, [0.8, -0.8, 0.8, -0.8]);
}

#[test]
fn starvation_stays_silent_after_late_refill_until_a_new_generation() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(0).unwrap();
    feed.submit(generation, &[[0.2, -0.2]; 2]).unwrap();
    feed.activate(generation).unwrap();
    let mut output = [1.0; 8];
    let report = callback.render(&mut output);
    assert_eq!(report.status, RenderStatus::Starved);
    assert_eq!(report.rendered_frames, 2);
    assert_eq!(&output[..4], &[0.2, -0.2, 0.2, -0.2]);
    assert_eq!(&output[4..], &[0.0; 4]);
    feed.submit(generation, &[[0.4; 2]; 4]).unwrap();
    assert_eq!(callback.render(&mut output).status, RenderStatus::Starved);
    assert_eq!(feed.activate(generation), Err(FeedError::AlreadyActive));
    assert_eq!(output, [0.0; 8]);
    let next = feed.restart(200).unwrap();
    feed.submit(next, &[[0.7, -0.7]; 4]).unwrap();
    feed.activate(next).unwrap();
    let report = callback.render(&mut output);
    assert_eq!(report.status, RenderStatus::Playing);
    assert_eq!(report.first_sample, Some(200));
    assert_eq!(report.discarded_packets, 1);
    assert_eq!(output, [0.7, -0.7, 0.7, -0.7, 0.7, -0.7, 0.7, -0.7]);
}

#[test]
fn pause_consumes_identity_and_only_explicit_restart_can_resume() {
    let (mut feed, mut callback) = channel().unwrap();
    let initial = feed.generation();
    assert_eq!(feed.submit(initial, &[[0.1; 2]]), Err(FeedError::Paused));
    assert_eq!(callback.render(&mut [1.0; 2]).status, RenderStatus::Paused);
    let active = feed.restart(8).unwrap();
    feed.submit(active, &[[0.3; 2]; 2]).unwrap();
    feed.activate(active).unwrap();
    let paused = feed.pause().unwrap();
    assert!(paused.get() > active.get());
    assert_eq!(
        feed.submit(active, &[[0.2; 2]]),
        Err(FeedError::StaleGeneration)
    );
    assert_eq!(feed.submit(paused, &[[0.2; 2]]), Err(FeedError::Paused));
    assert_eq!(feed.finish(paused), Err(FeedError::Paused));
    let mut output = [1.0; 2];
    let report = callback.render(&mut output);
    assert_eq!(report.status, RenderStatus::Paused);
    assert_eq!(report.discarded_packets, 1);
    assert_eq!(output, [0.0; 2]);
    let active = feed.restart(11).unwrap();
    feed.submit(active, &[[1.0, -1.0]]).unwrap();
    feed.activate(active).unwrap();
    assert_eq!(callback.render(&mut output).first_sample, Some(11));
    assert_eq!(output, [1.0, -1.0]);
}

#[test]
fn invalid_samples_packet_lengths_and_sample_overflow_never_advance_cursor() {
    let (mut feed, mut callback) = channel().unwrap();
    assert_eq!(feed.restart(-1), Err(FeedError::InvalidStart));
    assert_eq!(feed.generation().get(), 0);
    let generation = feed.restart(50).unwrap();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0001, -1.0001] {
        assert_eq!(
            feed.submit(generation, &[[0.0, invalid]]),
            Err(FeedError::InvalidSamples)
        );
    }
    assert_eq!(feed.submit(generation, &[]), Err(FeedError::InvalidFrames));
    assert_eq!(
        feed.submit(generation, &[[0.0; 2]; PACKET_FRAMES + 1]),
        Err(FeedError::InvalidFrames)
    );
    assert_eq!(feed.next_sample(), 50);
    feed.finish(generation).unwrap();
    feed.activate(generation).unwrap();
    assert_eq!(callback.render(&mut [1.0; 2]).status, RenderStatus::Ended);
    let generation = feed.restart(i64::MAX).unwrap();
    assert_eq!(
        feed.submit(generation, &[[0.5; 2]]),
        Err(FeedError::SampleOverflow)
    );
    assert_eq!(feed.next_sample(), i64::MAX);
    feed.finish(generation).unwrap();
    feed.activate(generation).unwrap();
    assert_eq!(callback.render(&mut [1.0; 2]).status, RenderStatus::Ended);
}

#[test]
fn device_fault_and_invalid_callback_shapes_are_permanent() {
    for size in [3, (MAX_CALLBACK_FRAMES + 1) * 2] {
        let (mut feed, mut callback) = channel().unwrap();
        let generation = feed.restart(0).unwrap();
        feed.submit(generation, &[[0.4; 2]; 3]).unwrap();
        feed.activate(generation).unwrap();
        let mut output = vec![1.0; size];
        let report = callback.render(&mut output);
        assert_eq!(report.status, RenderStatus::Fault);
        assert_eq!(report.rendered_frames, 0);
        assert!(output.iter().all(|sample| *sample == 0.0));
        assert_eq!(feed.restart(2), Err(FeedError::Fault));
    }
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(10).unwrap();
    feed.submit(generation, &[[0.2; 2]; 3]).unwrap();
    feed.activate(generation).unwrap();
    let signal = feed.fault_signal();
    let clone = signal.clone();
    clone.raise();
    let mut output = [1.0; 6];
    assert_eq!(callback.render(&mut output).status, RenderStatus::Fault);
    assert_eq!(output, [0.0; 6]);
    assert!(signal.is_faulted());
    assert_eq!(feed.restart(0), Err(FeedError::Fault));
    assert_eq!(feed.pause(), Err(FeedError::Fault));
    assert_eq!(feed.submit(generation, &[[0.0; 2]]), Err(FeedError::Fault));
}

#[test]
fn producer_and_consumer_threads_preserve_many_generation_boundaries() {
    let (mut feed, mut callback) = channel().unwrap();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let producer = thread::spawn(move || {
        for iteration in 0..200 {
            let start = iteration * 1000;
            let generation = feed.restart(start).unwrap();
            let samples: Vec<_> = (0..511).map(frame).collect();
            for packet in samples.chunks(PACKET_FRAMES) {
                feed.submit(generation, packet).unwrap();
            }
            feed.finish(generation).unwrap();
            feed.activate(generation).unwrap();
            ready_tx.send((generation, start)).unwrap();
            done_rx.recv().unwrap();
        }
    });
    for _ in 0..200 {
        let (generation, start) = ready_rx.recv().unwrap();
        let mut collected = Vec::new();
        loop {
            let mut output = [0.0; 194];
            let report = callback.render(&mut output);
            assert_eq!(report.generation, generation);
            assert_eq!(report.first_sample, Some(start + collected.len() as i64));
            collected.extend(
                output[..report.rendered_frames * 2]
                    .chunks_exact(2)
                    .map(|pair| [pair[0], pair[1]]),
            );
            if report.status == RenderStatus::Ended {
                break;
            }
            assert_eq!(report.status, RenderStatus::Playing);
        }
        assert_eq!(collected, (0..511).map(frame).collect::<Vec<_>>());
        done_tx.send(()).unwrap();
    }
    producer.join().unwrap();
}

#[test]
fn preparation_callbacks_cannot_starve_before_prefill_and_activation() {
    let (mut feed, mut callback) = channel().unwrap();
    let generation = feed.restart(700).unwrap();
    let mut output = [1.0; 10];
    for _ in 0..3 {
        assert_eq!(callback.render(&mut output).status, RenderStatus::Paused);
        assert_eq!(output, [0.0; 10]);
    }
    assert_eq!(feed.activate(generation), Err(FeedError::EmptyPreparation));
    feed.submit(generation, &[[0.6, -0.6]; 4]).unwrap();
    assert_eq!(callback.render(&mut output).status, RenderStatus::Paused);
    assert_eq!(output, [0.0; 10]);
    feed.finish(generation).unwrap();
    feed.activate(generation).unwrap();
    let report = callback.render(&mut output);
    assert_eq!(report.status, RenderStatus::Ended);
    assert_eq!(report.first_sample, Some(700));
    assert_eq!(report.rendered_frames, 4);
    assert_eq!(&output[..8], &[0.6, -0.6, 0.6, -0.6, 0.6, -0.6, 0.6, -0.6]);
    assert_eq!(&output[8..], &[0.0; 2]);
    let empty = feed.restart(900).unwrap();
    feed.finish(empty).unwrap();
    assert_eq!(callback.render(&mut output).status, RenderStatus::Paused);
    feed.activate(empty).unwrap();
    assert_eq!(callback.render(&mut output).status, RenderStatus::Ended);
    assert_eq!(output, [0.0; 10]);
}

#[test]
fn foreign_channel_token_is_stale_even_with_the_same_serial() {
    let (mut first, _) = channel().unwrap();
    let (mut second, _) = channel().unwrap();
    let foreign = first.restart(0).unwrap();
    let own = second.restart(0).unwrap();
    assert_eq!(foreign.get(), own.get());
    assert_ne!(foreign.channel_id(), own.channel_id());
    assert_ne!(foreign, own);
    assert_eq!(
        second.submit(foreign, &[[0.2; 2]]),
        Err(FeedError::StaleGeneration)
    );
    assert_eq!(second.finish(foreign), Err(FeedError::StaleGeneration));
    assert_eq!(second.activate(foreign), Err(FeedError::StaleGeneration));
    assert_eq!(second.next_sample(), 0);
    second.submit(own, &[[0.2; 2]]).unwrap();
}

#[test]
fn matching_pcm_still_renders_after_the_full_stale_cleanup_budget() {
    let (mut feed, mut callback) = channel().unwrap();
    let old = feed.restart(0).unwrap();
    feed.submit(old, &[[0.1; 2]; 2]).unwrap();
    feed.activate(old).unwrap();
    callback.render(&mut [0.0; 2]);
    for _ in 0..QUEUE_PACKETS - 1 {
        feed.submit(old, &[[0.2; 2]]).unwrap();
    }
    let new = feed.restart(500).unwrap();
    feed.submit(new, &[[0.9, -0.9]]).unwrap();
    feed.activate(new).unwrap();
    let mut output = [0.0; 2];
    let report = callback.render(&mut output);
    assert_eq!(report.discarded_packets, QUEUE_PACKETS);
    assert_eq!(report.status, RenderStatus::Playing);
    assert_eq!(report.first_sample, Some(500));
    assert_eq!(output, [0.9, -0.9]);
}
