use super::*;

#[path = "gain/slip.rs"]
mod slip;

fn gain(value: i32) -> GainDb {
    GainDb::new(value).unwrap()
}
fn gain_range(start: ExactRatio, end: ExactRatio) -> GainRange {
    GainRange::new(start, end).unwrap()
}
fn treatment(trim: i32, envelopes: Vec<GainEnvelope>, ranges: Vec<GainRange>) -> AudioTreatments {
    AudioTreatments::from_clip_gain(ClipGain::new(gain(trim), false, envelopes, ranges).unwrap())
}
fn step_gain(start: ExactRatio, end: ExactRatio, value: i32) -> GainEnvelope {
    GainEnvelope::new(
        GainClock::OwnerOutput,
        gain_range(start, end),
        gain(value),
        vec![GainSegment::new(end, gain(value), GainCurve::Step).unwrap()],
    )
    .unwrap()
}

fn source_gain() -> AudioTreatments {
    treatment(
        3000,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                gain_range(ExactRatio::ZERO, ratio(1280, 8008)),
                gain(-6000),
                vec![GainSegment::new(ratio(1280, 8008), gain(6000), GainCurve::Linear).unwrap()],
            )
            .unwrap(),
        ],
        vec![gain_range(ratio(517, 8008), ratio(597, 8008))],
    )
}

fn with_controls(document: &ProjectDocument, start: i64) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["a"]["audio_treatments"] = serde_json::to_value(source_gain()).unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(treatment(
        -3000,
        vec![step_gain(
            ratio(i128::from(start + 128) * 5, 8008),
            ratio(i128::from(start + 192) * 5, 8008),
            -6000,
        )],
        vec![],
    ))
    .unwrap();
    let mut asset = document.assets()[&AssetId::new("media").unwrap()].clone();
    asset.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    asset.still_image = false;
    wire["assets"]["media"] = serde_json::to_value(asset).unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let sound = SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 7000,
                time_base,
            },
            SourceTimestamp {
                ticks: 7256,
                time_base,
            },
        )
        .unwrap(),
    };
    wire["sounds"] = serde_json::to_value(BTreeMap::from([(
        SoundId::new("independent").unwrap(),
        SoundEvent {
            owner: id("root"),
            label: "Independent root sound".into(),
            mapping: SourceAudioMapping::natural_rate(
                sound.span,
                document.presentation_basis().frame_rate,
            )
            .unwrap(),
            source: sound,
            offset: AudioSample(start),
            gain_millidecibels: -6000,
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn bound_case(original: &ProjectDocument, resume: i64) -> ProjectDocument {
    let captured = capture_unbound_audio_bindings(
        original,
        AudioTimingId {
            allocation: revision("gain-before-prefix"),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut binding = captured.bindings()[&id("a")].clone();
    if resume != 0 {
        binding.resume = Some(AudioResume {
            local_boundary: ExactRatio::ZERO,
            phase: AudioLocalPhase {
                constant: ratio(i128::from(resume) * 5, 8008),
                terms: vec![],
            },
        });
    }
    let state = AudioBindingState::new(
        captured
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        BTreeMap::from([(id("a"), binding)]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(original).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(hold(2)).unwrap();
    let moved = ProjectDocument::from_json(&wire.to_string()).unwrap();
    with_bindings(&moved, &state)
}

fn authored(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    parts: &[u32],
) -> Vec<[f32; 2]> {
    assert_eq!(parts.iter().sum::<u32>(), 256);
    assert!(parts.iter().all(|count| *count > 0 && *count <= 256));
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let mut offsets = Vec::new();
    let mut offset = 0_u32;
    for count in parts {
        offsets.push((offset, *count));
        offset += count;
    }
    let mut result = vec![[0.; 2]; 256];
    // A fresh renderer starts at the last chunk, then seeks backward. The
    // expected gain and PCM do not depend on warm sequential reads.
    for (offset, count) in offsets.into_iter().rev() {
        let block = renderer
            .prepare_authored_bus(
                provider,
                AudioSample(start + i64::from(offset)),
                count,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        result[offset as usize..(offset + count) as usize].copy_from_slice(&block.samples);
    }
    result
}

fn owner_at(document: &ProjectDocument, at: i64) -> ExactRatio {
    let plan = RenderPlan::compile(document).unwrap();
    let query = plan
        .audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
        .unwrap();
    query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == id("a"))
        .unwrap()
        .sampling()
        .local_at(AudioSample(at))
        .unwrap()
}

// Independent NTSC arithmetic: first source sample is 2/5 past its mapping
// start. Add its independent seven-sample offset to get owner frame position.
fn local(resume: i64, sample: i64) -> ExactRatio {
    ratio(37 + 5 * i128::from(resume + sample), 8008)
}

fn expected(provider: &Provider, resume: i64, extra: bool) -> Vec<[f32; 2]> {
    let source = source_oracle(provider, 0..6000, ratio(2 + 5 * i128::from(resume), 5), 256);
    let sound = source_oracle(provider, 7000..7256, ExactRatio::integer(7000), 256);
    let q = i128::from(GAIN_NUMERIC_SCALE);
    (0..256)
        .map(|at| {
            let position = 37 + 5 * (i128::from(resume) + at as i128);
            let ramp = if position < 1280 {
                // Exact linear progress rounds once to Q32. Integer endpoint dB
                // makes interpolation itself exact on that grid.
                -6000.0
                    + 12_000.0 * ratio(position * q, 1280).round_even().unwrap() as f64
                        / GAIN_NUMERIC_SCALE as f64
            } else {
                0.0
            };
            let root = -3000.0
                - if (128..192).contains(&at) {
                    6000.0
                } else {
                    0.0
                };
            let fresh = if extra && (16..32).contains(&at) {
                6000.0
            } else {
                0.0
            };
            let source_amplitude = if (517..597).contains(&position) {
                0.0
            } else {
                10_f64.powf((3000.0 + ramp + root + fresh) / 20_000.0)
            };
            let sound_amplitude = 10_f64.powf((-6000.0 + root) / 20_000.0);
            std::array::from_fn(|channel| {
                (f64::from(source[at][channel]) * source_amplitude
                    + f64::from(sound[at][channel]) * sound_amplitude) as f32
            })
        })
        .collect()
}

#[test]
fn physical_source_prefix_translates_gain_keys_once_and_preserves_authored_pcm() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut voice = source(rate, 0, 6000, 4);
    let NodeKind::Source { source } = &mut voice.kind else {
        unreachable!()
    };
    source.audio_offset = AudioSample(7);
    voice.audio_edges.node_start = AudioEdgePolicy::Hard;
    voice.audio_edges.node_end = AudioEdgePolicy::Hard;
    voice.audio_edges.source_placement_start = AudioEdgePolicy::Hard;
    voice.audio_edges.source_placement_end = AudioEdgePolicy::Hard;
    let (original, mut provider) =
        document(rate, &["lead", "a"], vec![("lead", hold(1)), ("a", voice)]);
    for (label, raw, start, resume) in [
        ("plain", original.clone(), 1609, 0),
        ("bound", bound_case(&original, 0), 3210, 0),
        ("resumed", bound_case(&original, 64), 3210, 64),
    ] {
        let before = with_controls(&raw, start);
        let oracle = expected(&provider, resume, false);
        assert_eq!(
            authored(&before, &mut provider, start, &[256]),
            oracle,
            "{label} independent PCM"
        );
        let mut patch = prefix_patch(&before, "root", 1, before.audio_bindings());
        let untreated = patch.apply(&before).unwrap();
        assert_ne!(
            authored(&untreated, &mut provider, start, &[256]),
            oracle,
            "missing key translation must change gain"
        );
        patch
            .nodes
            .get_mut(&id("a"))
            .unwrap()
            .after
            .as_mut()
            .unwrap()
            .audio_treatments = before.nodes()[&id("a")]
            .audio_treatments
            .with_owner_prefix(frames(1))
            .unwrap();
        let after = patch.apply(&before).unwrap();
        assert_eq!(after.sounds(), before.sounds());
        assert_eq!(
            after.nodes()[&id("root")].audio_treatments,
            before.nodes()[&id("root")].audio_treatments
        );
        assert_eq!(
            after.audio_bindings().timings(),
            before.audio_bindings().timings()
        );
        for sample in [0, 16, 32, 96, 112, 255] {
            assert_eq!(
                owner_at(&before, start + sample),
                local(resume, sample),
                "{label} independently retained owner position"
            );
            assert_eq!(
                owner_at(&after, start + sample),
                local(resume, sample).checked_add(ExactRatio::ONE).unwrap(),
                "{label} rebased owner position"
            );
        }
        for pieces in [&[73, 55, 128][..], &[17, 127, 112][..]] {
            assert_eq!(
                authored(&after, &mut provider, start, pieces),
                oracle,
                "{label} cold chunks {pieces:?}"
            );
        }
        // Source range mute cannot silence the independent root-owned sound.
        let first_muted = usize::try_from(96 - resume).unwrap();
        let first_unmuted = usize::try_from(112 - resume).unwrap();
        let sound = source_oracle(&provider, 7000..7256, ExactRatio::integer(7000), 256);
        let sound_only = |at: usize| {
            sound[at].map(|value| (f64::from(value) * 10_f64.powf(-9000.0 / 20_000.0)) as f32)
        };
        assert_eq!(oracle[first_muted], sound_only(first_muted));
        assert_eq!(oracle[first_unmuted - 1], sound_only(first_unmuted - 1));
        assert_ne!(oracle[first_muted - 1], sound_only(first_muted - 1));
        assert_ne!(oracle[first_unmuted], sound_only(first_unmuted));
        let restored = patch.inverse().apply(&after).unwrap();
        assert_eq!(restored, before);
        assert_eq!(
            authored(&restored, &mut provider, start, &[101, 155]),
            oracle
        );

        // A new factor is authored directly in the expanded physical clock.
        // Its keys are not passed through the old effects' prefix transform.
        let clip = after.nodes()[&id("a")]
            .audio_treatments
            .clip_gain()
            .unwrap();
        let mut envelopes = clip.envelopes().to_vec();
        envelopes.push(step_gain(
            local(resume, 16).checked_add(ExactRatio::ONE).unwrap(),
            local(resume, 32).checked_add(ExactRatio::ONE).unwrap(),
            6000,
        ));
        let mut wire = serde_json::to_value(&after).unwrap();
        wire["nodes"]["a"]["audio_treatments"] =
            serde_json::to_value(AudioTreatments::from_clip_gain(
                ClipGain::new(
                    clip.trim(),
                    clip.muted(),
                    envelopes,
                    clip.mute_ranges().to_vec(),
                )
                .unwrap(),
            ))
            .unwrap();
        let fresh = ProjectDocument::from_json(&wire.to_string()).unwrap();
        assert_eq!(
            authored(&fresh, &mut provider, start, &[53, 203]),
            expected(&provider, resume, true),
            "{label} independent new factor"
        );
    }
}
