//! J-cuts and L-cuts as one Roll plus one picture-keeping cutaway.

use deadpan_core::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::num::NonZeroU32;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}

/// A 100-frame Original at 30 fps with a 1/30 time base: tick = frame.
fn picture() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 100,
            time_base,
        },
    )
    .unwrap()
}

/// A 20-frame beat showing Original frames [first, first + 20).
fn beat(first: i64) -> BeatNode {
    let window = SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(20)).unwrap();
    BeatNode {
        label: format!("From {first}"),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(20),
                edit_window: Some(window),
                video: SourceVideo::Stream {
                    asset: AssetId::new("original").unwrap(),
                    span: picture(),
                },
                video_mapping: SourceVideoMapping::SelectedPlacement {
                    start: ExactRatio::integer(-first),
                    frames: ExactRatio::integer(100),
                    selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(20))
                        .unwrap(),
                    endpoints: EndpointPolicy::HoldAdjacent,
                },
                audio: None,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn document() -> ProjectDocument {
    let asset = AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(picture()),
        audio: None,
        still_image: false,
        frame_count: Some(frames(100)),
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    };
    ProjectDocument::from_json(&json!({
        "schema_version":DOCUMENT_SCHEMA_VERSION,"project_id":"split","revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root","marks":{},"overrides":{},
        "assets":{"original":asset},
        "nodes":{"root":BeatNode::sequence("Root",vec![id("left"),id("right")]),"left":beat(10),"right":beat(50)},
    }).to_string())
    .unwrap()
}

fn allocate(request: SemanticAllocationRequest) -> Result<SemanticAllocation, EditError> {
    Ok(match request {
        SemanticAllocationRequest::Roll {
            step_index,
            needs_wrapper,
        } => SemanticAllocation::Roll {
            new_revision: RevisionId::new(format!("leaf-{step_index}")).unwrap(),
            wrapper: needs_wrapper.then(|| id(&format!("wrapper-{step_index}"))),
        },
        SemanticAllocationRequest::ParameterEdit { step_index } => {
            SemanticAllocation::ParameterEdit {
                new_revision: RevisionId::new(format!("leaf-{step_index}")).unwrap(),
            }
        }
        other => panic!("unexpected allocation {other:?}"),
    })
}

fn plan(kind: SplitEditKind, length: u32, cursor: i64) -> Result<SemanticPlan, EditError> {
    let document = document();
    plan_semantic(
        &document,
        &SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            selected_hold_duration: None,
            parent: id("root"),
            cursor: ProjectFrame(cursor),
            selected_child: None,
            visual_selection: None,
        },
        &SemanticProgram::new(vec![SemanticInstruction::SplitEdit {
            kind,
            length: PauseLength::Frames {
                frames: NonZeroU32::new(length).unwrap(),
            },
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 1,
        },
        RevisionId::new("outer").unwrap(),
        allocate,
        |_, _| panic!("no register is read"),
    )
}

/// The source tick (here equal to the Original frame) a child's own picture
/// shows at its local frame, from its retained affine mapping.
fn shown(document: &ProjectDocument, node: &NodeId, local: i64) -> i64 {
    let (host, offset) = cutaway_host(document, node).unwrap();
    let local = local + offset;
    let beat = &document.nodes()[&host];
    if let Some(cutaway) = beat
        .cutaways
        .iter()
        .find(|cutaway| (cutaway.range.start().0..cutaway.range.end().0).contains(&local))
    {
        let point = cutaway
            .picture_point(
                ExactRatio::new(2 * i128::from(local) + 1, 2).unwrap(),
                document.presentation_basis().frame_rate,
            )
            .unwrap()
            .unwrap();
        return point.ticks.floor() as i64;
    }
    let NodeKind::Source { source } = &beat.kind else {
        panic!("not a source")
    };
    let start = source.video_mapping.start_frames();
    ExactRatio::integer(local)
        .checked_sub(start)
        .unwrap()
        .floor() as i64
}

fn children(document: &ProjectDocument) -> Vec<(NodeId, i64)> {
    let NodeKind::Sequence { children } = &document.nodes()[&id("root")].kind else {
        panic!()
    };
    children
        .iter()
        .map(|child| {
            let frames = match &document.nodes()[child].kind {
                NodeKind::Source { source } => source.duration.frames(),
                NodeKind::Retime { duration, .. } => duration.frames(),
                _ => panic!(),
            };
            (child.clone(), frames)
        })
        .collect()
}

/// The Original frame shown at each Edit frame, through children and cutaways.
fn pictures(document: &ProjectDocument) -> Vec<i64> {
    let mut result = Vec::new();
    for (child, length) in children(document) {
        for local in 0..length {
            result.push(shown(document, &child, local));
        }
    }
    result
}

#[test]
fn j_and_l_cuts_move_the_sound_cut_and_keep_every_picture_and_the_duration() {
    let before = document();
    let original = pictures(&before);
    assert_eq!(original[19], 29);
    assert_eq!(original[20], 50);
    for (kind, left, right) in [(SplitEditKind::J, 14, 26), (SplitEditKind::L, 26, 14)] {
        let planned = plan(kind, 6, 20).unwrap();
        let after = &planned.document;
        // The linked beats (and so their sound) moved by six frames...
        let lengths: Vec<_> = children(after).into_iter().map(|(_, n)| n).collect();
        assert_eq!(lengths, vec![left, right], "{kind:?}");
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
        // ...while every picture, including the cut at frame 20, is unchanged.
        assert_eq!(pictures(after), original, "{kind:?}");
        assert_eq!(planned.context.cursor, ProjectFrame(20));
        let replay =
            replay_compound::<EditError>(&before, planned.request.as_ref().unwrap(), |_| Ok(()))
                .unwrap();
        assert_eq!(&replay.document, after);
        assert_eq!(replay.edit.inverse.apply(&replay.document).unwrap(), before);
    }
}

#[test]
fn split_edits_refuse_without_a_seam_handles_or_whole_length() {
    // Not on a cut between two beats.
    assert!(plan(SplitEditKind::J, 6, 12).is_err());
    assert!(plan(SplitEditKind::L, 6, 40).is_err());
    // The left beat cannot give up all of its 20 frames.
    let error = plan(SplitEditKind::J, 20, 20).unwrap_err();
    assert!(error.message.contains("at most"), "{error:?}");
    // The right beat starts at Original 50, with 50 frames of handle before it.
    assert!(plan(SplitEditKind::J, 19, 20).is_ok());
}

fn plan_role(role: MediaRole, anchor: i64, head: i64) -> Result<SemanticPlan, EditError> {
    let document = document();
    plan_semantic(
        &document,
        &SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            selected_hold_duration: None,
            parent: id("root"),
            cursor: ProjectFrame(head),
            selected_child: None,
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(anchor),
                head: ProjectFrame(head),
                extending: false,
            }),
        },
        &SemanticProgram::new(vec![SemanticInstruction::DeleteRole { role }]).unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 1,
        },
        RevisionId::new("outer").unwrap(),
        allocate,
        |_, _| panic!("no register is read"),
    )
}

#[test]
fn role_only_deletes_keep_time_and_the_other_role_within_and_across_beats() {
    let before = document();
    // Audio: a mute range of the beat over Edit [24, 30) = right-local [4, 10).
    let audio = plan_role(MediaRole::Audio, 24, 30).unwrap();
    let right = &audio.document.nodes()[&id("right")];
    let ranges = right.audio_treatments.clip_gain().unwrap().mute_ranges();
    assert_eq!(ranges.len(), 1);
    assert_eq!(
        (ranges[0].start(), ranges[0].end()),
        (ExactRatio::integer(4), ExactRatio::integer(10))
    );
    assert!(right.cutaways.is_empty());
    assert_eq!(
        audio.document.duration().unwrap(),
        before.duration().unwrap()
    );
    assert_eq!(audio.context.visual_selection, None);
    // Video: a removed-picture cutaway recording Original 54..60, sound kept.
    let video = plan_role(MediaRole::Video, 30, 24).unwrap();
    let right = &video.document.nodes()[&id("right")];
    assert!(right.audio_treatments.is_empty());
    assert_eq!(right.cutaways.len(), 1);
    let removed = &right.cutaways[0];
    assert!(removed.removed);
    assert_eq!((removed.range.start().0, removed.range.end().0), (4, 10));
    assert_eq!(
        (
            removed.selection.start().ticks,
            removed.selection.end().ticks
        ),
        (ExactRatio::integer(54), ExactRatio::integer(60))
    );
    assert_eq!(pictures(&before), pictures_without_removed(&video.document));
    let json = serde_json::to_value(removed).unwrap();
    assert_eq!(json["removed"], true);
    for planned in [&audio, &video] {
        let replay =
            replay_compound::<EditError>(&before, planned.request.as_ref().unwrap(), |_| Ok(()))
                .unwrap();
        assert_eq!(replay.document, planned.document);
        assert_eq!(replay.edit.inverse.apply(&replay.document).unwrap(), before);
    }
    // Across the cut at 20, each beat loses that role over its own part, in
    // one transaction; time and the other role stay.
    let across = plan_role(MediaRole::Audio, 15, 25).unwrap();
    for (node, expected) in [("left", (15, 20)), ("right", (0, 5))] {
        let ranges = across.document.nodes()[&id(node)]
            .audio_treatments
            .clip_gain()
            .unwrap()
            .mute_ranges()
            .to_vec();
        assert_eq!(
            ranges
                .iter()
                .map(|range| (range.start(), range.end()))
                .collect::<Vec<_>>(),
            vec![(
                ExactRatio::integer(expected.0),
                ExactRatio::integer(expected.1)
            )],
            "{node}"
        );
    }
    let blank = plan_role(MediaRole::Video, 25, 15).unwrap();
    for (node, expected, original) in [("left", (15, 20), (25, 30)), ("right", (0, 5), (50, 55))] {
        let cutaways = &blank.document.nodes()[&id(node)].cutaways;
        assert_eq!(cutaways.len(), 1, "{node}");
        assert!(cutaways[0].removed);
        assert_eq!(
            (cutaways[0].range.start().0, cutaways[0].range.end().0),
            expected
        );
        assert_eq!(
            (
                cutaways[0].selection.start().ticks,
                cutaways[0].selection.end().ticks
            ),
            (
                ExactRatio::integer(original.0),
                ExactRatio::integer(original.1)
            )
        );
    }
    assert_eq!(pictures(&before), pictures_without_removed(&blank.document));
    for planned in [&across, &blank] {
        assert_eq!(
            planned.document.duration().unwrap(),
            before.duration().unwrap()
        );
        let replay =
            replay_compound::<EditError>(&before, planned.request.as_ref().unwrap(), |_| Ok(()))
                .unwrap();
        assert_eq!(replay.edit.inverse.apply(&replay.document).unwrap(), before);
    }
    // Linked or empty: refused.
    assert!(plan_role(MediaRole::Linked, 24, 30).is_err());
    assert!(plan_role(MediaRole::Video, 24, 24).is_err());
}

/// Pictures with removed ranges read as their recorded Original frames, so
/// the comparison shows nothing else moved.
fn pictures_without_removed(document: &ProjectDocument) -> Vec<i64> {
    let mut wire = serde_json::to_value(document).unwrap();
    for node in wire["nodes"].as_object_mut().unwrap().values_mut() {
        if let Some(cutaways) = node.get_mut("cutaways") {
            cutaways
                .as_array_mut()
                .unwrap()
                .retain(|cutaway| cutaway.get("removed").is_none());
        }
    }
    pictures(&ProjectDocument::from_json(&wire.to_string()).unwrap())
}

/// The two-beat fixture with linked 48 kHz sound on both beats.
fn with_sound() -> ProjectDocument {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let sound = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 160_000,
            time_base,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(document()).unwrap();
    wire["assets"]["original"]["audio"] = serde_json::to_value(sound).unwrap();
    for (node, first) in [("left", 10), ("right", 50)] {
        let source = &mut wire["nodes"][node]["kind"]["source"];
        source["audio"] = json!({"asset":"original","span":sound});
        source["audio_mapping"] = serde_json::to_value(SourceAudioMapping::Placement {
            start: ExactRatio::integer(-first),
            frames: ExactRatio::integer(100),
        })
        .unwrap();
        source["link"] = json!("linked");
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn plan_repeat(
    document: &ProjectDocument,
    role: MediaRole,
    plays: u32,
    trim: bool,
) -> Result<SemanticPlan, EditError> {
    plan_repeat_with(document, role, plays, trim, false)
}

fn plan_repeat_with(
    document: &ProjectDocument,
    role: MediaRole,
    plays: u32,
    trim: bool,
    extend: bool,
) -> Result<SemanticPlan, EditError> {
    plan_repeat_over(document, role, plays, trim, extend, 24, 30)
}

fn plan_repeat_over(
    document: &ProjectDocument,
    role: MediaRole,
    plays: u32,
    trim: bool,
    extend: bool,
    anchor: i64,
    head: i64,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_speech(
        document,
        &SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            selected_hold_duration: None,
            parent: id("root"),
            cursor: ProjectFrame(head),
            selected_child: None,
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(anchor),
                head: ProjectFrame(head),
                extending: false,
            }),
        },
        &SemanticProgram::new(vec![SemanticInstruction::RoleRepeat {
            role,
            plays: NonZeroU32::new(plays).unwrap(),
            trim,
            extend,
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 1,
        },
        RevisionId::new("outer").unwrap(),
        |request| match request {
            SemanticAllocationRequest::Sound { step_index } => Ok(SemanticAllocation::Sound {
                new_revision: RevisionId::new(format!("leaf-{step_index}")).unwrap(),
                id: SoundId::new(format!("play-{step_index}")).unwrap(),
            }),
            SemanticAllocationRequest::InsertPause {
                step_index,
                required_split_ids,
            } => Ok(SemanticAllocation::InsertPause {
                new_revision: RevisionId::new(format!("leaf-{step_index}")).unwrap(),
                id: id(&format!("held-{step_index}")),
                split: SplitIdentities {
                    nodes: (0..required_split_ids)
                        .map(|n| id(&format!("held-split-{step_index}-{n}")))
                        .collect(),
                },
            }),
            other => allocate(other),
        },
        |_, _| panic!("no register is read"),
        |_| Err(speech_unavailable()),
        |_, _| {
            Ok(PauseProvider {
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            })
        },
    )
}

#[test]
fn an_audio_repeat_with_extend_hold_adds_exactly_its_overflow_and_permits_it() {
    let before = with_sound();
    // Three plays of Edit [24, 30) need [24, 42); the beat ends at 40.
    let audio = plan_repeat_with(&before, MediaRole::Audio, 3, false, true).unwrap();
    let after = &audio.document;
    assert_eq!(
        after.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 2
    );
    // A two-frame silent pause follows the beat.
    let NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        panic!()
    };
    let right = children
        .iter()
        .position(|child| child == &id("right"))
        .unwrap();
    let pause = &children[right + 1];
    let NodeKind::Hold { recipe } = &after.nodes()[pause].kind else {
        panic!("expected the extension pause after the beat")
    };
    assert_eq!(recipe.duration.frames(), 2);
    // Two repeated sounds; only the one reaching the pause may sound over it.
    assert_eq!(after.sounds().len(), 2);
    let allowed: Vec<_> = after.sound_allowances().iter().collect();
    assert_eq!(allowed.len(), 1, "{allowed:?}");
    // Video repeats and two policies at once refuse.
    assert!(plan_repeat_with(&before, MediaRole::Video, 3, false, true).is_err());
    assert!(
        SemanticProgram::new(vec![SemanticInstruction::RoleRepeat {
            role: MediaRole::Audio,
            plays: NonZeroU32::new(3).unwrap(),
            trim: true,
            extend: true,
        }])
        .is_err()
    );
    let replay =
        replay_compound::<EditError>(&before, audio.request.as_ref().unwrap(), |_| Ok(())).unwrap();
    assert_eq!(&replay.document, after);
    assert_eq!(replay.edit.inverse.apply(&replay.document).unwrap(), before);
}

#[test]
fn role_repeats_play_one_role_again_without_inserting_time() {
    let before = with_sound();
    // Three plays of Edit [24, 30) need [24, 42); the beat ends at 40.
    let error = plan_repeat(&before, MediaRole::Audio, 3, false).unwrap_err();
    assert!(error.message.contains("overflow=trim"), "{error:?}");
    assert!(plan_repeat(&before, MediaRole::Video, 3, false).is_err());
    assert!(plan_repeat(&before, MediaRole::Linked, 3, true).is_err());

    // Audio: plays two and three are root sounds of the same Original sound
    // at Edit 30 and 36 (the last trimmed at the beat's end), and the host's
    // own sound is muted under them.
    let audio = plan_repeat(&before, MediaRole::Audio, 3, true).unwrap();
    let after = &audio.document;
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    let ranges = after.nodes()[&id("right")]
        .audio_treatments
        .clip_gain()
        .unwrap()
        .mute_ranges()
        .to_vec();
    assert_eq!(
        ranges
            .iter()
            .map(|range| (range.start(), range.end()))
            .collect::<Vec<_>>(),
        vec![(ExactRatio::integer(10), ExactRatio::integer(20))]
    );
    // Root sound events, which follow root-clock edits.
    assert!(after.beat_sounds().is_empty());
    let heard: Vec<_> = after
        .sounds()
        .values()
        .map(|event| {
            let selected = event
                .mapping
                .selection_frames_with_offset(
                    FrameDuration::ZERO,
                    event.offset,
                    FrameRate::new(30, 1).unwrap(),
                )
                .unwrap();
            // Root frames.
            let start = selected.start.floor() as i64;
            (start, selected.end.floor() as i64, event.label.clone())
        })
        .collect();
    assert_eq!(
        heard.iter().map(|(a, b, _)| (*a, *b)).collect::<Vec<_>>(),
        vec![(30, 36), (36, 40)]
    );
    assert!(heard[0].2.starts_with("Repeat 2 of 3"));
    for event in after.sounds().values() {
        let NodeKind::Source { source } = &before.nodes()[&id("right")].kind else {
            panic!()
        };
        assert_eq!(Some(&event.source), source.audio.as_ref());
        // The mapping keeps the host's exact affine phase: Original frame 54
        // (the range's first frame) sounds at each play's onset.
        let SourceAudioMapping::SelectedPlacement {
            start, selection, ..
        } = event.mapping
        else {
            panic!()
        };
        assert_eq!(
            selection.start.checked_sub(start).unwrap(),
            ExactRatio::integer(54)
        );
    }

    // Video: a looping cutaway over the later plays shows the range's
    // pictures again; the sound is untouched.
    let video = plan_repeat(&before, MediaRole::Video, 3, true).unwrap();
    let pictures = pictures(&video.document);
    assert_eq!(&pictures[24..30], &[54, 55, 56, 57, 58, 59]);
    assert_eq!(&pictures[30..36], &[54, 55, 56, 57, 58, 59]);
    assert_eq!(&pictures[36..40], &[54, 55, 56, 57]);
    assert!(video.document.sounds().is_empty());
    assert!(
        video.document.nodes()[&id("right")]
            .audio_treatments
            .is_empty()
    );
    for planned in [&audio, &video] {
        let replay =
            replay_compound::<EditError>(&before, planned.request.as_ref().unwrap(), |_| Ok(()))
                .unwrap();
        assert_eq!(replay.document, planned.document);
        assert_eq!(replay.edit.inverse.apply(&replay.document).unwrap(), before);
    }
}

#[test]
fn split_edits_capture_clocks_beside_a_pitch_shifted_stage() {
    for (frames, semitones) in [(20, 3), (15, -2)] {
        let mut wire = serde_json::to_value(with_sound()).unwrap();
        wire["nodes"]["inner"] = wire["nodes"]["left"].clone();
        // The rolled pair keeps picture-only sources; the shifted stage has sound.
        let plain = serde_json::to_value(document()).unwrap();
        wire["nodes"]["left"] = plain["nodes"]["left"].clone();
        wire["nodes"]["right"] = plain["nodes"]["right"].clone();
        wire["nodes"]["shifted"] = json!({
            "label":"Shifted","kind":{"type":"retime","child":"inner","duration":frames,
            "mapping":{"start":0,"end":20},"pitch":{"shift":{"semitones":semitones}}}
        });
        wire["nodes"]["root"]["kind"]["children"] = json!(["left", "right", "shifted"]);
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let planned = plan_semantic(
            &document,
            &SemanticContext {
                original: None,
                scoped: None,
                hold_duration: None,
                shared_hold_duration: None,
                mixed_hold_duration: None,
                selected_hold_duration: None,
                parent: id("root"),
                cursor: ProjectFrame(20),
                selected_child: None,
                visual_selection: None,
            },
            &SemanticProgram::new(vec![SemanticInstruction::SplitEdit {
                kind: SplitEditKind::J,
                length: PauseLength::Frames {
                    frames: NonZeroU32::new(6).unwrap(),
                },
            }])
            .unwrap(),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 1,
            },
            RevisionId::new("outer").unwrap(),
            allocate,
            |_, _| panic!("no register is read"),
        )
        .unwrap();
        assert!(
            !planned.document.audio_bindings().bindings().is_empty(),
            "the Roll captured clocks"
        );
        let replay =
            replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| Ok(()))
                .unwrap();
        assert_eq!(
            replay.edit.inverse.apply(&replay.document).unwrap(),
            document
        );
    }
}

#[test]
fn audio_repeats_follow_root_edits_and_refuse_captures_that_would_leave_them_behind() {
    let before = with_sound();
    let after = plan_repeat(&before, MediaRole::Audio, 3, true)
        .unwrap()
        .document;
    let heard = |document: &ProjectDocument| -> Vec<(i64, i64)> {
        document
            .sounds()
            .values()
            .map(|event| {
                let selected = event
                    .mapping
                    .selection_frames_with_offset(
                        FrameDuration::ZERO,
                        event.offset,
                        FrameRate::new(30, 1).unwrap(),
                    )
                    .unwrap();
                (selected.start.floor() as i64, selected.end.floor() as i64)
            })
            .collect()
    };
    let capture = |node: &str| {
        CapturedEditSlice::capture_selection(
            &after,
            &id("root"),
            &SliceCaptureSelection::Child { node: id(node) },
            AudioTimingId {
                allocation: RevisionId::new("capture").unwrap(),
                ordinal: 0,
            },
        )
    };
    // Copying the muted beat would leave its repeats behind: refused.
    let error = capture("right").unwrap_err();
    assert!(
        error.message.contains("repeated from the Original"),
        "{error:?}"
    );
    // A copy that does not meet the repeats is unaffected.
    assert!(capture("left").is_ok());
    let request = |command: Command, revision: &str| CommandRequest {
        project_id: after.project_id().clone(),
        expected_revision: after.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    };
    // A ripple delete before the beat moves its repeats with it, and keeps
    // the mute.
    let deleted = apply(
        &after,
        &request(
            Command::DeleteRipple {
                node: id("left"),
                timing: AudioTimingId {
                    allocation: RevisionId::new("deleted").unwrap(),
                    ordinal: 0,
                },
            },
            "deleted",
        ),
    )
    .unwrap()
    .forward
    .apply(&after)
    .unwrap();
    // The recipes keep their authored clock; a root route records the
    // 20-frame ripple that moves them (rendering follows it).
    assert_eq!(heard(&deleted), heard(&after));
    assert_eq!(deleted.sounds().len(), 2);
    for id in deleted.sounds().keys() {
        assert!(deleted.sound_routes().contains_key(id), "{id:?} is routed");
    }
    assert_eq!(
        deleted.nodes()[&id("right")].audio_treatments,
        after.nodes()[&id("right")].audio_treatments
    );
    // Later edits still work: a Split of the muted beat keeps both repeats.
    let split = apply(
        &after,
        &request(
            Command::Split {
                node: id("right"),
                at: frames(5),
                identities: SplitIdentities {
                    nodes: (0..8).map(|n| id(&format!("split-{n}"))).collect(),
                },
            },
            "split",
        ),
    )
    .unwrap()
    .forward
    .apply(&after)
    .unwrap();
    assert_eq!(heard(&split), heard(&after));
}

fn plan_shift(
    document: &ProjectDocument,
    cursor: i64,
    frames: i64,
    trim: bool,
    extend: bool,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_speech(
        document,
        &SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            selected_hold_duration: None,
            parent: id("root"),
            cursor: ProjectFrame(cursor),
            selected_child: None,
            visual_selection: None,
        },
        &SemanticProgram::new(vec![SemanticInstruction::AudioShift {
            frames,
            trim,
            extend,
        }])?,
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 1,
        },
        RevisionId::new("outer").unwrap(),
        |request| match request {
            SemanticAllocationRequest::Sound { step_index } => Ok(SemanticAllocation::Sound {
                new_revision: RevisionId::new(format!("leaf-{step_index}")).unwrap(),
                id: SoundId::new(format!("shift-{step_index}")).unwrap(),
            }),
            SemanticAllocationRequest::InsertPause {
                step_index,
                required_split_ids,
            } => Ok(SemanticAllocation::InsertPause {
                new_revision: RevisionId::new(format!("leaf-{step_index}")).unwrap(),
                id: id(&format!("held-{step_index}")),
                split: SplitIdentities {
                    nodes: (0..required_split_ids)
                        .map(|n| id(&format!("held-split-{step_index}-{n}")))
                        .collect(),
                },
            }),
            other => allocate(other),
        },
        |_, _| panic!("no register is read"),
        |_| Err(speech_unavailable()),
        |_, _| {
            Ok(PauseProvider {
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            })
        },
    )
}

/// Each shifted sound as (first root frame, end root frame, Original frame at
/// its first root frame), in onset order.
fn shifted(document: &ProjectDocument) -> Vec<(i64, i64, i64)> {
    let mut heard: Vec<_> = document
        .sounds()
        .values()
        .map(|event| {
            let selected = event
                .mapping
                .selection_frames_with_offset(
                    FrameDuration::ZERO,
                    event.offset,
                    FrameRate::new(30, 1).unwrap(),
                )
                .unwrap();
            let SourceAudioMapping::SelectedPlacement {
                start, selection, ..
            } = event.mapping
            else {
                panic!("a shifted sound keeps its selected placement")
            };
            let original = selection.start.checked_sub(start).unwrap();
            (
                selected.start.floor() as i64,
                selected.end.floor() as i64,
                original.floor() as i64,
            )
        })
        .collect();
    heard.sort();
    heard
}

fn mutes(document: &ProjectDocument, node: &str) -> Vec<(i64, i64)> {
    document.nodes()[&id(node)]
        .audio_treatments
        .clip_gain()
        .map(|clip| {
            clip.mute_ranges()
                .iter()
                .map(|range| (range.start().floor() as i64, range.end().floor() as i64))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_audio_shift_slides_sound_across_cuts_and_keeps_every_picture() {
    let before = with_sound();
    let pictures_before = pictures(&before);

    // Forward: sound from Edit 15 plays 3 frames later, across the cut at 20.
    // Edit 15 is Original 25 in "left"; Edit 20 is Original 50 in "right".
    let error = plan_shift(&before, 15, 3, false, false).unwrap_err();
    assert!(error.message.contains("overflow=trim"), "{error:?}");
    let forward = plan_shift(&before, 15, 3, true, false).unwrap().document;
    assert_eq!(forward.duration().unwrap(), before.duration().unwrap());
    assert_eq!(pictures(&forward), pictures_before);
    assert_eq!(mutes(&forward, "left"), vec![(15, 20)]);
    assert_eq!(mutes(&forward, "right"), vec![(0, 20)]);
    assert_eq!(shifted(&forward), vec![(18, 23, 25), (23, 40, 50)]);

    // Backward: the sound slides 4 frames earlier over Edit 11..15, which is
    // muted too; nothing passes the end.
    let backward = plan_shift(&before, 15, -4, false, false).unwrap().document;
    assert_eq!(pictures(&backward), pictures_before);
    assert_eq!(mutes(&backward, "left"), vec![(11, 20)]);
    assert_eq!(shifted(&backward), vec![(11, 16, 25), (16, 36, 50)]);

    // extend=hold holds the last picture for exactly the shift and lets the
    // sound that reaches it play over the pause.
    let held = plan_shift(&before, 15, 3, false, true).unwrap().document;
    assert_eq!(
        held.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 3
    );
    assert_eq!(shifted(&held), vec![(18, 23, 25), (23, 43, 50)]);
    let allowed: Vec<_> = held.sound_allowances().iter().collect();
    assert_eq!(allowed.len(), 1, "{allowed:?}");

    for (cursor, frames, trim, extend, message) in [
        (15, 0, true, false, "zero audio shift"),
        (15, 3, true, true, "one overflow policy"),
        (2, -4, false, false, "before this group starts"),
        (40, 2, true, false, "at the end of its group"),
    ] {
        let error = plan_shift(&before, cursor, frames, trim, extend).unwrap_err();
        assert!(error.message.contains(message), "{message}: {error:?}");
    }
}

#[test]
fn an_audio_repeat_spans_cuts_and_plays_each_beats_sound_again() {
    let before = with_sound();
    let pictures_before = pictures(&before);
    // Edit [16, 24) is Original 26..30 in "left" and 50..54 in "right"; three
    // plays fill [16, 40), exactly to the end of "right".
    let audio = plan_repeat_over(&before, MediaRole::Audio, 3, false, false, 16, 24).unwrap();
    let after = &audio.document;
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(pictures(after), pictures_before);
    assert_eq!(mutes(after, "left"), Vec::<(i64, i64)>::new());
    assert_eq!(mutes(after, "right"), vec![(4, 20)]);
    assert_eq!(
        shifted(after),
        vec![(24, 28, 26), (28, 32, 50), (32, 36, 26), (36, 40, 50)]
    );
    assert!(
        after
            .sounds()
            .values()
            .any(|event| event.label.starts_with("Repeat 3 of 3"))
    );
    let replay =
        replay_compound::<EditError>(&before, audio.request.as_ref().unwrap(), |_| Ok(())).unwrap();
    assert_eq!(&replay.document, after);
    assert_eq!(replay.edit.inverse.apply(&replay.document).unwrap(), before);

    // A fourth play passes the last beat: refuse, trim it away, or hold.
    let error = plan_repeat_over(&before, MediaRole::Audio, 4, false, false, 16, 24).unwrap_err();
    assert!(error.message.contains("overflow=trim"), "{error:?}");
    let trimmed = plan_repeat_over(&before, MediaRole::Audio, 4, true, false, 16, 24)
        .unwrap()
        .document;
    assert_eq!(shifted(&trimmed), shifted(after));
    let held = plan_repeat_over(&before, MediaRole::Audio, 4, false, true, 16, 24)
        .unwrap()
        .document;
    assert_eq!(
        held.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 8
    );
    assert_eq!(
        shifted(&held),
        vec![
            (24, 28, 26),
            (28, 32, 50),
            (32, 36, 26),
            (36, 40, 50),
            (40, 44, 26),
            (44, 48, 50)
        ]
    );
    assert_eq!(held.sound_allowances().iter().count(), 2);

    // Sound already muted in the range would sound again: refuse.
    let mut wire = serde_json::to_value(after).unwrap();
    wire["revision_id"] = json!("muted");
    let muted = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let error = plan_repeat_over(&muted, MediaRole::Audio, 2, true, false, 16, 26).unwrap_err();
    assert!(error.message.contains("muted"), "{error:?}");

    // Split fragments of "right" continue one sound: each play has one event
    // for them, so no fade dips at the Split seam.
    let split = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new("split").unwrap(),
            command: Command::Split {
                node: id("right"),
                at: frames(5),
                identities: SplitIdentities {
                    nodes: (0..8).map(|n| id(&format!("split-{n}"))).collect(),
                },
            },
        },
    )
    .unwrap()
    .forward
    .apply(&before)
    .unwrap();
    let merged = plan_repeat_over(&split, MediaRole::Audio, 2, false, false, 16, 28)
        .unwrap()
        .document;
    assert_eq!(shifted(&merged), vec![(28, 32, 26), (32, 40, 50)]);

    // A video-only repeat still stays inside one beat.
    let error = plan_repeat_over(&before, MediaRole::Video, 2, true, false, 16, 24).unwrap_err();
    assert!(error.message.contains("one beat"), "{error:?}");
}

fn plan_role_on(
    document: &ProjectDocument,
    role: MediaRole,
    anchor: i64,
    head: i64,
) -> Result<SemanticPlan, EditError> {
    plan_semantic(
        document,
        &SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            selected_hold_duration: None,
            parent: id("root"),
            cursor: ProjectFrame(head),
            selected_child: None,
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(anchor),
                head: ProjectFrame(head),
                extending: false,
            }),
        },
        &SemanticProgram::new(vec![SemanticInstruction::DeleteRole { role }]).unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 1,
        },
        RevisionId::new(format!("{}-next", document.revision_id())).unwrap(),
        allocate,
        |_, _| panic!("no register is read"),
    )
}

/// "right" (Edit 20..40, Original 50..69) with a hold cutaway of Original
/// 80..90 over its local frames 2..12.
fn with_cutaway(fit: CutawayFit) -> ProjectDocument {
    let mut document = document();
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    let point = |ticks: i64| SourcePoint {
        ticks: ExactRatio::integer(ticks),
        time_base,
    };
    let cutaway = Cutaway {
        range: FrameRange::new(ProjectFrame(2), ProjectFrame(12)).unwrap(),
        asset: AssetId::new("original").unwrap(),
        selection: ExactSourceSpan::new(point(80), point(90)).unwrap(),
        fit,
        removed: false,
    };
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"]["right"]["cutaways"] = serde_json::to_value(vec![cutaway]).unwrap();
    document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    document
}

#[test]
fn a_video_delete_removes_the_top_picture_layer_first() {
    let before = with_cutaway(CutawayFit::Hold);
    let shown_before = pictures(&before);
    assert_eq!(
        shown_before[22..32],
        [80, 81, 82, 83, 84, 85, 86, 87, 88, 89]
    );

    // Inside the cutaway: its part is trimmed and the beat's own pictures show
    // again (Original 54..57); both outer pieces show exactly what they did.
    let inner = plan_role_on(&before, MediaRole::Video, 24, 28)
        .unwrap()
        .document;
    let mut expected = shown_before.clone();
    expected[24..28].copy_from_slice(&[54, 55, 56, 57]);
    assert_eq!(pictures(&inner), expected);
    assert!(
        inner.nodes()[&id("right")]
            .cutaways
            .iter()
            .all(|cutaway| !cutaway.removed)
    );
    assert_eq!(inner.duration().unwrap(), before.duration().unwrap());

    // Across the cutaway's start: the uncovered frames lose the beat's picture
    // to the background, the covered ones show the beat, the rest is kept.
    let across = plan_role_on(&before, MediaRole::Video, 20, 26)
        .unwrap()
        .document;
    let cutaways = &across.nodes()[&id("right")].cutaways;
    assert_eq!(
        cutaways
            .iter()
            .map(|cutaway| (
                cutaway.range.start().0,
                cutaway.range.end().0,
                cutaway.removed
            ))
            .collect::<Vec<_>>(),
        vec![(0, 2, true), (6, 12, false)]
    );
    let shown = pictures_without_removed(&across);
    assert_eq!(
        shown[20..32],
        [50, 51, 52, 53, 54, 55, 84, 85, 86, 87, 88, 89]
    );

    // The beat's own picture is now on top there; deleting again reaches the
    // background, and an already removed range refuses.
    let twice = plan_role_on(&inner, MediaRole::Video, 24, 28)
        .unwrap()
        .document;
    assert!(
        twice.nodes()[&id("right")]
            .cutaways
            .iter()
            .any(|cutaway| cutaway.removed && cutaway.range.start().0 == 4)
    );
    assert!(plan_role_on(&twice, MediaRole::Video, 24, 28).is_err());

    // A looping cutaway cannot start mid-cycle, but loses its end exactly.
    let looping = with_cutaway(CutawayFit::Loop);
    assert!(plan_role_on(&looping, MediaRole::Video, 24, 28).is_err());
    let tail = plan_role_on(&looping, MediaRole::Video, 28, 32)
        .unwrap()
        .document;
    assert_eq!(pictures(&tail)[22..28], pictures(&looping)[22..28]);
    assert_eq!(pictures(&tail)[28..32], [58, 59, 60, 61]);
}
