use super::*;

fn descriptor(start: i64, end: i64) -> EditWaveformDescriptor {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    EditWaveformDescriptor {
        stage: EditWaveformStage::AuthoredBusBeforeLimiter,
        project_id: ProjectId::new("edit-peaks").unwrap(),
        revision_id: RevisionId::new("revision").unwrap(),
        root: NodeId::new("root").unwrap(),
        frame_rate: rate,
        project_duration: FrameDuration::new(10).unwrap(),
        sample_rate: 48_000,
        grid: AudioSampleGrid::new(
            ExactRatio::ZERO,
            ExactRatio::new(5, 8008).unwrap(),
            AudioBoundaryRule::RoundEven,
        )
        .unwrap(),
        samples: AudioSample(start)..AudioSample(end),
        leaf_stride: 0,
    }
}

#[test]
fn absolute_ntsc_window_preserves_partial_terminal_bin_and_sample_grid() {
    let memory = WaveformMemory::default();
    let mut builder =
        EditWaveformBuilder::new(descriptor(1602, 2115), WaveformLimits::default(), &memory)
            .unwrap();
    builder
        .push(AudioSample(1602), &[[0.25, -0.5]; 256])
        .unwrap();
    builder
        .push(AudioSample(1858), &[[-3.0, 2.0]; 256])
        .unwrap();
    builder.push(AudioSample(2114), &[[9.0, -7.0]]).unwrap();
    let result = builder.finish(WaveformCompletion::Complete);
    let peaks = result.waveform;
    assert_eq!(result.examined_samples, 513);
    assert_eq!(peaks.measured_end(), AudioSample(2115));
    assert_eq!(
        peaks.bin_samples(0, 0),
        Some(AudioSample(1602)..AudioSample(1858))
    );
    assert_eq!(
        peaks.bin_samples(0, 2),
        Some(AudioSample(2114)..AudioSample(2115))
    );
    assert_eq!(
        peaks.bin_samples(2, 0),
        Some(AudioSample(1602)..AudioSample(2115))
    );
    assert_eq!(
        peaks.bin_project_frames(0, 0).unwrap(),
        ExactRatio::new(4005, 4004).unwrap()..ExactRatio::new(4645, 4004).unwrap()
    );
    assert_eq!(peaks.level(2).unwrap()[0].minimum(), [-3.0, -7.0]);
    assert_eq!(peaks.level(2).unwrap()[0].maximum(), [9.0, 2.0]);
    assert_eq!(
        peaks.descriptor().stage.label(),
        "authored_bus_pcm_before_limiter"
    );
}

#[test]
fn partial_and_invalid_blocks_do_not_publish_unknown_samples_or_zero_origin() {
    let memory = WaveformMemory::default();
    let mut builder =
        EditWaveformBuilder::new(descriptor(100, 869), WaveformLimits::default(), &memory).unwrap();
    builder.push(AudioSample(100), &[[2.0, -3.0]; 255]).unwrap();
    assert!(builder.push(AudioSample(0), &[[8.0; 2]]).is_err());
    assert!(
        builder
            .push(AudioSample(355), &[[8.0; 2], [f32::NAN; 2]])
            .is_err()
    );
    assert_eq!(builder.examined_samples(), 255);
    let unknown = builder.snapshot(&memory).unwrap();
    assert_eq!(unknown.measured_end(), AudioSample(100));
    assert!(unknown.level(0).unwrap().is_empty());
    builder.push(AudioSample(355), &[[1.0; 2]]).unwrap();
    let result = builder.finish(WaveformCompletion::Partial(WaveformStopReason::Cancelled));
    assert_eq!(result.waveform.measured_end(), AudioSample(356));
    assert_eq!(result.waveform.level(0).unwrap().len(), 1);
    assert!(result.waveform.level(1).unwrap().is_empty());
    assert_eq!(unknown.measured_end(), AudioSample(100));
}

#[test]
fn closed_geometry_and_shared_definition_ledger_keep_terminal_storage_available() {
    let memory = WaveformMemory::default();
    let definition = WaveformDescriptor {
        stage: WAVEFORM_STAGE,
        project_id: ProjectId::new("p").unwrap(),
        revision_id: RevisionId::new("r").unwrap(),
        definition: AudioDefinitionSelector::Node {
            node: NodeId::new("a").unwrap(),
        },
        root: NodeId::new("a").unwrap(),
        frame_rate: FrameRate::new(48_000, 1).unwrap(),
        owner_duration: FrameDuration::new(513).unwrap(),
        sample_rate: 48_000,
        grid: AudioSampleGrid::new(
            ExactRatio::ZERO,
            ExactRatio::ONE,
            AudioBoundaryRule::PointCeil,
        )
        .unwrap(),
        total_samples: SignalSample(513),
        leaf_stride: 0,
    };
    let old = WaveformBuilder::new(definition, WaveformLimits::default(), &memory).unwrap();
    let old_bytes = memory.resident_bytes();
    let builder =
        EditWaveformBuilder::new(descriptor(100, 613), WaveformLimits::default(), &memory).unwrap();
    assert!(memory.resident_bytes() > old_bytes);
    let peaks = builder
        .finish(WaveformCompletion::Partial(WaveformStopReason::OutputLimit))
        .waveform;
    let clone = peaks.clone();
    drop(peaks);
    drop(old);
    assert!(memory.resident_bytes() > 0);
    drop(clone);
    assert_eq!(memory.resident_bytes(), 0);
    for (start, end) in [(-1, 1), (2, 1), (0, 16017)] {
        assert!(
            EditWaveformBuilder::new(descriptor(start, end), WaveformLimits::default(), &memory)
                .is_err()
        );
    }
    let mut wrong = descriptor(0, 1);
    wrong.grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::new(5, 8008).unwrap(),
        AudioBoundaryRule::PointCeil,
    )
    .unwrap();
    assert!(EditWaveformBuilder::new(wrong, WaveformLimits::default(), &memory).is_err());
    assert_eq!(memory.resident_bytes(), 0);
    let empty =
        EditWaveformBuilder::new(descriptor(16016, 16016), WaveformLimits::default(), &memory)
            .unwrap()
            .finish(WaveformCompletion::Complete);
    assert_eq!(empty.waveform.measured_end(), AudioSample(16016));
    assert_eq!(empty.waveform.level_count(), 0);
    let denied = WaveformMemory::new(1).unwrap();
    assert!(
        EditWaveformBuilder::new(descriptor(0, 1), WaveformLimits::default(), &denied).is_err()
    );
    assert_eq!(denied.resident_bytes(), 0);
}
