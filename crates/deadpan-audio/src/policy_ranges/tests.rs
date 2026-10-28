use super::*;

#[test]
fn typed_signal_ranges_validate_all_before_numeric_writes_or_metadata_emission() {
    let ranges = [
        SignalSample(-4)..SignalSample(-2),
        SignalSample(-5)..SignalSample(-4),
    ];
    let mut pcm = [[0.25, -0.5]; 4];
    let before = pcm;
    assert!(matches!(
        apply_suppression(SignalSample(-4), &mut pcm, &ranges, |at| at.0),
        Err(StageAudioError::Range)
    ));
    assert_eq!(pcm, before);
    let mut visits = 0;
    let mut observations = 0;
    assert!(matches!(
        visit(
            SignalSample(-4),
            4,
            &ranges,
            |at| at.0,
            || {
                visits += 1;
                Ok(())
            },
            |_, _| {
                observations += 1;
                Ok(())
            },
        ),
        Err(StageAudioError::Range)
    ));
    assert_eq!(visits, 2, "the refused validation was visited");
    assert_eq!(observations, 0);
}

#[test]
fn negative_labels_duplicates_and_order_match_the_actual_final_signal_operation() {
    let ranges = [
        SignalSample(-3)..SignalSample(-1),
        SignalSample(-4)..SignalSample(-3),
        SignalSample(-3)..SignalSample(-1),
    ];
    let mut seen = Vec::new();
    let mut visits = 0;
    visit(
        SignalSample(-4),
        4,
        &ranges,
        |at| at.0,
        || {
            visits += 1;
            Ok(())
        },
        |range, offsets| {
            seen.push((range.clone(), offsets));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(visits, 6);
    assert_eq!(
        seen,
        vec![
            (ranges[0].clone(), 1..3),
            (ranges[1].clone(), 0..1),
            (ranges[2].clone(), 1..3),
        ]
    );
    let mut pcm = [[1.0, -0.0]; 4];
    apply_suppression(SignalSample(-4), &mut pcm, &ranges, |at| at.0).unwrap();
    assert_eq!(pcm[0..3], [[0.0; 2]; 3]);
    assert_eq!(
        pcm[3].map(f32::to_bits),
        [1.0_f32.to_bits(), (-0.0_f32).to_bits()]
    );
}

#[test]
fn refused_emission_keeps_validation_work_and_never_retries() {
    let ranges = [
        AudioSample(0)..AudioSample(1),
        AudioSample(1)..AudioSample(2),
    ];
    let mut visits = 0;
    let mut emitted = 0;
    let result = visit(
        AudioSample(0),
        2,
        &ranges,
        |at| at.0,
        || {
            visits += 1;
            Ok(())
        },
        |_, _| {
            emitted += 1;
            Err(StageAudioError::Range)
        },
    );
    assert!(matches!(result, Err(StageAudioError::Range)));
    assert_eq!(visits, 3);
    assert_eq!(emitted, 1);
}
