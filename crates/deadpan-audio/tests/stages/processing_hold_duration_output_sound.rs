//! A fixed stage's independent output sound survives a changed input operand.
use super::*;

const EVENT: &str = "event";

fn change(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(name).unwrap(),
            command,
        },
    )
    .unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    let reopened = ProjectDocument::from_json(&after.to_json().unwrap()).unwrap();
    assert_eq!(reopened, after);
    reopened
}

fn event() -> BeatSound {
    let mut source = audio_at_rate(3100, 3200, 44_100);
    source.asset = AssetId::new(EVENT).unwrap();
    BeatSound {
        label: "Independent stage output".into(),
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            FrameRate::new(30_000, 1001).unwrap(),
        )
        .unwrap(),
        source,
        offset: AudioSample(37),
        treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    }
}

fn attached(document: &ProjectDocument, owner: &str) -> ProjectDocument {
    let mut event = event();
    if owner == "z-body" {
        // Keep the negative input-owned voice inside the selected input[1,8),
        // rather than obtaining a refusal for a voice cropped out entirely.
        event.mapping = SourceAudioMapping::Placement {
            start: ExactRatio::integer(2),
            frames: event.mapping.duration_frames(duration(9)).unwrap(),
        };
    }
    change(
        document,
        &format!("attach-{owner}"),
        Command::SetBeatSound {
            owner: id(owner),
            id: SoundId::new(EVENT).unwrap(),
            event,
        },
    )
}

fn output_fixture(journal: bool, child_sound: bool) -> ProjectDocument {
    output_fixture_with_treatments(journal, child_sound, &[])
}

fn output_fixture_with_treatments(
    journal: bool,
    child_sound: bool,
    treatments: &[(&str, AudioTreatments)],
) -> ProjectDocument {
    let mut before = voice();
    let NodeKind::Source { source } = &mut before.kind else {
        unreachable!()
    };
    source.duration = duration(3);
    let room = || room_tone(2, audio_at_rate(3100, 23_100, 44_100));
    // z-body sorts after processor: a scope-only proof cache must not reuse
    // the processor's output-only proof for the child-owned event below.
    let document = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["prefix", "processor", "suffix"],
        [
            ("prefix", room_tone(3, audio_at_rate(3100, 23_100, 44_100))),
            (
                "processor",
                retime("z-body", 4, 1..8, PitchPolicy::Preserve),
            ),
            (
                "z-body",
                BeatNode::sequence("Input", vec![id("before"), id("pause"), id("after")]),
            ),
            ("before", before.clone()),
            ("pause", room()),
            ("after", voice()),
            ("suffix", before),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let mut wire = serde_json::to_value(&document).unwrap();
    let mut record = document.assets()[&AssetId::new("media").unwrap()].clone();
    record.label = "Independent event PCM".into();
    let event_bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav"),
    )
    .unwrap();
    record.content_hash = Sha256::digest(&event_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    // Match the qualified sound-clock fixtures in audio_definition: the core
    // admits a qualified catalog asset, and the provider separately verifies
    // actual WAV bytes/index before exposing PCM or retained-context reads.
    record.source_qualification = Some(SourceQualificationId::new("c".repeat(64)).unwrap());
    wire["assets"][EVENT] = serde_json::to_value(record).unwrap();
    // Hard node edges keep the test about clock transport, with no coincident
    // ancestor envelope modifying the explicit hard-edged event oracle.
    let mut nodes = document.nodes().clone();
    for node in nodes.values_mut() {
        node.audio_edges.node_start = AudioEdgePolicy::Hard;
        node.audio_edges.node_end = AudioEdgePolicy::Hard;
    }
    for (owner, treatments) in treatments {
        nodes.get_mut(&id(owner)).unwrap().audio_treatments = treatments.clone();
    }
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let document = attached(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        "processor",
    );
    let document = if child_sound {
        attached(&document, "z-body")
    } else {
        document
    };
    if journal {
        change(
            &document,
            "move-unchanged-stage",
            Command::SetHoldDuration {
                node: id("prefix"),
                duration: duration(4),
            },
        )
    } else {
        document
    }
}

struct SeparateProvider {
    original: PreparedSource,
    event: PreparedSource,
    assets: BTreeMap<AssetId, AssetRecord>,
    denied: bool,
    retained_admissions: usize,
}

fn finite_zero_source() -> PreparedSource {
    // Actual finite 44.1-kHz mono PCM, not a mute treatment or a silent Hold.
    let data_bytes = 44_117_u32 * 2;
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data_bytes).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16_u32.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(44_100_u32.to_le_bytes());
    wav.extend(88_200_u32.to_le_bytes());
    wav.extend(2_u16.to_le_bytes());
    wav.extend(16_u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(data_bytes.to_le_bytes());
    wav.resize(44 + data_bytes as usize, 0);
    let cancelled = AtomicBool::new(false);
    let session = AudioSession::open_verified(
        &mut Cursor::new(&wav),
        SourceContentIdentity::new(Sha256::digest(&wav).into(), wav.len() as u64).unwrap(),
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let index = session.index().clone();
    PreparedSource::with_layout(
        session,
        &index,
        AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
        &cancelled,
    )
    .unwrap()
}

impl SeparateProvider {
    fn new(document: &ProjectDocument, zero_original: bool) -> Self {
        Self {
            original: if zero_original {
                finite_zero_source()
            } else {
                mono_original(2).1.source
            },
            event: mono_original(2).1.source,
            assets: document.assets().clone(),
            denied: false,
            retained_admissions: 0,
        }
    }

    fn admitted(&self, asset: &AssetId) -> Result<&PreparedSource, PreparationError> {
        if self.denied && asset.as_str() == EVENT {
            return Err(PreparationError::SourceUnavailable(
                "revoked independent event qualification".into(),
            ));
        }
        assert!(self.assets.contains_key(asset));
        Ok(if asset.as_str() == EVENT {
            &self.event
        } else {
            &self.original
        })
    }
}

impl AudioSourceProvider for SeparateProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        _revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(project, &ProjectId::new("stage-project").unwrap());
        assert!(!cancelled.load(Ordering::Relaxed));
        self.admitted(asset)
    }

    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(expected, &self.assets[asset]);
        self.retained_admissions += 1;
        self.source(project, revision, asset, cancelled)
    }
}

fn bus(
    renderer: &mut StageAudio,
    provider: &mut SeparateProvider,
    start: i64,
    count: u32,
) -> Recorded {
    let block = renderer
        .prepare_authored_bus(
            provider,
            AudioSample(start),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    Recorded {
        samples: block.samples,
        suppressed: (start..start + i64::from(count))
            .map(|sample| {
                block
                    .suppressed
                    .iter()
                    .any(|span| span.contains(&AudioSample(sample)))
            })
            .collect(),
    }
}

fn full_bus(
    document: &ProjectDocument,
    provider: &mut SeparateProvider,
    pieces: &[u32],
) -> Recorded {
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let total = renderer.plan().audio_duration().unwrap().0;
    let mut result = Recorded {
        samples: Vec::new(),
        suppressed: Vec::new(),
    };
    for count in pieces.iter().copied().cycle() {
        let start = result.samples.len() as i64;
        if start == total {
            break;
        }
        let block = bus(
            &mut renderer,
            provider,
            start,
            count.min((total - start) as u32),
        );
        result.samples.extend(block.samples);
        result.suppressed.extend(block.suppressed);
    }
    result
}

fn event_reference(provider: &SeparateProvider, start: i64, phase: ExactRatio) -> Vec<[f32; 2]> {
    let recipe = ResampleRecipe::new(
        3100..3200,
        phase,
        AudioSample(start),
        ratio(147, 160),
        AudioSample(start)..AudioSample(start + 109),
    )
    .unwrap();
    assert_eq!(recipe.source_at(AudioSample(start)).unwrap(), phase);
    provider
        .event
        .prepare(
            recipe,
            AudioSample(start),
            109,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

fn physical_suppression(journal: bool) -> Vec<bool> {
    let mut mask = vec![false; ntsc_boundary(if journal { 11 } else { 10 })];
    if journal {
        // The genuine root ripple moves a 6406-point captured processor output
        // into a 6407-point current allocation. Its final extra point exhausts
        // the retained physical output; RoomTone policy is still non-silent.
        // Compute these boundaries from literal frame geometry, not the plan
        // being tested or the observed component's suppression metadata.
        let old_start = ntsc_boundary(3);
        let old_end = ntsc_boundary(7);
        let current_start = ntsc_boundary(4);
        let current_end = ntsc_boundary(8);
        assert_eq!((old_start, old_end), (4805, 11211));
        assert_eq!((current_start, current_end), (6406, 12813));
        let retained_end = current_start + old_end - old_start;
        assert_eq!(retained_end, 12812);
        mask[retained_end..current_end].fill(true);
    }
    mask
}

fn witness_output_sound(length: i64, journal: bool) {
    let before = output_fixture(journal, false);
    assert_eq!(before.node_duration(&id("z-body")).unwrap(), duration(9));
    for owner in ["prefix", "pause"] {
        assert!(
            matches!(&before.nodes()[&id(owner)].kind, NodeKind::Hold { recipe } if matches!(recipe.audio, HoldAudio::RoomTone { .. }))
        );
    }
    assert!(before.sound_allowances().is_empty());
    let saved = before.beat_sounds().clone();
    let clocks = before.audio_bindings().sound_clocks().clone();
    if journal {
        let clock = &clocks[&id("processor")][&SoundId::new(EVENT).unwrap()].clocks()[0];
        assert_eq!(clock.scope(), &id("processor"));
        assert_eq!(clock.owner(), &id("processor"));
        assert_eq!(
            clock.timing().allocation,
            RevisionId::new("move-unchanged-stage").unwrap()
        );
    } else {
        assert!(clocks.is_empty());
    }
    let after = resize(&before, length, "changed-input-independent-output");
    assert_eq!(after.beat_sounds(), &saved);
    assert_eq!(after.audio_bindings().sound_clocks(), &clocks);
    let start = if journal { 6443 } else { 4842 };
    let mut provider = SeparateProvider::new(&after, true);
    // The historical 1/5 mix-sample residue survives even when a root ripple
    // changes the new ideal onset residue to -2/5. Derive neither from the plan.
    let event = event_reference(&provider, start, ratio(2_480_147, 800));
    assert!(event.iter().flatten().any(|value| value.abs() > 0.01));
    let wrong = event_reference(&provider, start, ratio(1_239_853, 400));
    assert_ne!(event, wrong, "known PCM must detect a phase reset");
    let mut expected = Recorded {
        samples: vec![[0.; 2]; ntsc_boundary(if journal { 11 } else { 10 })],
        suppressed: physical_suppression(journal),
    };
    expected.samples[start as usize..start as usize + 109].copy_from_slice(&event);
    for document in [&before, &after] {
        for pieces in [&[256_u32][..], &[127, 3, 251][..]] {
            same_recorded(
                &full_bus(document, &mut provider, pieces),
                &expected,
                0,
                "complete independent output event and finite support",
            );
        }
        let plan = Arc::new(RenderPlan::compile(document).unwrap());
        // Every request starts from a cold renderer, including terminal and
        // pre-onset windows. Also warm a renderer in deliberately shuffled order.
        let mut warm = StageAudio::new(Arc::clone(&plan));
        let mut visits = vec![(109, 17), (43, 66), (-11, 54), (0, 1), (1, 108), (108, 2)];
        if journal {
            visits.extend([(12811 - start, 3), (12812 - start, 1)]);
        }
        for (offset, count) in visits {
            let at = start + offset;
            let mut cold = StageAudio::new(Arc::clone(&plan));
            for renderer in [&mut cold, &mut warm] {
                let actual = bus(renderer, &mut provider, at, count);
                assert_eq!(
                    actual.samples,
                    expected.samples[at as usize..at as usize + count as usize]
                );
                assert_eq!(
                    actual.suppressed,
                    expected.suppressed[at as usize..at as usize + count as usize]
                );
            }
        }
        provider.denied = true;
        assert!(provider.admitted(&AssetId::new("media").unwrap()).is_ok());
        assert!(
            matches!(
                warm.prepare_authored_bus(
                    &mut provider,
                    AudioSample(start),
                    109,
                    TIMEOUT,
                    &AtomicBool::new(false)
                ),
                Err(StageAudioError::Preparation(PreparationError::SourceUnavailable(message)))
                    if message == "revoked independent event qualification"
            ),
            "warm PCM must re-admit the independent event while Original remains available"
        );
        provider.denied = false;
    }
    if journal {
        assert!(provider.retained_admissions > 0);
    }

    // Current nonzero Original+RoomTone is allowed to change. Check the current
    // bus as its current physical component plus the independently known event,
    // rather than asserting unchanged old total PCM or muting an ancestor.
    let without = change(
        &after,
        "remove-event-component",
        Command::DeleteBeatSound {
            owner: id("processor"),
            id: SoundId::new(EVENT).unwrap(),
        },
    );
    let mut provider = SeparateProvider::new(&after, false);
    let component = full_bus(&without, &mut provider, &[239, 1]);
    let actual = full_bus(&after, &mut provider, &[127, 3, 251]);
    assert_eq!(component.suppressed, physical_suppression(journal));
    assert_eq!(actual.suppressed, component.suppressed);
    assert!(
        component
            .samples
            .iter()
            .flatten()
            .any(|value| value.abs() > 0.01)
    );
    let mut mixed = component.samples.clone();
    for (at, sample) in event.iter().enumerate() {
        for channel in 0..2 {
            mixed[start as usize + at][channel] += sample[channel];
        }
    }
    same_pcm(
        &actual.samples,
        &mixed,
        0,
        "current nonzero physical bus plus independent output event",
    );
    let old_component = change(
        &before,
        "remove-old-event-component",
        Command::DeleteBeatSound {
            owner: id("processor"),
            id: SoundId::new(EVENT).unwrap(),
        },
    );
    assert_ne!(
        full_bus(&old_component, &mut provider, &[251, 3]).samples,
        component.samples,
        "changed RoomTone operand must actually rebuild its current DSP"
    );
}

#[test]
fn fixed_retime_output_sound_fresh_growth_pcm() {
    witness_output_sound(3, false);
}
#[test]
fn fixed_retime_output_sound_fresh_shrink_pcm() {
    witness_output_sound(1, false);
}
#[test]
fn fixed_retime_output_sound_journaled_growth_pcm() {
    witness_output_sound(3, true);
}
#[test]
fn fixed_retime_output_sound_journaled_shrink_pcm() {
    witness_output_sound(1, true);
}

#[test]
fn fixed_retime_output_sound_current_nonlinear_owners_shape_current_shared_bus() {
    nonlinear_shared_bus_witness(false, false);
}

#[test]
fn fixed_retime_output_sound_fractional_owner_gap_complete_nonlinear_bus() {
    nonlinear_shared_bus_witness(true, false);
}

#[test]
fn fixed_retime_output_sound_fractional_owner_gap_keeps_negative_hold_gain_clock() {
    nonlinear_shared_bus_witness(true, true);
}

fn nonlinear_shared_bus_witness(include_before: bool, child_gain: bool) {
    let processor_drive = Saturation::new(GainDb::new(9000).unwrap()).unwrap();
    let root_drive = Saturation::new(GainDb::new(6000).unwrap()).unwrap();
    let mut treatments = vec![
        (
            "processor",
            AudioTreatments::default()
                .with_saturation(Some(processor_drive))
                .unwrap(),
        ),
        (
            "root",
            AudioTreatments::default()
                .with_saturation(Some(root_drive))
                .unwrap(),
        ),
    ];
    if child_gain {
        treatments.push((
            "before",
            AudioTreatments::from_clip_gain(
                ClipGain::new(GainDb::new(-6000).unwrap(), false, vec![], vec![]).unwrap(),
            ),
        ));
        treatments.push((
            "pause",
            AudioTreatments::from_clip_gain(
                ClipGain::new(
                    GainDb::new(-12_000).unwrap(),
                    false,
                    vec![
                        GainEnvelope::new(
                            GainClock::OwnerOutput,
                            GainRange::new(ExactRatio::ZERO, ExactRatio::integer(3)).unwrap(),
                            GainDb::new(3000).unwrap(),
                            vec![
                                GainSegment::new(
                                    ExactRatio::integer(3),
                                    GainDb::new(3000).unwrap(),
                                    GainCurve::Step,
                                )
                                .unwrap(),
                            ],
                        )
                        .unwrap(),
                    ],
                    vec![],
                )
                .unwrap(),
            ),
        ));
    }
    // Both nonlinear owners are already present when the actual root-ripple
    // journal is captured; the target edit changes only the RoomTone duration.
    let before = output_fixture_with_treatments(true, false, &treatments);
    let after = resize(&before, 3, "nonlinear-current-input");
    before.validate().unwrap();
    after.validate().unwrap();
    assert_eq!(after.nodes()[&id("root")], before.nodes()[&id("root")]);
    assert_eq!(
        after.audio_bindings().sound_clocks(),
        before.audio_bindings().sound_clocks()
    );
    let mut provider = SeparateProvider::new(&after, false);
    let event = event_reference(&provider, 6443, ratio(2_480_147, 800));
    let plain_before = output_fixture(true, false);
    let plain_after = resize(&plain_before, 3, "untreated-current-input");
    let mut physical = Vec::new();
    for (document, revision) in [
        (&plain_before, "old-nonlinear-component"),
        (&plain_after, "current-nonlinear-component"),
    ] {
        let without_event = change(
            document,
            revision,
            Command::DeleteBeatSound {
                owner: id("processor"),
                id: SoundId::new(EVENT).unwrap(),
            },
        );
        let component = full_bus(&without_event, &mut provider, &[239, 1]);
        assert_eq!(component.suppressed, physical_suppression(true));
        physical.push(component.samples);
    }
    // Independently locate sample8236 in the retained output grid. Moving the
    // stage translates its old root sample4805 to current6406, so this sample
    // reads oldroot6635. The PCM coordinate maps to input13727/4576, but
    // structural ownership uses the root n+1/2 probe: the old Source cut at
    // 6635.2 rounds to6635, so following Hold owns this allocated sample.
    // Its exact local clock is still -1/4576; ownership cannot clamp that
    // coordinate to zero or substitute the preceding Source's treatments.
    let historical_frame = ExactRatio::integer(8236 - 6406 + 4805)
        .checked_mul(ratio(5, 8008))
        .unwrap();
    let input_frame = historical_frame
        .checked_sub(ExactRatio::integer(3))
        .unwrap()
        .checked_mul(ratio(7, 4))
        .unwrap()
        .checked_add(ExactRatio::ONE)
        .unwrap();
    assert_eq!(input_frame, ratio(13_727, 4576));
    let old_cut = ExactRatio::integer(3)
        .checked_add(ratio(8, 7))
        .unwrap()
        .checked_mul(ratio(8008, 5))
        .unwrap();
    assert_eq!(old_cut, ratio(33_176, 5));
    let old_cut_sample = old_cut.round_even().unwrap();
    assert_eq!(old_cut_sample, 6635);
    assert_eq!(old_cut_sample - 4805 + 6406, 8236);
    let hold_local = input_frame.checked_sub(ExactRatio::integer(3)).unwrap();
    assert_eq!(hold_local, ratio(-1, 4576));
    assert_eq!(
        hold_local.checked_add(ratio(35, 32_032)).unwrap(),
        ratio(1, 1144)
    );
    if child_gain {
        let gain = &after.nodes()[&id("pause")].audio_treatments;
        assert_eq!(
            gain.evaluate(hold_local).unwrap().millidecibels,
            ExactRatio::integer(-12_000)
        );
        assert_eq!(
            gain.evaluate(ratio(1, 1144)).unwrap().millidecibels,
            ExactRatio::integer(-9000)
        );
        assert_eq!(
            gain.evaluate(ExactRatio::ZERO).unwrap().millidecibels,
            ExactRatio::integer(-9000)
        );
    }
    for document in [&before, &after] {
        assert!(
            matches!(&document.nodes()[&id("root")].kind, NodeKind::Sequence { children } if children == &vec![id("prefix"), id("processor"), id("suffix")])
        );
        assert!(
            matches!(&document.nodes()[&id("processor")].kind, NodeKind::Retime { child, .. } if child == &id("z-body"))
        );
        assert!(
            matches!(&document.nodes()[&id("z-body")].kind, NodeKind::Sequence { children } if children == &vec![id("before"), id("pause"), id("after")])
        );
        assert!(
            matches!(&document.nodes()[&id("pause")].kind, NodeKind::Hold { recipe } if matches!(recipe.audio, HoldAudio::RoomTone { .. }))
        );
    }
    assert!(
        physical
            .iter()
            .all(|raw| raw[8236].iter().any(|value| *value != 0.0))
    );
    // The structural probe chooses the following Hold. Its exact local clock
    // stays signed; query results are checked against literals and never supply
    // treatment values or processing order to the independent scalar oracle.
    for (document, raw) in [(&before, &physical[0]), (&after, &physical[1])] {
        let plan = RenderPlan::compile(document).unwrap();
        for (at, local) in [(8236, ratio(-1, 4576)), (8237, ratio(1, 1144))] {
            let owners = plan
                .audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
                .unwrap();
            assert_eq!(owners.spans().len(), 1);
            let span = &owners.spans()[0];
            assert_eq!(span.support(), deadpan_plan::AudioOwnerSupport::Active);
            assert_eq!(
                span.owners()
                    .iter()
                    .map(|owner| owner.instance().node.clone())
                    .collect::<Vec<_>>(),
                vec![id("root"), id("processor"), id("z-body"), id("pause")]
            );
            let hold = span.owners().last().unwrap();
            assert_eq!(hold.sampling().local_at(AudioSample(at)).unwrap(), local);
            assert_eq!(hold.sampling().local_frames_per_sample(), ratio(35, 32_032));
        }
        let exhausted = plan
            .audio_gain_owners(AudioSample(12812)..AudioSample(12813), Default::default())
            .unwrap();
        assert_eq!(exhausted.spans().len(), 1);
        assert_eq!(
            exhausted.spans()[0].support(),
            deadpan_plan::AudioOwnerSupport::Inactive
        );
        assert_eq!(
            exhausted.spans()[0]
                .owners()
                .iter()
                .map(|owner| owner.instance().node.clone())
                .collect::<Vec<_>>(),
            vec![id("root")]
        );
        assert_eq!(raw[12812], [0.0; 2]);
    }
    let processor = ntsc_boundary(4)..ntsc_boundary(8);
    // Literal retained structural allocations: Sourcebefore ends at8236;
    // after growth the Hold runs through10981. At its first point only trim
    // applies; its envelope starts at local0 and adds3dB from8237 onwards.
    let original_gain = |at: usize, hold_end: usize| -> f64 {
        let db = if !child_gain {
            0.0
        } else if (6406..8236).contains(&at) {
            -6000.0
        } else if at == 8236 {
            -12_000.0
        } else if (8237..hold_end).contains(&at) {
            -9000.0
        } else {
            0.0
        };
        10.0_f64.powf(db / 20_000.0)
    };
    let shape = |component: &[[f32; 2]], shared: bool, hold_end: usize| {
        component
            .iter()
            .enumerate()
            .map(|(at, original)| {
                std::array::from_fn(|channel| {
                    let sound = if (6443..6552).contains(&at) {
                        f64::from(event[at - 6443][channel])
                    } else {
                        0.0
                    };
                    let value = f64::from(original[channel]) * original_gain(at, hold_end);
                    let owner = if processor.contains(&at) {
                        if shared {
                            processor_drive.shape(value + sound)
                        } else {
                            processor_drive.shape(value) + processor_drive.shape(sound)
                        }
                    } else {
                        value + sound
                    };
                    root_drive.shape(owner) as f32
                })
            })
            .collect::<Vec<[f32; 2]>>()
    };
    let expected = shape(&physical[1], true, 10_982);
    if child_gain {
        // These controls rule out the tempting fixes: retain only ancestors,
        // reuse the preceding Source, or clamp the following Hold clock to0.
        for db in [0.0, -6000.0, -9000.0] {
            let wrong = physical[1][8236].map(|value| {
                root_drive
                    .shape(processor_drive.shape(f64::from(value) * 10.0_f64.powf(db / 20_000.0)))
                    as f32
            });
            assert_ne!(
                expected[8236], wrong,
                "Hold's negative local clock must distinguish {db}milli-dB"
            );
        }
        assert!(
            (6443..6552).any(|at| {
                let wrong: [f32; 2] = std::array::from_fn(|channel| {
                    root_drive.shape(processor_drive.shape(
                        (f64::from(physical[1][at][channel])
                            + f64::from(event[at - 6443][channel]))
                            * original_gain(at, 10_982),
                    )) as f32
                });
                wrong != expected[at]
            }),
            "child gain must never scale the independent processor-owned event"
        );
    }
    let stale = shape(&physical[0], true, 10_982);
    assert!(
        expected
            .iter()
            .zip(&stale)
            .enumerate()
            .any(|(at, (a, b))| at != 8236 && a != b),
        "old mixed input is stale on meaningful support"
    );
    let separated = shape(&physical[1], false, 10_982);
    assert!(
        expected
            .iter()
            .zip(&separated)
            .any(|(a, b)| { a.iter().zip(b).any(|(a, b)| (a - b).abs() > 0.0001) }),
        "this witness must distinguish shared-owner saturation from per-voice processing"
    );
    let previous_expected = shape(&physical[0], true, 10_067);
    let states = if include_before {
        vec![(&before, &previous_expected), (&after, &expected)]
    } else {
        vec![(&after, &expected)]
    };
    let mask = physical_suppression(true);
    for (document, expected) in states {
        let plan = Arc::new(RenderPlan::compile(document).unwrap());
        // Every sample is covered, including the former gap8236. The distinct
        // exhausted output point12812 remains zero with its exact mask.
        for pieces in [&[256_u32][..], &[127, 3, 251][..]] {
            let mut renderer = StageAudio::new(Arc::clone(&plan));
            let mut at = 0;
            for count in pieces.iter().copied().cycle() {
                if at == 17_618 {
                    break;
                }
                let count = count.min((17_618 - at) as u32);
                let actual = bus(&mut renderer, &mut provider, at, count);
                same_pcm(
                    &actual.samples,
                    &expected[at as usize..at as usize + count as usize],
                    at,
                    "complete shared processor then root saturation",
                );
                assert_eq!(
                    actual.suppressed,
                    mask[at as usize..at as usize + count as usize]
                );
                at += i64::from(count);
            }
        }
        for (at, count) in [
            (12_700, 114),
            (6442, 111),
            (9600, 256),
            (6406, 37),
            (8192, 44),
            (8237, 128),
            (8236, 1),
            (8192, 128),
            (8235, 3),
            (10_066, 3),
            (10_981, 3),
        ] {
            let mut cold = StageAudio::new(Arc::clone(&plan));
            let actual = bus(&mut cold, &mut provider, at, count);
            assert_eq!(
                actual.samples,
                expected[at as usize..at as usize + count as usize]
            );
            assert_eq!(
                actual.suppressed,
                mask[at as usize..at as usize + count as usize]
            );
        }
    }
}

#[test]
fn fixed_retime_output_sound_proof_does_not_admit_child_owned_sound() {
    for journal in [false, true] {
        let before = output_fixture(journal, true);
        let bytes = before.to_json().unwrap();
        if journal {
            for owner in ["processor", "z-body"] {
                let clock = &before.audio_bindings().sound_clocks()[&id(owner)]
                    [&SoundId::new(EVENT).unwrap()]
                    .clocks()[0];
                assert_eq!(clock.scope(), &id("processor"));
                assert_eq!(clock.owner(), &id(owner));
            }
        }
        for length in [1, 3] {
            assert!(
                apply(
                    &before,
                    &CommandRequest {
                        project_id: before.project_id().clone(),
                        expected_revision: before.revision_id().clone(),
                        new_revision: RevisionId::new("input-owned-refusal").unwrap(),
                        command: Command::SetHoldDuration {
                            node: id("pause"),
                            duration: duration(length)
                        },
                    }
                )
                .is_err(),
                "a complete changed input proof remains required"
            );
            assert_eq!(before.to_json().unwrap(), bytes);
        }
    }
}
