//! The exact sibling reducer must preserve the same sample lattice and root
//! sound route as one atomic timing operation, including an empty side.
use super::*;

fn copied_child(document: &ProjectDocument, parent: &str, child: &str) -> CapturedEditSlice {
    CapturedEditSlice::capture_selection(
        document,
        &id(parent),
        &SliceCaptureSelection::Child { node: id(child) },
        AudioTimingId {
            allocation: revision("children-capture"),
            ordinal: 0,
        },
    )
    .unwrap()
}

fn identities(slice: &CapturedEditSlice, name: &str) -> SlicePasteIdentities {
    let required = slice.identity_requirements().unwrap();
    SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..required.nodes)
                .map(|index| id(&format!("{name}-node-{index}")))
                .collect(),
            marks: (0..required.marks)
                .map(|index| MarkId::new(format!("{name}-mark-{index}")).unwrap())
                .collect(),
        },
        aliases: (0..required.aliases)
            .map(|index| id(&format!("{name}-alias-{index}")))
            .collect(),
    }
}

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}

fn sounded(document: &ProjectDocument, start: i64, name: &str) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    let qualified = ProjectDocument::from_json(&wire.to_string()).unwrap();
    edit(
        &qualified,
        name,
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event: SoundEvent {
                owner: id("root"),
                label: "Root sound across replacement".into(),
                source: audio(100..20_100),
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::integer(start),
                    frames: ratio(2_000_000, 147_147),
                    selection: ExactFrameRange::new(
                        ExactRatio::integer(start),
                        ExactRatio::integer(start + 1),
                    )
                    .unwrap(),
                },
                offset: AudioSample(7),
                gain_millidecibels: -6000,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    )
}

fn authored(document: &ProjectDocument, provider: &mut Provider) -> Vec<[f32; 2]> {
    let end = boundary(document.duration().unwrap().frames());
    let mut reader = renderer(document, provider);
    (0..end)
        .step_by(193)
        .flat_map(|at| {
            reader
                .prepare_authored_bus(
                    provider,
                    AudioSample(at),
                    u32::try_from((end - at).min(193)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        })
        .collect()
}

#[test]
fn nested_exact_children_replacement_keeps_suffix_and_root_sound_on_one_ntsc_clock() {
    // In each case, the two empty endpoint siblings are real selected owners.
    // The reference is one existing timing operation, not a delete-then-insert
    // composition whose sample rounding could mask a double transform.
    for (case, old_frames, new_frames) in [
        ("positive-empty", 3, 0),
        ("empty-positive", 0, 2),
        ("positive-positive", 3, 2),
    ] {
        let body = if old_frames == 0 {
            BeatNode::sequence("Empty body", vec![])
        } else {
            source(ntsc(), old_frames)
        };
        let original = document(
            ntsc(),
            &["lead", "group", "tail"],
            vec![
                ("lead", source(ntsc(), 1)),
                (
                    "group",
                    BeatNode::sequence("Nested", vec![id("left"), id("body"), id("right")]),
                ),
                ("left", BeatNode::sequence("Empty left", vec![])),
                ("body", body),
                ("right", BeatNode::sequence("Empty right", vec![])),
                ("tail", preserve("tail-source", 4, 5)),
                ("tail-source", source(ntsc(), 4)),
            ],
        );
        let before = sounded(&original, 2 + old_frames, &format!("{case}-sound"));
        let slice = if new_frames == 0 {
            copied_child(&before, "group", "left")
        } else {
            let donor = document(
                ntsc(),
                &["donor"],
                vec![("donor", source(ntsc(), new_frames))],
            );
            let mut wire = serde_json::to_value(donor).unwrap();
            wire["assets"]["media"]["source_qualification"] =
                serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
            let donor = ProjectDocument::from_json(&wire.to_string()).unwrap();
            copied_child(&donor, "root", "donor")
        };
        let target = before
            .slice_children_replacement(&id("group"), &id("left"), &id("right"), &slice)
            .unwrap();
        assert_eq!(target.range.start(), ProjectFrame(1));
        assert_eq!(target.range.end(), ProjectFrame(1 + old_frames));
        let after = edit(
            &before,
            &format!("{case}-exact"),
            Command::ReplaceSliceChildren {
                parent: id("group"),
                first: id("left"),
                last: id("right"),
                slice: slice.clone(),
                identities: identities(&slice, case),
                timing: timing(&format!("{case}-exact")),
            },
        );
        let reference_name = format!("{case}-reference");
        let reference_command = if new_frames == 0 {
            Command::DeleteChildren {
                parent: id("group"),
                first: id("left"),
                last: id("right"),
                timing: timing(&reference_name),
            }
        } else if old_frames == 0 {
            Command::SpliceSlice {
                parent: id("group"),
                index: 0,
                slice: slice.clone(),
                identities: identities(&slice, "single-splice"),
                timing: timing(&reference_name),
            }
        } else {
            Command::ReplaceSlice {
                parent: id("group"),
                range: FrameRange::new(ProjectFrame(1), ProjectFrame(1 + old_frames)).unwrap(),
                slice: slice.clone(),
                identities: identities(&slice, "single-replace"),
                split_identities: SplitIdentities { nodes: vec![] },
                timing: timing(&reference_name),
            }
        };
        let reference = edit(&before, &reference_name, reference_command);
        assert_eq!(after.duration(), reference.duration());
        assert_eq!(after.sounds(), reference.sounds(), "{case}");
        assert_eq!(after.sound_routes(), reference.sound_routes(), "{case}");
        assert!(!after.sound_routes().is_empty(), "{case}");

        let mut old_provider = Provider::new();
        let prefix = pcm(
            &before,
            &mut old_provider,
            0,
            usize::try_from(boundary(1)).unwrap(),
        );
        let suffix_start = boundary(1 + old_frames);
        let suffix_end = boundary(before.duration().unwrap().frames());
        let new_suffix_start = boundary(1 + new_frames);
        let new_suffix_end = boundary(after.duration().unwrap().frames());
        let suffix_count =
            usize::try_from((suffix_end - suffix_start).min(new_suffix_end - new_suffix_start))
                .unwrap();
        let suffix = pcm(&before, &mut old_provider, suffix_start, suffix_count);
        let mut new_provider = Provider::new();
        cold_reverse(&after, &mut new_provider, 0, &prefix);
        cold_reverse(&after, &mut new_provider, new_suffix_start, &suffix);

        let expected_bus = authored(&reference, &mut Provider::new());
        let actual_bus = authored(&after, &mut Provider::new());
        assert_eq!(actual_bus.len(), expected_bus.len(), "{case}");
        for (index, (actual, expected)) in actual_bus.iter().zip(expected_bus).enumerate() {
            assert_eq!(
                actual.map(f32::to_bits),
                expected.map(f32::to_bits),
                "{case}: authored sample {index}"
            );
        }
    }
}
