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
