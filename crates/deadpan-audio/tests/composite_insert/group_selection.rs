//! Real decoded 44.1 kHz PCM through canonical raw and authored buses. Video
//! timestamps are synthetic pure-plan witnesses, not decoded-picture/GPU proof.

use super::*;

fn boundary(frame: i64) -> i64 {
    ntsc().audio_boundary(ProjectFrame(frame)).unwrap().0
}

fn frame_range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn grouped(
    before: &ProjectDocument,
    parent: &str,
    selection: SliceCaptureSelection,
    name: &str,
) -> ProjectDocument {
    let query = before.group_selection(&id(parent), &selection).unwrap();
    let after = edit(
        before,
        name,
        Command::GroupSelection {
            parent: id(parent),
            selection,
            label: "Named gag".into(),
            identities: GroupSelectionIdentities {
                group: id(name),
                split: SplitIdentities {
                    nodes: (0..query.required_split_ids)
                        .map(|n| id(&format!("{name}-split-{n}")))
                        .collect(),
                },
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    after
}

fn ungrouped(before: &ProjectDocument, target: &str, name: &str) -> ProjectDocument {
    let after = edit(before, name, Command::Ungroup { node: id(target) });
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    assert_eq!(after.sound_allowances(), before.sound_allowances());
    after
}

fn gain(frames: i64, from: i32, to: i32) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(
            GainDb::UNITY,
            false,
            vec![
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    GainRange::new(ExactRatio::ZERO, ExactRatio::integer(frames)).unwrap(),
                    GainDb::new(from).unwrap(),
                    vec![
                        GainSegment::new(
                            ExactRatio::integer(frames),
                            GainDb::new(to).unwrap(),
                            GainCurve::Linear,
                        )
                        .unwrap(),
                    ],
                )
                .unwrap(),
            ],
            vec![],
        )
        .unwrap(),
    )
}

fn framed(mut node: BeatNode, label: &str, end_scale: i64) -> BeatNode {
    node.label = label.into();
    node.framing = Some(
        Framing::creep(
            FramingPose::default(),
            FramingPose::new(ratio(2, 5), ratio(3, 5), ExactRatio::integer(end_scale)).unwrap(),
            FramingCurve::Linear,
        )
        .unwrap(),
    );
    node
}

/// The WAV is decoded by Provider. These explicit synthetic video descriptors
/// only exercise RenderPlan's exact timestamp and live framing composition.
fn picture_clocks(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    let mut asset = document.assets()[&AssetId::new("media").unwrap()].clone();
    asset.video = asset.audio;
    asset.still_image = false;
    asset.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    wire["assets"]["media"] = serde_json::to_value(asset).unwrap();
    for (id, beat) in document.nodes() {
        if let NodeKind::Source { source } = &beat.kind {
            let audio = source.audio.as_ref().unwrap();
            wire["nodes"][id.as_str()]["kind"]["source"]["video"] =
                serde_json::to_value(SourceVideo::Stream {
                    asset: audio.asset.clone(),
                    span: audio.span,
                })
                .unwrap();
        }
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn chunk(
    reader: &mut StageAudio,
    provider: &mut Provider,
    start: i64,
    count: u32,
    authored: bool,
) -> Vec<[f32; 2]> {
    if authored {
        reader
            .prepare_authored_bus(
                provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples
    } else {
        read(reader, provider, start, count)
    }
}

fn complete(document: &ProjectDocument, provider: &mut Provider, authored: bool) -> Vec<[f32; 2]> {
    let end = boundary(document.duration().unwrap().frames());
    let mut reader = renderer(document, provider);
    (0..end)
        .step_by(251)
        .flat_map(|at| {
            chunk(
                &mut reader,
                provider,
                at,
                u32::try_from((end - at).min(251)).unwrap(),
                authored,
            )
        })
        .collect()
}

#[track_caller]
fn assert_bits(actual: &[[f32; 2]], expected: &[[f32; 2]], at: usize) {
    assert_eq!(actual.len(), expected.len());
    for (offset, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            actual
                .iter()
                .chain(expected)
                .all(|sample| sample.is_finite())
        );
        assert_eq!(
            actual.map(f32::to_bits),
            expected.map(f32::to_bits),
            "canonical PCM differs at sample {}",
            at + offset,
        );
    }
}

fn shuffled(document: &ProjectDocument, expected: &[[f32; 2]], authored: bool, cold_at: usize) {
    // A new verified source session and a new StageAudio ensure this document
    // cannot reuse either the old document's decoded window or Preserve cache.
    let mut provider = Provider::new();
    let mut reader = renderer(document, &mut provider);
    for at in [cold_at, 0, cold_at] {
        let count = (expected.len() - at).min(97);
        assert_bits(
            &chunk(
                &mut reader,
                &mut provider,
                i64::try_from(at).unwrap(),
                u32::try_from(count).unwrap(),
                authored,
            ),
            &expected[at..at + count],
            at,
        );
    }
    let starts: Vec<_> = (0..expected.len()).step_by(193).collect();
    // Compare every output sample with a different chunking and access order.
    for &at in starts
        .iter()
        .step_by(2)
        .rev()
        .chain(starts.iter().skip(1).step_by(2))
    {
        let count = (expected.len() - at).min(193);
        assert_bits(
            &chunk(
                &mut reader,
                &mut provider,
                i64::try_from(at).unwrap(),
                u32::try_from(count).unwrap(),
                authored,
            ),
            &expected[at..at + count],
            at,
        );
    }
    assert!(provider.calls > 0, "real fixture samples must be requested");
}

fn same_pictures(before: &ProjectDocument, after: &ProjectDocument) {
    let old = RenderPlan::compile(before).unwrap();
    let new = RenderPlan::compile(after).unwrap();
    let mut source_frames = 0;
    for frame in (0..before.duration().unwrap().frames()).rev() {
        let was = old.picture(ProjectFrame(frame)).unwrap();
        let now = new.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(now.picture, was.picture, "picture at frame {frame}");
        assert_eq!(now.local_position, was.local_position);
        assert_eq!(now.gap_after, was.gap_after);
        assert_eq!(now.picture_context, was.picture_context);
        if matches!(now.picture, deadpan_plan::Picture::Source { .. }) {
            source_frames += 1;
        }
        let framing = |document: &ProjectDocument, sample: &deadpan_plan::PictureSample| {
            sample
                .framing
                .iter()
                .filter_map(|layer| {
                    layer.instance.validate(document).unwrap();
                    layer.pose.map(|pose| {
                        (
                            document.nodes()[&layer.instance.node].label.clone(),
                            layer.local_position,
                            layer.duration,
                            pose,
                        )
                    })
                })
                .collect::<Vec<_>>()
        };
        // Physical cloned IDs can differ. Ordered authored operations, complete
        // owner clocks and evaluated poses must remain exact and apply once.
        assert_eq!(framing(after, &now), framing(before, &was));
        now.instance.validate(after).unwrap();
    }
    assert!(
        source_frames > 0,
        "picture check must include real Source mappings"
    );
}

#[test]
fn partial_ntsc_source_group_keeps_offset_pcm_and_exact_picture_coordinates() {
    let mut voice = framed(source(ntsc(), 8), "source camera", 3);
    let NodeKind::Source {
        source: voice_source,
    } = &mut voice.kind
    else {
        unreachable!()
    };
    voice_source.audio_offset = AudioSample(17);
    voice.audio_treatments = gain(8, -6000, -1000);
    let before = picture_clocks(&document(ntsc(), &["voice"], vec![("voice", voice)]));
    let grouped = grouped(
        &before,
        "root",
        SliceCaptureSelection::Range {
            range: frame_range(1, 5),
        },
        "partial-source",
    );
    let restored = ungrouped(&grouped, "partial-source", "source-ungroup");
    let mut provider = Provider::new();
    let raw = complete(&before, &mut provider, false);
    let authored = complete(&before, &mut provider, true);
    assert_eq!(raw.len(), 12_813); // B(8), preserving the NTSC sample origin.
    assert_ne!(raw, authored, "the owner gain envelope must affect PCM");
    assert!(raw[..17].iter().all(|sample| *sample == [0.; 2]));
    // Independent source-phase witness: sample 257 has advanced 240 samples
    // from the separate +17 mix-sample offset, without rounding a frame entry.
    assert_close(
        &raw[257..385],
        &expected(&provider, ExactRatio::integer(240), 128),
    );
    for document in [&grouped, &restored] {
        same_pictures(&before, document);
        shuffled(
            document,
            &raw,
            false,
            usize::try_from(boundary(5)).unwrap() - 97,
        );
        shuffled(
            document,
            &authored,
            true,
            usize::try_from(boundary(5)).unwrap() - 97,
        );
    }
}

fn composite_fixture() -> ProjectDocument {
    let mut repeating = framed(repeat("repeat-voice", 3), "repeat camera", 2);
    repeating.audio_treatments = gain(6, -4000, -1000);
    let mut stage = framed(preserve("voice", 4, 12), "preserve camera", 3);
    stage.audio_treatments = gain(12, -5000, -1000);
    let mut voice = framed(source(ntsc(), 4), "voice camera", 2);
    let NodeKind::Source {
        source: voice_source,
    } = &mut voice.kind
    else {
        unreachable!()
    };
    voice_source.audio_offset = AudioSample(-11);
    voice.audio_treatments = gain(4, -2000, 0);
    let mut scope = framed(
        BeatNode::sequence("scope", vec![id("repeat"), id("crop"), id("quiet")]),
        "scope camera",
        2,
    );
    scope.audio_treatments = gain(13, -3000, -1000);
    let initial = picture_clocks(&document(
        ntsc(),
        &["lead", "scope", "tail"],
        vec![
            ("lead", source(ntsc(), 2)),
            ("scope", scope),
            ("repeat", repeating),
            (
                "repeat-voice",
                framed(source(ntsc(), 2), "repeat voice camera", 2),
            ),
            ("crop", framed(partition("stage", 2..7), "crop camera", 2)),
            ("stage", stage),
            ("voice", voice),
            ("quiet", silence(2)),
            ("tail", source(ntsc(), 3)),
        ],
    ));
    let full = audio(0..44_117);
    let natural = SourceAudioMapping::natural_rate(full.span, ntsc())
        .unwrap()
        .duration_frames(frames(18))
        .unwrap();
    let sounded = edit(
        &initial,
        "group-sound",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event: SoundEvent {
                owner: id("root"),
                label: "Routed effect".into(),
                source: full,
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::ZERO,
                    frames: natural,
                    selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(17))
                        .unwrap(),
                },
                offset: AudioSample(7),
                gain_millidecibels: -6000,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    );
    // This establishes old sampling/resume bindings and a nonidentity root
    // sound route before the Group command, including a silent routed gap.
    let paused = insert_pause(&sounded, 2, 1, "group-old-pause");
    let allowed = edit(
        &paused,
        "group-allowance",
        Command::SetSoundAllowance {
            sound: SoundId::new("effect").unwrap(),
            issuer: SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: id("quiet"),
                    repeats: vec![],
                },
            },
            allowed: true,
        },
    );
    let mut wire = serde_json::to_value(&allowed).unwrap();
    wire["nodes"]["root"]["audio_treatments"] =
        serde_json::to_value(gain(19, -3000, -1000)).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn partial_repeat_preserve_group_and_ordinary_ungroup_keep_authored_pcm_and_routes() {
    let before = composite_fixture();
    assert_eq!(before.duration().unwrap().frames(), 19);
    assert!(!before.audio_bindings().is_empty());
    assert!(!before.sound_routes().is_empty());
    assert!(!before.sound_allowances().is_empty());
    // Scope is [3,16): Repeat [3,9), partial Preserve [9,14), silence [14,16).
    // Both endpoints require complete composite contexts, with existing clocks.
    let partial = grouped(
        &before,
        "scope",
        SliceCaptureSelection::Range {
            range: frame_range(4, 13),
        },
        "partial-composite",
    );
    let restored = ungrouped(&partial, "partial-composite", "partial-ungroup");
    let ordinary = edit(
        &before,
        "ordinary-group",
        Command::Group {
            parent: id("scope"),
            start: 0,
            end: 3,
            id: id("ordinary-group"),
            label: "All three children".into(),
        },
    );
    let roundtrip = ungrouped(&ordinary, "ordinary-group", "ordinary-ungroup");
    assert_eq!(roundtrip.nodes(), before.nodes());
    assert_eq!(ordinary.audio_bindings(), before.audio_bindings());
    assert_eq!(roundtrip.audio_bindings(), before.audio_bindings());
    let mut provider = Provider::new();
    let raw = complete(&before, &mut provider, false);
    let authored = complete(&before, &mut provider, true);
    assert_eq!(raw.len(), 30_430); // B(19), not a sum of rounded child lengths.
    assert_ne!(raw, authored);
    for (start, end) in [(3, 9), (9, 14), (16, 19)] {
        let selected =
            usize::try_from(boundary(start)).unwrap()..usize::try_from(boundary(end)).unwrap();
        assert!(
            raw[selected]
                .iter()
                .flatten()
                .any(|sample| sample.abs() > 0.001)
        );
    }
    let quiet = usize::try_from(boundary(14)).unwrap()..usize::try_from(boundary(16)).unwrap();
    assert!(raw[quiet.clone()].iter().all(|sample| *sample == [0.; 2]));
    assert!(
        authored[quiet]
            .iter()
            .flatten()
            .any(|sample| sample.abs() > 0.001),
        "the actual routed sound must survive the explicit Hold allowance"
    );
    for document in [&partial, &restored, &ordinary, &roundtrip] {
        assert_eq!(document.sounds(), before.sounds());
        assert_eq!(document.sound_routes(), before.sound_routes());
        assert_eq!(document.sound_allowances(), before.sound_allowances());
        same_pictures(&before, document);
        let cold_at = usize::try_from(boundary(13)).unwrap() - 97;
        shuffled(document, &raw, false, cold_at);
        shuffled(document, &authored, true, cold_at);
    }
}
