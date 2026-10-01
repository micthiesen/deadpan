//! Real qualified PCM witnesses for zero-time structural copies. These compare
//! raw processing and the authored bus; they do not qualify device delivery.
use super::*;

fn boundary(frame: i64) -> i64 {
    ntsc().audio_boundary(ProjectFrame(frame)).unwrap().0
}

fn fixture() -> ProjectDocument {
    let initial = document(
        ntsc(),
        &[
            "lead",
            "empty-left",
            "notes",
            "empty-right",
            "room",
            "crop",
            "quiet",
            "tail",
        ],
        vec![
            ("lead", source(ntsc(), 3)),
            ("empty-left", BeatNode::sequence("Left", vec![])),
            (
                "notes",
                BeatNode::sequence("Notes", vec![id("nested-notes")]),
            ),
            ("nested-notes", BeatNode::sequence("Nested notes", vec![])),
            ("empty-right", BeatNode::sequence("Right", vec![])),
            ("room", BeatNode::hold("Room tone", room(3, 700..921))),
            ("crop", partition("stage", 2..6)),
            ("stage", preserve("voice", 4, 12)),
            ("voice", source(ntsc(), 4)),
            ("quiet", silence(2)),
            ("tail", source(ntsc(), 3)),
        ],
    );
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["assets"]["media"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    let qualified = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let sounded = edit(
        &qualified,
        "structural-sound",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event: SoundEvent {
                owner: id("root"),
                label: "Retained root effect".into(),
                source: audio(100..20_100),
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::ZERO,
                    frames: ratio(2_000_000, 147_147),
                    selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(13))
                        .unwrap(),
                },
                offset: AudioSample(7),
                gain_millidecibels: -3000,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    );
    // This first, positive insertion gives the media owners retained sampling
    // bindings and turns the existing sound into a nontrivial routed event.
    let paused = insert_pause(&sounded, 3, 1, "retained-pause");
    let allowed = edit(
        &paused,
        "structural-allowance",
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
    let gain = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-6000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(&gain).unwrap();
    wire["nodes"]["notes"]["audio_treatments"] = serde_json::to_value(&gain).unwrap();
    // Retain explicit lineage on the root as well as physical owners. A zero
    // insertion must not detach the root merely because its child list changes.
    let lineage: BTreeMap<_, _> = allowed
        .nodes()
        .keys()
        .map(|node| {
            (
                node.clone(),
                AudioLineageId {
                    allocation: revision("retained-lineage"),
                    origin: node.clone(),
                },
            )
        })
        .collect();
    wire["audio_lineage"] = serde_json::to_value(lineage).unwrap();
    let marks: BTreeMap<_, _> = [
        ("note-edge", "notes", ExactRatio::ZERO, MarkState::Bound),
        (
            "hidden-tail",
            "tail",
            ExactRatio::integer(100),
            MarkState::Unresolved {
                reason: MarkLossReason::OutOfRange,
            },
        ),
    ]
    .into_iter()
    .map(|(name, owner, position, state)| {
        (
            MarkId::new(name).unwrap(),
            Mark {
                owner: id(owner),
                label: name.into(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: id(owner),
                        position,
                    },
                    bias: InsertionBias::Right,
                },
                loss_policy: AnchorLossPolicy::KeepUnresolved,
                state,
                fragments: vec![],
            },
        )
    })
    .collect();
    wire["marks"] = serde_json::to_value(marks).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn copied_notes(before: &ProjectDocument) -> CapturedEditSlice {
    let slice = CapturedEditSlice::capture_selection(
        before,
        before.root(),
        &SliceCaptureSelection::Child { node: id("notes") },
        AudioTimingId {
            allocation: revision("capture-notes"),
            ordinal: u32::MAX,
        },
    )
    .unwrap();
    assert_eq!(slice.duration(), frames(0));
    assert_eq!(slice.identity_requirements().unwrap().timings, 0);
    CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap()
}

fn paste(before: &ProjectDocument, slice: &CapturedEditSlice, index: usize) -> ProjectDocument {
    let name = format!("structural-slot-{index}");
    let required = slice.identity_requirements().unwrap();
    let after = edit(
        before,
        &name,
        Command::SpliceSlice {
            parent: before.root().clone(),
            index,
            slice: slice.clone(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|n| id(&format!("{name}-node-{n}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|n| id(&format!("{name}-alias-{n}")))
                    .collect(),
            },
            // There is no new sample clock, including at this final valid ordinal.
            timing: AudioTimingId {
                allocation: revision(&name),
                ordinal: u32::MAX,
            },
        },
    );
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    assert_eq!(after.sound_allowances(), before.sound_allowances());
    let old_root = &before.nodes()[before.root()];
    let new_root = &after.nodes()[after.root()];
    assert_eq!(new_root.audio_treatments, old_root.audio_treatments);
    assert_eq!(new_root.audio_edges, old_root.audio_edges);
    assert_eq!(new_root.framing, old_root.framing);
    assert_eq!(new_root.label, old_root.label);
    for (owner, lineage) in before.audio_lineage() {
        assert_eq!(after.audio_lineage().get(owner), Some(lineage));
    }
    for (mark, value) in before.marks() {
        assert_eq!(after.marks().get(mark), Some(value));
    }
    for (owner, node) in before.nodes() {
        if owner != before.root() {
            assert_eq!(after.nodes().get(owner), Some(node));
        }
    }
    after
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
        .step_by(256)
        .flat_map(|start| {
            chunk(
                &mut reader,
                provider,
                start,
                u32::try_from((end - start).min(256)).unwrap(),
                authored,
            )
        })
        .collect()
}

fn shuffled(
    document: &ProjectDocument,
    provider: &mut Provider,
    expected: &[[f32; 2]],
    authored: bool,
) {
    let mut reader = renderer(document, provider);
    // First seek cold into the end of the Preserve crop, then revisit it warm.
    let preserve_end = usize::try_from(boundary(11)).unwrap();
    for start in [preserve_end - 97, 0, preserve_end - 97] {
        assert_eq!(
            chunk(
                &mut reader,
                provider,
                i64::try_from(start).unwrap(),
                97,
                authored
            ),
            expected[start..start + 97]
        );
    }
    let chunks: Vec<_> = (0..expected.len()).step_by(193).collect();
    for start in chunks
        .iter()
        .step_by(2)
        .chain(chunks.iter().skip(1).step_by(2).rev())
    {
        let count = (expected.len() - start).min(193);
        assert_eq!(
            chunk(
                &mut reader,
                provider,
                i64::try_from(*start).unwrap(),
                u32::try_from(count).unwrap(),
                authored
            ),
            expected[*start..*start + count],
            "{} samples at {}",
            if authored { "authored" } else { "raw" },
            start,
        );
    }
}

#[test]
fn empty_group_paste_preserves_all_pcm_gain_and_routed_sounds_at_every_seam() {
    let before = fixture();
    assert!(!before.audio_bindings().is_empty());
    assert!(!before.sound_routes().is_empty());
    assert!(!before.sound_allowances().is_empty());
    let snapshot = before.clone();
    let slice = copied_notes(&before);
    assert_eq!(before, snapshot);
    let mut provider = Provider::new();
    let raw = complete(&before, &mut provider, false);
    let authored = complete(&before, &mut provider, true);
    assert_eq!(raw.len(), 25_626); // B(16), not sixteen rounded frame lengths.
    assert_eq!(authored.len(), raw.len());
    assert_ne!(authored, raw);
    assert!(raw.iter().flatten().any(|value| value.abs() > 0.001));
    // An independent source-phase check and RoomTone-cycle check give the
    // before/after comparison nonzero media witnesses away from edge fades.
    assert_close(&raw[..128], &expected(&provider, ExactRatio::ZERO, 128));
    let room_wave = room_reference(&provider, 700..921, 4805);
    let room_start = usize::try_from(boundary(4)).unwrap();
    assert_close(
        &raw[room_start..room_start + 128],
        &sample_reference(&room_wave, ratio(1, 5), ExactRatio::ONE, 128),
    );
    let quiet = usize::try_from(boundary(11)).unwrap()..usize::try_from(boundary(13)).unwrap();
    assert!(raw[quiet.clone()].iter().all(|sample| *sample == [0.; 2]));
    assert!(
        authored[quiet]
            .iter()
            .flatten()
            .any(|value| value.abs() > 0.001),
        "the retained sound allowance must contribute real PCM"
    );
    let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
        unreachable!()
    };
    for index in 0..=children.len() {
        let after = paste(&before, &slice, index);
        assert_eq!(complete(&after, &mut provider, false), raw);
        assert_eq!(complete(&after, &mut provider, true), authored);
        shuffled(&after, &mut provider, &raw, false);
        shuffled(&after, &mut provider, &authored, true);
    }
}
