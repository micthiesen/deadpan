//! Rebuild current DSP while retaining an independently authored output clock.
use super::*;
use deadpan_audio::{DomainSignalTransfer, RootSignalBlock, SignalTransferError};
use deadpan_plan::AudioBoundDomain;

fn processed(input: &[[f32; 2]], output: u32, rate: u64, semitones: i32) -> Vec<[f32; 2]> {
    let recipe = CanonicalRecipe::with_rate(
        input.len() as u32,
        output,
        StretchRate::new(rate, 1).unwrap(),
        semitones,
    )
    .unwrap();
    let pcm = StereoPcm::new(
        input.iter().map(|sample| sample[0]).collect(),
        input.iter().map(|sample| sample[1]).collect(),
    )
    .unwrap();
    let mut dsp = CanonicalStretch::new(recipe, pcm).unwrap();
    let mut result = Vec::new();
    while result.len() < output as usize {
        let count = (output as usize - result.len()).min(256);
        let mut left = vec![0.; count];
        let mut right = vec![0.; count];
        assert_eq!(
            dsp.read(&mut left, &mut right, &AtomicBool::new(false))
                .unwrap(),
            count
        );
        result.extend(
            left.into_iter()
                .zip(right)
                .map(|(left, right)| [left, right]),
        );
    }
    result
}

fn transferred_reference(raw: &[[f32; 2]], quiet: i64, chunks: &[u32]) -> Recorded {
    let total = raw.len() as i64;
    let transfer = RootSignalTransfer::new(
        AudioSample(0)..AudioSample(total),
        ratio(1, 3),
        SignalSample(0),
        ExactRatio::ONE,
        SignalSample(0)..SignalSample(total),
    )
    .unwrap();
    let mut result = Recorded {
        samples: Vec::new(),
        suppressed: Vec::new(),
    };
    for count in chunks.iter().copied().cycle() {
        let start = result.samples.len() as i64;
        if start == total {
            break;
        }
        let count = count.min((total - start) as u32);
        let block = transfer
            .render::<SignalTransferError>(
                SignalSample(start),
                count,
                &AtomicBool::new(false),
                |at, count| {
                    let end = at.0 + i64::from(count);
                    Ok(RootSignalBlock {
                        start: at,
                        samples: raw[at.0 as usize..end as usize].to_vec(),
                        suppressed: if at.0 < quiet {
                            vec![at..AudioSample(end.min(quiet))]
                        } else {
                            vec![]
                        },
                    })
                },
            )
            .unwrap();
        result
            .suppressed
            .extend((start..start + i64::from(count)).map(|sample| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&SignalSample(sample)))
            }));
        result.samples.extend(block.samples);
    }
    result
}

fn raw_transferred(
    plan: &Arc<RenderPlan>,
    provider: &mut FixtureProvider,
    total: i64,
    chunks: &[u32],
) -> Recorded {
    provider
        .revisions
        .insert(plan.metadata().revision_id.clone());
    let query = plan
        .audio_signal()
        .query(
            SignalSample(0)..SignalSample(1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let AudioSignalContent::Bound(bound) = query.spans.into_iter().next().unwrap().content else {
        panic!("fractionally resumed processing output must retain its binding")
    };
    assert_eq!(bound.reference_at_offset(0).unwrap(), ratio(1, 3));
    assert_eq!(bound.reference_samples_per_output_sample(), ExactRatio::ONE);
    let AudioBoundDomain::Root(domain) = bound.raw_domain().unwrap() else {
        panic!("root output lattice")
    };
    assert_eq!(domain.root_samples(), AudioSample(0)..AudioSample(total));
    let transfer = DomainSignalTransfer::new(
        domain,
        ratio(1, 3),
        SignalSample(0),
        ExactRatio::ONE,
        SignalSample(0)..SignalSample(total),
    )
    .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut result = Recorded {
        samples: Vec::new(),
        suppressed: Vec::new(),
    };
    for count in chunks.iter().copied().cycle() {
        let start = result.samples.len() as i64;
        if start == total {
            break;
        }
        let count = count.min((total - start) as u32);
        let block = renderer
            .read_domain_transferred(
                provider,
                &transfer,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.stage, "physical_domain_on_point_grid_before_effects");
        assert_eq!(&block.revision_id, &plan.metadata().revision_id);
        result
            .suppressed
            .extend((start..start + i64::from(count)).map(|sample| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&SignalSample(sample)))
            }));
        result.samples.extend(block.samples);
    }
    result
}

fn fractional_output(pitch: PitchPolicy) {
    let original = fixture(pitch, 0, true);
    let mut wire = serde_json::to_value(&original).unwrap();
    // One third of an old output mix sample, expressed in local frames.
    wire["audio_bindings"]["bindings"]["processor"]["resume"] = serde_json::to_value(AudioResume {
        local_boundary: ExactRatio::ZERO,
        phase: AudioLocalPhase {
            constant: ratio(5, 24024),
            terms: vec![],
        },
    })
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = resize(&before, 3, "fractional-output-current-input");
    assert_eq!(
        after.audio_bindings().bindings()[&id("processor")],
        before.audio_bindings().bindings()[&id("processor")]
    );
    let (_, mut provider) = mono_original(2);
    let preserve = pitch == PitchPolicy::Preserve;
    let (prepared_count, root_count, rate, semitones) = if preserve {
        // Prepared output PointCeil(4*1601.6)=6407, physical root
        // RoundEven(4*1601.6)=6406. Never use one count for both domains.
        (6407, 6406, 2, 0)
    } else {
        (12813, 12813, 1, 3)
    };
    let mut references = Vec::new();
    for (document, hold_frames) in [(&before, 2), (&after, 3)] {
        // Construct the complete CURRENT operand independently. Changed DSP
        // neighborhoods are legitimate, so old processed PCM is no oracle.
        let input = direct_input_reference(&provider, 0, hold_frames);
        assert_eq!(input.samples.len(), 12813);
        let mut prepared = processed(&input.samples, prepared_count, rate, semitones);
        let quiet = match (preserve, hold_frames) {
            (true, 2) => 4004,
            (true, 3) => 4805,
            (false, 2) => 8008,
            (false, 3) => 9610,
            _ => unreachable!(),
        };
        // Canonical output policy is applied before retained output sampling.
        prepared[..quiet as usize].fill([0.; 2]);
        let raw = &prepared[..root_count as usize];
        let expected = transferred_reference(raw, quiet, &[251, 3]);
        same_recorded(
            &transferred_reference(raw, quiet, &[1, 127]),
            &expected,
            0,
            "independent output transfer chunks",
        );
        assert!(
            expected
                .samples
                .iter()
                .flatten()
                .any(|value| value.abs() > 0.01)
        );
        assert!(
            expected.suppressed[..quiet as usize]
                .iter()
                .all(|value| *value)
        );
        assert!(
            expected.suppressed[quiet as usize..]
                .iter()
                .all(|value| !value)
        );
        let plan = Arc::new(RenderPlan::compile(document).unwrap());
        let actual_raw = raw_transferred(&plan, &mut provider, root_count, &[127, 3, 251]);
        same_recorded(
            &actual_raw,
            &expected,
            0,
            &format!("{pitch:?} Hold{hold_frames} raw fractional output"),
        );
        // The consuming output's CURRENT silence has the same exact half-open
        // endpoint here. Check it independently of the raw transfer mask.
        let mut consuming = Recorded {
            samples: expected.samples.clone(),
            suppressed: expected.suppressed.clone(),
        };
        consuming.samples[..quiet as usize].fill([0.; 2]);
        consuming.suppressed[..quiet as usize].fill(true);
        same_recorded(
            &record(document, &mut provider, false, &[127, 3, 251]),
            &consuming,
            0,
            "current consuming output policy",
        );
        same_recorded(
            &record(document, &mut provider, false, &[1, 239]),
            &consuming,
            0,
            "current consuming output chunk parity",
        );
        references.push(consuming);
    }
    assert!(
        references[0].samples != references[1].samples,
        "current input must rebuild DSP"
    );
}

#[test]
fn ordinary_hold_growth_rebuilds_preserve_under_unchanged_fractional_output_binding() {
    fractional_output(PitchPolicy::Preserve);
}

#[test]
fn ordinary_hold_growth_rebuilds_shift_under_unchanged_fractional_output_binding() {
    fractional_output(PitchPolicy::Shift { semitones: 3 });
}

#[test]
fn moved_nested_processing_output_keeps_its_own_input_clock() {
    let mut inner = voice();
    let NodeKind::Source { source } = &mut inner.kind else {
        unreachable!()
    };
    source.duration = duration(2);
    let document = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["processor"],
        [
            (
                "processor",
                retime("body", 8, 0..8, PitchPolicy::Shift { semitones: 3 }),
            ),
            (
                "body",
                BeatNode::sequence(
                    "Outer input",
                    vec![id("lead"), id("pause"), id("nested"), id("suffix")],
                ),
            ),
            ("lead", hold(3)),
            ("pause", hold(2)),
            (
                "nested",
                retime("inner", 2, 0..2, PitchPolicy::Shift { semitones: 3 }),
            ),
            ("inner", inner),
            ("suffix", voice()),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("nested-processing-input-old").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = resize(&before, 3, "moved-nested-processing-output");
    assert_eq!(
        before.audio_bindings().bindings()[&id("inner")]
            .lattice
            .reference
            .root,
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("nested")
        }
    );
    assert_eq!(
        before.audio_bindings().bindings()[&id("nested")]
            .lattice
            .reference
            .root,
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("processor")
        }
    );
    assert_eq!(
        before.audio_bindings().bindings()[&id("inner")],
        after.audio_bindings().bindings()[&id("inner")],
        "outer movement must not move the intrinsic input"
    );
    assert_eq!(
        before.audio_bindings().bindings()[&id("processor")],
        after.audio_bindings().bindings()[&id("processor")]
    );
    let (_, mut provider) = mono_original(2);
    let native = finite_pcm(
        &provider,
        3100..6043,
        ratio(3100, 1),
        0,
        ratio(147, 160),
        3204,
    );
    let nested = processed(&native, 3204, 1, 3);
    // The nested processor's old PointCeil output allocation [8008,11212)
    // has3204 points. Its current [9610,12813) allocation has3203; its
    // intrinsic input still starts at its own point0, never at outer9610.
    assert_eq!(11212 - 8008, 3204);
    assert_eq!(12813 - 9610, 3203);
    let mut expected = Recorded {
        samples: vec![[0.; 2]; 12813],
        suppressed: vec![true; 12813],
    };
    expected.samples[9610..].copy_from_slice(&nested[..3203]);
    expected.suppressed[9610..].fill(false);
    let plan = Arc::new(RenderPlan::compile(&after).unwrap());
    same_recorded(
        &input_record(
            &plan,
            &mut provider,
            ratio(0, 1)..ratio(8, 1),
            0..12813,
            &[127, 3, 251],
        ),
        &expected,
        0,
        "moved opaque nested output",
    );
    same_recorded(
        &input_record(
            &plan,
            &mut provider,
            ratio(0, 1)..ratio(8, 1),
            0..12813,
            &[1, 239],
        ),
        &expected,
        0,
        "nested output chunk parity",
    );
}
