use super::*;
use deadpan_plan::AudioBoundaryRule;

fn descriptor(samples: i64) -> WaveformDescriptor {
    WaveformDescriptor {
        stage: WAVEFORM_STAGE,
        project_id: ProjectId::new("waveform").unwrap(),
        revision_id: RevisionId::new("revision").unwrap(),
        definition: AudioDefinitionSelector::Node {
            node: NodeId::new("source").unwrap(),
        },
        root: NodeId::new("source").unwrap(),
        frame_rate: FrameRate::new(48_000, 1).unwrap(),
        owner_duration: FrameDuration::new(samples).unwrap(),
        sample_rate: 48_000,
        grid: AudioSampleGrid::new(
            ExactRatio::ZERO,
            ExactRatio::ONE,
            AudioBoundaryRule::PointCeil,
        )
        .unwrap(),
        total_samples: SignalSample(samples),
        leaf_stride: 0,
    }
}

fn builder(samples: i64, memory: &WaveformMemory) -> WaveformBuilder {
    WaveformBuilder::new(descriptor(samples), WaveformLimits::default(), memory).unwrap()
}

fn push(builder: &mut WaveformBuilder, samples: &[[f32; 2]], pieces: &[usize]) {
    let mut start = 0;
    for &count in pieces.iter().cycle() {
        if start == samples.len() {
            break;
        }
        let end = (start + count).min(samples.len());
        builder
            .push(
                SignalSample(i64::try_from(start).unwrap()),
                &samples[start..end],
            )
            .unwrap();
        start = end;
    }
}

#[test]
fn signed_stereo_extrema_keep_polarity_boundary_impulses_and_over_range_values() {
    let mut samples = vec![[0.25, -0.5]; 513];
    samples[255] = [-7.0, 0.75];
    samples[256] = [5.0, -3.0];
    samples[511] = [-9.0, 2.0];
    samples[512] = [18.0, -19.0];
    let memory = WaveformMemory::default();
    let mut builder = builder(513, &memory);
    push(&mut builder, &samples, &[256]);
    let result = builder.finish(WaveformCompletion::Complete).waveform;
    let leaves = result.level(0).unwrap();
    assert_eq!(leaves.len(), 3);
    assert_eq!(leaves[0].minimum(), [-7.0, -0.5]);
    assert_eq!(leaves[0].maximum(), [0.25, 0.75]);
    assert_eq!(leaves[1].minimum(), [-9.0, -3.0]);
    assert_eq!(leaves[1].maximum(), [5.0, 2.0]);
    assert_eq!(leaves[2].minimum(), [18.0, -19.0]);
    assert_eq!(leaves[2].maximum(), [18.0, -19.0]);
    assert_eq!(result.level(2).unwrap()[0].minimum(), [-9.0, -19.0]);
    assert_eq!(result.level(2).unwrap()[0].maximum(), [18.0, 2.0]);
    assert_eq!(
        result.bin_samples(0, 2),
        Some(SignalSample(512)..SignalSample(513))
    );
    assert_eq!(
        result.bin_samples(1, 1),
        Some(SignalSample(512)..SignalSample(513))
    );
    assert_eq!(
        result.bin_samples(2, 0),
        Some(SignalSample(0)..SignalSample(513))
    );
}

#[test]
fn irregular_blocks_and_signed_zero_produce_identical_pyramids() {
    let samples: Vec<_> = (0_u16..2049)
        .map(|i| {
            if i % 127 == 0 {
                [-0.0, 0.0]
            } else if i % 127 == 1 {
                [0.0, -0.0]
            } else {
                [f32::from(i % 97) - 45.0, 3.0 - f32::from(i % 13)]
            }
        })
        .collect();
    let memory = WaveformMemory::default();
    let mut whole = builder(2049, &memory);
    let mut irregular = builder(2049, &memory);
    push(&mut whole, &samples, &[256]);
    push(&mut irregular, &samples, &[1, 29, 7, 253]);
    let whole = whole.finish(WaveformCompletion::Complete).waveform;
    let irregular = irregular.finish(WaveformCompletion::Complete).waveform;
    for level in 0..whole.level_count() {
        let bits = |waveform: &DefinitionWaveform| {
            waveform
                .level(level)
                .unwrap()
                .iter()
                .map(|peak| {
                    (
                        peak.minimum().map(f32::to_bits),
                        peak.maximum().map(f32::to_bits),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&whole), bits(&irregular));
    }
}

#[test]
fn partial_leaf_and_unpaired_prefix_never_claim_parent_coverage() {
    let memory = WaveformMemory::default();
    let mut builder = builder(769, &memory);
    builder.push(SignalSample(0), &[[0.0; 2]; 255]).unwrap();
    let unknown = builder.snapshot(&memory).unwrap();
    assert_eq!(unknown.measured_end(), SignalSample(0));
    assert!(unknown.level(0).unwrap().is_empty());
    builder.push(SignalSample(255), &[[0.0; 2]]).unwrap();
    let first = builder.snapshot(&memory).unwrap();
    assert_eq!(first.measured_end(), SignalSample(256));
    assert_eq!(first.level(0).unwrap().len(), 1);
    assert!(first.level(1).unwrap().is_empty());
    builder.push(SignalSample(256), &[[1.0; 2]; 256]).unwrap();
    builder.push(SignalSample(512), &[[2.0; 2]; 256]).unwrap();
    let prefix = builder.snapshot(&memory).unwrap();
    assert_eq!(prefix.level(0).unwrap().len(), 3);
    assert_eq!(prefix.level(1).unwrap().len(), 1);
    assert!(prefix.level(2).unwrap().is_empty());
    assert_eq!(
        prefix.bin_samples(1, 0),
        Some(SignalSample(0)..SignalSample(512))
    );
    builder.push(SignalSample(768), &[[3.0; 2]]).unwrap();
    let complete = builder.finish(WaveformCompletion::Complete).waveform;
    assert_eq!(complete.level(2).unwrap().len(), 1);
    assert_eq!(
        complete.bin_samples(2, 0),
        Some(SignalSample(0)..SignalSample(769))
    );
    assert_eq!(
        prefix.measured_end(),
        SignalSample(768),
        "published results are immutable"
    );
}

#[test]
fn invalid_block_is_atomic_even_when_its_good_prefix_would_finish_a_leaf() {
    let memory = WaveformMemory::default();
    let mut builder = builder(513, &memory);
    builder.push(SignalSample(0), &[[2.0, -3.0]; 255]).unwrap();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            builder.push(SignalSample(255), &[[99.0; 2], [invalid; 2]]),
            Err(WaveformError::InvalidSamples)
        ));
        assert_eq!(builder.examined_samples(), 255);
        assert_eq!(builder.measured_end(), SignalSample(0));
    }
    assert!(matches!(
        builder.push(SignalSample(254), &[[1.0; 2]]),
        Err(WaveformError::InvalidSequence)
    ));
    assert!(matches!(
        builder.push(SignalSample(255), &[]),
        Err(WaveformError::InvalidSamples)
    ));
    builder.push(SignalSample(255), &[[1.0, -1.0]]).unwrap();
    let result = builder.finish(WaveformCompletion::Partial(WaveformStopReason::Cancelled));
    assert_eq!(result.examined_samples, 256);
    assert_eq!(result.waveform.level(0).unwrap()[0].maximum(), [2.0, -1.0]);
}

#[test]
fn ntsc_terminal_frame_clamps_only_the_final_exact_sample_boundary() {
    let memory = WaveformMemory::default();
    let mut descriptor = descriptor(1602);
    descriptor.frame_rate = FrameRate::new(30_000, 1001).unwrap();
    descriptor.owner_duration = FrameDuration::new(1).unwrap();
    descriptor.grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::new(5, 8008).unwrap(),
        AudioBoundaryRule::PointCeil,
    )
    .unwrap();
    let mut builder = WaveformBuilder::new(descriptor, WaveformLimits::default(), &memory).unwrap();
    push(&mut builder, &[[1.0; 2]; 1602], &[197]);
    let result = builder.finish(WaveformCompletion::Complete).waveform;
    assert_eq!(
        result.bin_owner_frames(0, 0).unwrap(),
        ExactRatio::ZERO..ExactRatio::new(160, 1001).unwrap()
    );
    assert_eq!(
        result.bin_owner_frames(0, 6).unwrap(),
        ExactRatio::new(960, 1001).unwrap()..ExactRatio::ONE
    );
    assert!(
        result
            .descriptor()
            .grid
            .at(SignalSample(1602))
            .unwrap()
            .compare_integer(1)
            .is_gt()
    );
    assert_eq!(
        result.bin_samples(0, 6),
        Some(SignalSample(1536)..SignalSample(1602))
    );
}

#[test]
fn empty_and_huge_extents_have_bounded_geometry_without_eager_sample_work() {
    let memory = WaveformMemory::default();
    let empty = builder(0, &memory).finish(WaveformCompletion::Complete);
    assert_eq!(empty.waveform.level_count(), 0);
    assert_eq!(empty.waveform.measured_end(), SignalSample(0));
    let huge = builder(i64::MAX, &memory);
    assert_eq!(huge.peaks.data.levels[0].capacity, 4096);
    assert_eq!(huge.peaks.data.level_count(), 13);
    assert!(huge.peaks.data.peaks.len() < 8192);
    assert!(huge.descriptor.value.leaf_stride.is_power_of_two());
    assert_eq!(huge.examined_samples(), 0);
    assert!(memory.resident_bytes() < MAX_WAVEFORM_BYTES);
}

#[test]
fn aggregate_reservations_survive_all_arc_clones_and_release_after_final_drop() {
    let sizing = WaveformMemory::default();
    let reference = builder(513, &sizing);
    let builder_bytes = sizing.resident_bytes();
    let snapshot = reference.snapshot(&sizing).unwrap();
    let one_snapshot = sizing.resident_bytes() - builder_bytes;
    drop(snapshot);
    drop(reference);
    assert_eq!(sizing.resident_bytes(), 0);
    let memory = WaveformMemory::new(builder_bytes + one_snapshot).unwrap();
    let mut builder = builder(513, &memory);
    builder.push(SignalSample(0), &[[1.0; 2]; 256]).unwrap();
    let snapshot = builder.snapshot(&memory).unwrap();
    assert_eq!(memory.resident_bytes(), memory.maximum_bytes());
    assert!(matches!(
        builder.snapshot(&memory),
        Err(WaveformError::MemoryLimit)
    ));
    let clone = Arc::clone(&snapshot);
    drop(snapshot);
    assert_eq!(memory.resident_bytes(), memory.maximum_bytes());
    let terminal = builder.finish(WaveformCompletion::Partial(WaveformStopReason::OutputLimit));
    assert_eq!(terminal.waveform.measured_end(), SignalSample(256));
    assert_eq!(
        memory.resident_bytes(),
        memory.maximum_bytes(),
        "finalization needs no additional peak storage"
    );
    drop(clone);
    assert_eq!(memory.resident_bytes(), builder_bytes);
    drop(terminal);
    assert_eq!(memory.resident_bytes(), 0);
}

#[test]
fn failed_admission_releases_its_descriptor_and_all_limits_are_closed() {
    let memory = WaveformMemory::new(1).unwrap();
    assert!(matches!(
        WaveformBuilder::new(descriptor(4096), WaveformLimits::default(), &memory),
        Err(WaveformError::MemoryLimit)
    ));
    assert_eq!(memory.resident_bytes(), 0);
    for (samples, leaves, timeout) in [
        (0, 1, Duration::from_secs(1)),
        (MAX_WAVEFORM_SAMPLES + 1, 1, Duration::from_secs(1)),
        (1, 0, Duration::from_secs(1)),
        (1, MAX_WAVEFORM_LEAVES + 1, Duration::from_secs(1)),
        (1, 1, Duration::ZERO),
        (1, 1, Duration::from_secs(21)),
    ] {
        assert!(WaveformLimits::new(samples, leaves, timeout).is_err());
    }
    assert!(WaveformMemory::new(0).is_err());
    assert!(WaveformMemory::new(MAX_WAVEFORM_BYTES + 1).is_err());
}
