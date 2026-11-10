//! J- and L-cuts beside pauses and across group boundaries, and Slip and Trim
//! of a Source whose sound has an audio lag, at 30000/1001 fps.

use deadpan_core::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::num::NonZeroU32;

const TOTAL: i64 = 300;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}

fn rate() -> FrameRate {
    FrameRate::new(30_000, 1_001).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

/// 300 pictures with a 1001/30000 time base (one tick per picture).
fn picture() -> SourceSpan {
    let time_base = SourceTimeBase::new(1_001, 30_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: TOTAL,
            time_base,
        },
    )
    .unwrap()
}

/// The same 10.01 s of 48 kHz sound.
fn sound() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 480_480,
            time_base,
        },
    )
    .unwrap()
}

/// A linked beat showing and sounding Original frames [first, first + length).
fn beat(first: i64, length: i64) -> BeatNode {
    let selected = ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(length)).unwrap();
    BeatNode {
        label: format!("From {first}"),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(length),
                edit_window: Some(
                    SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(length)).unwrap(),
                ),
                video: SourceVideo::Stream {
                    asset: AssetId::new("original").unwrap(),
                    span: picture(),
                },
                video_mapping: SourceVideoMapping::SelectedPlacement {
                    start: ExactRatio::integer(-first),
                    frames: ExactRatio::integer(TOTAL),
                    selection: selected,
                    endpoints: EndpointPolicy::HoldAdjacent,
                },
                audio: Some(SourceAudio {
                    asset: AssetId::new("original").unwrap(),
                    span: sound(),
                }),
                audio_mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::integer(-first),
                    frames: ExactRatio::integer(TOTAL),
                    selection: selected,
                },
                audio_offset: AudioSample(0),
                link: LinkRelation::Linked,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn pause(length: i64, audio: HoldAudio) -> BeatNode {
    BeatNode {
        label: "Pause".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Hold {
            recipe: HoldRecipe {
                picture_context: None,
                duration: frames(length),
                video: HoldVideo::Background,
                audio,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn document(nodes: serde_json::Value) -> ProjectDocument {
    let asset = AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(picture()),
        audio: Some(sound()),
        still_image: false,
        frame_count: Some(frames(TOTAL)),
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    };
    ProjectDocument::from_json(&json!({
        "schema_version":DOCUMENT_SCHEMA_VERSION,"project_id":"roles","revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root","marks":{},"overrides":{},
        "assets":{"original":asset},
        "nodes":nodes,
    }).to_string())
    .unwrap()
}

/// Edit: a [0,30) Original 30..60, a silent pause [30,45), b [45,75)
/// Original 120..150, group g [75,115) of c (Original 200..220) and d
/// (Original 280..300, the end of the Original), then e [115,135) Original
/// 100..120.
fn edit() -> ProjectDocument {
    document(json!({
        "root":BeatNode::sequence("Root",vec![id("a"),id("hold"),id("b"),id("g"),id("e")]),
        "a":beat(30, 30),
        "hold":pause(15, HoldAudio::Silence),
        "b":beat(120, 30),
        "g":BeatNode::sequence("Group",vec![id("c"),id("d")]),
        "c":beat(200, 20),
        "d":beat(280, 20),
        "e":beat(100, 20),
    }))
}

/// A never-repeated revision, so plans can run on earlier plans' results.
fn fresh(step_index: usize) -> RevisionId {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    RevisionId::new(format!("leaf-{n}-{step_index}")).unwrap()
}

fn allocate(request: SemanticAllocationRequest) -> Result<SemanticAllocation, EditError> {
    Ok(match request {
        SemanticAllocationRequest::ParameterEdit { step_index } => {
            SemanticAllocation::ParameterEdit {
                new_revision: fresh(step_index),
            }
        }
        SemanticAllocationRequest::Sound { step_index } => SemanticAllocation::Sound {
            new_revision: fresh(step_index),
            id: SoundId::new(format!("sound-{step_index}")).unwrap(),
        },
        SemanticAllocationRequest::Roll {
            step_index,
            needs_wrapper,
        } => SemanticAllocation::Roll {
            new_revision: fresh(step_index),
            wrapper: needs_wrapper.then(|| id(&format!("wrapper-{step_index}"))),
        },
        SemanticAllocationRequest::Trim {
            step_index,
            target_wrapper,
            right_wrapper,
            split_nodes,
            filler_nodes,
        } => SemanticAllocation::Trim {
            new_revision: fresh(step_index),
            target_wrapper: target_wrapper.then(|| id(&format!("crop-{step_index}"))),
            right_wrapper: right_wrapper.then(|| id(&format!("right-crop-{step_index}"))),
            split: SplitIdentities {
                nodes: (0..split_nodes)
                    .map(|n| id(&format!("split-{step_index}-{n}")))
                    .collect(),
            },
            fillers: (0..filler_nodes)
                .map(|n| id(&format!("filler-{step_index}-{n}")))
                .collect(),
        },
        other => panic!("unexpected allocation {other:?}"),
    })
}

fn run(
    document: &ProjectDocument,
    cursor: i64,
    selected: Option<&str>,
    instructions: Vec<SemanticInstruction>,
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
            cursor: ProjectFrame(cursor),
            selected_child: selected.map(id),
            visual_selection: None,
        },
        &SemanticProgram::new(instructions)?,
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 1,
        },
        fresh(usize::MAX),
        allocate,
        |_, _| panic!("no register is read"),
    )
}

fn split(kind: SplitEditKind, length: u32, at: i64) -> Result<SemanticPlan, EditError> {
    run(
        &edit(),
        at,
        None,
        vec![SemanticInstruction::SplitEdit {
            kind,
            length: PauseLength::Frames {
                frames: NonZeroU32::new(length).unwrap(),
            },
        }],
    )
}

/// The only root sound: its heard Edit frames and the Original frame heard
/// at its first one, both exact.
fn heard(document: &ProjectDocument) -> (ExactFrameRange, ExactRatio, &SoundEvent) {
    assert_eq!(document.sounds().len(), 1);
    let event = document.sounds().values().next().unwrap();
    let selected = event
        .mapping
        .selection_frames_with_offset(FrameDuration::ZERO, event.offset, rate())
        .unwrap();
    let SourceAudioMapping::SelectedPlacement {
        start, selection, ..
    } = event.mapping
    else {
        panic!("a handle sound keeps its selected placement")
    };
    (selected, selection.start.checked_sub(start).unwrap(), event)
}

fn range(start: i64, end: i64) -> ExactFrameRange {
    ExactFrameRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap()
}

fn mutes(document: &ProjectDocument, node: &str) -> Vec<ExactFrameRange> {
    document.nodes()[&id(node)]
        .audio_treatments
        .clip_gain()
        .map(|clip| {
            clip.mute_ranges()
                .iter()
                .map(|range| ExactFrameRange::new(range.start(), range.end()).unwrap())
                .collect()
        })
        .unwrap_or_default()
}

/// Every node except audio treatments and edges is unchanged, so every
/// picture, beat and duration is too.
fn assert_pictures_and_time_unchanged(before: &ProjectDocument, after: &ProjectDocument) {
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.nodes().len(), before.nodes().len());
    for (node, beat) in before.nodes().iter() {
        let changed = &after.nodes()[node];
        assert_eq!(changed.kind, beat.kind, "{node}");
        assert_eq!(changed.cutaways, beat.cutaways, "{node}");
        assert_eq!(changed.framing, beat.framing, "{node}");
    }
}

fn assert_undo_exact(before: &ProjectDocument, planned: &SemanticPlan) {
    let replay =
        replay_compound::<EditError>(before, planned.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(&replay.document, &planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        *before
    );
}

fn allowed(document: &ProjectDocument) -> Vec<NodeId> {
    document
        .sound_allowances()
        .values()
        .flat_map(|issuers| issuers.iter().map(|issuer| issuer.instance().node.clone()))
        .collect()
}

#[test]
fn an_l_cut_into_a_pause_keeps_the_outgoing_sound_from_its_handle_over_it() {
    let before = edit();
    let planned = split(SplitEditKind::L, 6, 30).unwrap();
    let after = &planned.document;
    assert_pictures_and_time_unchanged(&before, after);
    let (frames_heard, original, event) = heard(after);
    // Edit 30..36 hear Original 60..66, the handle after a's last picture.
    assert_eq!(frames_heard, range(30, 36));
    assert_eq!(original, ExactRatio::integer(60));
    assert_eq!(
        (event.start_edge, event.end_edge),
        (AudioEdgePolicy::Hard, AudioEdgePolicy::Automatic)
    );
    assert_eq!(event.offset, AudioSample(0));
    // The sound starts on a's own last sample boundary: B(30) = 48,048.
    assert_eq!(
        rate().audio_boundary(ProjectFrame(30)).unwrap(),
        AudioSample(48_048)
    );
    assert_eq!(
        after.nodes()[&id("a")].audio_edges.node_end,
        AudioEdgePolicy::Hard
    );
    assert_eq!(allowed(after), vec![id("hold")]);
    assert_eq!(planned.context.cursor, ProjectFrame(30));
    assert_undo_exact(&before, &planned);
}

#[test]
fn a_j_cut_out_of_a_pause_starts_the_incoming_sound_early_over_it() {
    let before = edit();
    let planned = split(SplitEditKind::J, 6, 45).unwrap();
    let after = &planned.document;
    assert_pictures_and_time_unchanged(&before, after);
    let (frames_heard, original, event) = heard(after);
    assert_eq!(frames_heard, range(39, 45));
    assert_eq!(original, ExactRatio::integer(114));
    assert_eq!(
        (event.start_edge, event.end_edge),
        (AudioEdgePolicy::Automatic, AudioEdgePolicy::Hard)
    );
    assert_eq!(
        after.nodes()[&id("b")].audio_edges.node_start,
        AudioEdgePolicy::Hard
    );
    assert_eq!(allowed(after), vec![id("hold")]);
    assert_undo_exact(&before, &planned);
}

#[test]
fn split_edits_across_a_group_boundary_mute_the_other_side_under_the_handle() {
    let before = edit();
    // J at b|g: the group's first beat c (Original 200) starts 4 frames early
    // over b's last frames, which are muted.
    let planned = split(SplitEditKind::J, 4, 75).unwrap();
    let after = &planned.document;
    assert_pictures_and_time_unchanged(&before, after);
    let (frames_heard, original, _) = heard(after);
    assert_eq!(frames_heard, range(71, 75));
    assert_eq!(original, ExactRatio::integer(196));
    assert_eq!(mutes(after, "b"), vec![range(26, 30)]);
    assert_eq!(
        after.nodes()[&id("c")].audio_edges.node_start,
        AudioEdgePolicy::Hard
    );
    assert!(allowed(after).is_empty());
    assert_undo_exact(&before, &planned);

    // L at b|g: b's sound runs on over the group's first beat, which is muted.
    let planned = split(SplitEditKind::L, 4, 75).unwrap();
    let after = &planned.document;
    let (frames_heard, original, _) = heard(after);
    assert_eq!(frames_heard, range(75, 79));
    assert_eq!(original, ExactRatio::integer(150));
    assert_eq!(mutes(after, "c"), vec![range(0, 4)]);
    assert_undo_exact(&before, &planned);

    // J at g|e: e (Original 100) starts early over the group's last beat d.
    let planned = split(SplitEditKind::J, 4, 115).unwrap();
    let after = &planned.document;
    let (frames_heard, original, _) = heard(after);
    assert_eq!(frames_heard, range(111, 115));
    assert_eq!(original, ExactRatio::integer(96));
    assert_eq!(mutes(after, "d"), vec![range(16, 20)]);
    assert_undo_exact(&before, &planned);
}

#[test]
fn split_edits_beside_pauses_and_groups_refuse_what_they_cannot_carry() {
    for (kind, length, at, message) in [
        // A pause has no sound to lead with.
        (SplitEditKind::J, 4, 30, "no sound of its own"),
        (SplitEditKind::L, 4, 45, "no sound of its own"),
        // The pause is 15 frames long.
        (SplitEditKind::L, 16, 30, "at most 15 frames"),
        // d ends at the Original's last picture: no handle after it.
        (SplitEditKind::L, 4, 115, "at most 0 frames"),
    ] {
        let error = split(kind, length, at).unwrap_err();
        assert!(error.message.contains(message), "{message}: {error:?}");
    }
    // A pause with its own sound refuses rather than playing under the handle.
    let toned = document(json!({
        "root":BeatNode::sequence("Root",vec![id("a"),id("hold")]),
        "a":beat(30, 30),
        "hold":pause(15, HoldAudio::Tone { frequency_hz: 1_000, level: GainDb::new(-10_000).unwrap() }),
    }));
    let error = run(
        &toned,
        30,
        None,
        vec![SemanticInstruction::SplitEdit {
            kind: SplitEditKind::L,
            length: PauseLength::Frames {
                frames: NonZeroU32::new(4).unwrap(),
            },
        }],
    )
    .unwrap_err();
    assert!(error.message.contains("its own sound"), "{error:?}");
    // Two adjacent Sources still roll, exactly as before.
    let two = document(json!({
        "root":BeatNode::sequence("Root",vec![id("a"),id("b")]),
        "a":beat(30, 30),
        "b":beat(120, 30),
    }));
    let rolled = run(
        &two,
        30,
        None,
        vec![SemanticInstruction::SplitEdit {
            kind: SplitEditKind::L,
            length: PauseLength::Frames {
                frames: NonZeroU32::new(4).unwrap(),
            },
        }],
    )
    .unwrap();
    assert!(rolled.document.sounds().is_empty());
    assert_eq!(rolled.document.node_duration(&id("a")).unwrap(), frames(34));
}

fn audio_mapping(document: &ProjectDocument, node: &str) -> (SourceNode, ExactFrameRange) {
    let NodeKind::Source { source } = &document.nodes()[&id(node)].kind else {
        panic!("not a source")
    };
    let SourceAudioMapping::SelectedPlacement { selection, .. } = source.audio_mapping else {
        panic!("not a selected placement")
    };
    (source.clone(), selection)
}

fn lag(document: &ProjectDocument, samples: i64) -> SemanticPlan {
    run(
        document,
        0,
        Some("a"),
        vec![SemanticInstruction::SetAudioLag {
            offset: AudioSample(samples),
        }],
    )
    .unwrap()
}

#[test]
fn an_audio_lag_keeps_the_editorial_window_so_slip_and_trim_carry_the_offset() {
    let before = document(json!({
        "root":BeatNode::sequence("Root",vec![id("a"),id("b")]),
        "a":beat(30, 30),
        "b":beat(120, 30),
    }));
    // 2,400 samples are 1500/1001 frames at 30000/1001 fps.
    let lagged = lag(&before, 2_400);
    let shift = ratio(1_500, 1_001);
    let (source, selection) = audio_mapping(&lagged.document, "a");
    assert_eq!(source.audio_offset, AudioSample(2_400));
    assert_eq!(
        source.edit_window,
        Some(SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(30)).unwrap())
    );
    // The window hears the offset sound, filled from the handle before it:
    // its selection before the offset is the window moved back by the lag.
    assert_eq!(
        selection,
        ExactFrameRange::new(
            ExactRatio::ZERO.checked_sub(shift).unwrap(),
            ExactRatio::integer(30).checked_sub(shift).unwrap()
        )
        .unwrap()
    );
    assert_eq!(
        source
            .audio_mapping
            .selection_frames_with_offset(source.duration, source.audio_offset, rate())
            .unwrap(),
        range(0, 30)
    );
    assert_undo_exact(&before, &lagged);

    // Slip +5 moves picture and the offset sound together, keeping the lag.
    let slipped = run(
        &lagged.document,
        0,
        Some("a"),
        vec![SemanticInstruction::Slip { frames: 5 }],
    )
    .unwrap();
    let (after, selection) = audio_mapping(&slipped.document, "a");
    assert_eq!(after.audio_offset, AudioSample(2_400));
    assert_eq!(after.audio_mapping.start_frames(), ExactRatio::integer(-35));
    assert_eq!(after.video_mapping.start_frames(), ExactRatio::integer(-35));
    assert_eq!(
        selection,
        ExactFrameRange::new(
            ExactRatio::ZERO.checked_sub(shift).unwrap(),
            ExactRatio::integer(30).checked_sub(shift).unwrap()
        )
        .unwrap()
    );
    assert_undo_exact(&lagged.document, &slipped);

    // Trim Out +3 lengthens the beat; the offset sound fills the new frames.
    let trimmed = run(
        &lagged.document,
        0,
        Some("a"),
        vec![SemanticInstruction::Trim {
            intent: SourceTrimIntent {
                in_frames: 0,
                out_frames: 3,
                slip_frames: 0,
                roll_frames: 0,
                policy: SourceTrimPolicy::Ripple,
            },
        }],
    )
    .unwrap();
    assert_eq!(trimmed.document.duration().unwrap(), frames(63));
    let (after, _) = audio_mapping(&trimmed.document, "a");
    assert_eq!(after.audio_offset, AudioSample(2_400));
    assert_eq!(
        after
            .audio_mapping
            .selection_frames_with_offset(after.duration, after.audio_offset, rate())
            .unwrap(),
        range(0, 33)
    );
    assert_undo_exact(&lagged.document, &trimmed);

    // A lag past the start of the Original leaves the start silent: a beat at
    // Original 0 has no handle before it, so its selection starts at 0.
    let start = document(json!({
        "root":BeatNode::sequence("Root",vec![id("a")]),
        "a":beat(0, 30),
    }));
    let (source, selection) = audio_mapping(&lag(&start, 2_400).document, "a");
    assert!(source.edit_window.is_some());
    assert_eq!(
        selection,
        ExactFrameRange::new(
            ExactRatio::ZERO,
            ExactRatio::integer(30).checked_sub(shift).unwrap()
        )
        .unwrap()
    );

    // A Source without an editorial window keeps its old lag: the mapping is
    // unchanged and Slip still refuses.
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["a"]["kind"]["source"]
        .as_object_mut()
        .unwrap()
        .remove("edit_window");
    let legacy = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let lagged = lag(&legacy, 2_400);
    let (source, selection) = audio_mapping(&lagged.document, "a");
    assert_eq!(source.edit_window, None);
    assert_eq!(selection, range(0, 30));
    let error = run(
        &lagged.document,
        0,
        Some("a"),
        vec![SemanticInstruction::Slip { frames: 5 }],
    )
    .unwrap_err();
    assert!(error.message.contains("editorial window"), "{error:?}");
}
