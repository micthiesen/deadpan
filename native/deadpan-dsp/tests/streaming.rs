use deadpan_dsp::{
    CanonicalRecipe, CanonicalStretch, DspError, MAX_STREAM_WINDOW_FRAMES, QUANTUM, StereoPcm,
    StreamRecipe, StreamingStretch, StretchRate,
};
use std::sync::atomic::AtomicBool;

fn fixture(at: u32) -> [f32; 2] {
    [
        ((at * 97 % 16384) as f32 - 8192.0) / 32768.0,
        ((at * 43 % 8192) as f32 - 4096.0) / 32768.0,
    ]
}

#[test]
fn large_logical_lengths_keep_fixed_input_windows() {
    for rate in [
        StretchRate::new(1, 8).unwrap(),
        StretchRate::new(8, 1).unwrap(),
    ] {
        let mut stream =
            StreamingStretch::new(StreamRecipe::new(u32::MAX, u32::MAX, rate, 24).unwrap())
                .unwrap();
        for iteration in 0..3 {
            let range = stream.required_input(256).unwrap();
            let count = (range.end - range.start) as usize;
            assert!(
                count
                    <= if iteration == 0 {
                        MAX_STREAM_WINDOW_FRAMES
                    } else {
                        2048
                    }
            );
            let input = vec![0.0; count];
            let (mut left, mut right) = ([1.0; 256], [2.0; 256]);
            assert_eq!(
                stream
                    .read(
                        range.start,
                        [&input, &input],
                        [&mut left, &mut right],
                        &AtomicBool::new(false)
                    )
                    .unwrap(),
                256
            );
            assert_eq!(left, [0.0; 256]);
            assert_eq!(right, [0.0; 256]);
        }
    }
}

#[test]
fn streamed_exact_schedule_matches_owned_pcm_for_rates_pitches_and_partitions() {
    for (numerator, denominator) in [(1, 8), (8, 1), (1, 1), (3, 2), (1000, 1001)] {
        for pitch in [-24, 0, 24] {
            let (input, output) = (10003, 13337);
            let rate = StretchRate::new(numerator, denominator).unwrap();
            let samples: Vec<_> = (0..input).map(fixture).collect();
            let pcm = StereoPcm::new(
                samples.iter().map(|s| s[0]).collect(),
                samples.iter().map(|s| s[1]).collect(),
            )
            .unwrap();
            let mut owned = CanonicalStretch::new(
                CanonicalRecipe::with_rate(input, output, rate, pitch).unwrap(),
                pcm,
            )
            .unwrap();
            let mut expected = Vec::new();
            let cancel = AtomicBool::new(false);
            while owned.position() < output {
                let (mut left, mut right) = ([0.0; QUANTUM], [0.0; QUANTUM]);
                let count = owned.read(&mut left, &mut right, &cancel).unwrap();
                expected.extend(
                    left[..count]
                        .iter()
                        .zip(&right[..count])
                        .map(|(&l, &r)| [l, r]),
                );
            }
            for pattern in [&[256][..], &[1, 17, 253, 127, 256][..]] {
                let mut streamed =
                    StreamingStretch::new(StreamRecipe::new(input, output, rate, pitch).unwrap())
                        .unwrap();
                let mut previous_end = 0;
                for &count in pattern.iter().cycle() {
                    let range = streamed.required_input(count).unwrap();
                    assert!((range.end - range.start) as usize <= MAX_STREAM_WINDOW_FRAMES);
                    if !range.is_empty() {
                        assert_eq!(range.start, previous_end);
                        previous_end = range.end;
                    }
                    let left: Vec<_> = range.clone().map(|at| fixture(at)[0]).collect();
                    let right: Vec<_> = range.clone().map(|at| fixture(at)[1]).collect();
                    let (mut first, mut second) = ([123.0; QUANTUM], [456.0; QUANTUM]);
                    let start = streamed.position() as usize;
                    let read = streamed
                        .read(
                            range.start,
                            [&left, &right],
                            [&mut first[..count], &mut second[..count]],
                            &cancel,
                        )
                        .unwrap();
                    for index in 0..read {
                        assert_eq!(
                            [first[index], second[index]],
                            expected[start + index],
                            "rate {rate:?} pitch {pitch} at {}",
                            start + index
                        );
                    }
                    assert!(first[read..].iter().all(|&v| v == 123.0));
                    assert!(second[read..].iter().all(|&v| v == 456.0));
                    if read == 0 {
                        break;
                    }
                }
            }
        }
    }
}

#[test]
fn rejected_or_cancelled_windows_leave_the_stream_reusable() {
    let mut stream = StreamingStretch::new(
        StreamRecipe::new(50003, 50003, StretchRate::new(1, 1).unwrap(), 0).unwrap(),
    )
    .unwrap();
    let range = stream.required_input(256).unwrap();
    let len = (range.end - range.start) as usize;
    let mut input = vec![0.0; len];
    let (mut left, mut right) = ([123.0; 256], [456.0; 256]);
    let cancel = AtomicBool::new(false);
    assert_eq!(
        stream.read(
            range.start + 1,
            [&input, &input],
            [&mut left, &mut right],
            &cancel
        ),
        Err(DspError::InputLength)
    );
    input[0] = f32::NAN;
    assert_eq!(
        stream.read(
            range.start,
            [&input, &input],
            [&mut left, &mut right],
            &cancel
        ),
        Err(DspError::NonFiniteInput)
    );
    input[0] = 17.0;
    assert_eq!(
        stream.read(
            range.start,
            [&input, &input],
            [&mut left, &mut right],
            &cancel
        ),
        Err(DspError::InputPeak)
    );
    input[0] = 0.0;
    assert_eq!(
        stream.read(
            range.start,
            [&input, &input],
            [&mut left, &mut right],
            &AtomicBool::new(true)
        ),
        Err(DspError::Cancelled)
    );
    assert_eq!(stream.position(), 0);
    assert_eq!(left, [123.0; 256]);
    assert_eq!(right, [456.0; 256]);
    assert_eq!(
        stream
            .read(
                range.start,
                [&input, &input],
                [&mut left, &mut right],
                &cancel
            )
            .unwrap(),
        256
    );
    assert_eq!(left, [0.0; 256]);
    assert_eq!(right, [0.0; 256]);
}
