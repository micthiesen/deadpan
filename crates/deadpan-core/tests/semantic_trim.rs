//! The recorded Trim and Slip instructions: exact accepted values, one Undo,
//! refusal instead of clamping on a beat without enough source.

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
        SemanticAllocationRequest::Trim {
            step_index,
            target_wrapper,
            right_wrapper,
            split_nodes,
            filler_nodes,
        } => SemanticAllocation::Trim {
            new_revision: RevisionId::new(format!("leaf-{step_index}")).unwrap(),
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
        SemanticAllocationRequest::ParameterEdit { step_index } => {
            SemanticAllocation::ParameterEdit {
                new_revision: RevisionId::new(format!("leaf-{step_index}")).unwrap(),
            }
        }
        other => panic!("unexpected allocation {other:?}"),
    })
}

fn plan(
    document: &ProjectDocument,
    selected: &str,
    instructions: Vec<SemanticInstruction>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic(
        document,
        &SemanticContext {
            parent: id("root"),
            cursor: ProjectFrame(0),
            selected_child: Some(id(selected)),
            visual_selection: None,
        },
        &SemanticProgram::new(instructions).unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 1,
        },
        RevisionId::new("outer").unwrap(),
        allocate,
        |_, _| panic!("no register is read"),
    )
}

fn trim(out_frames: i64, policy: SourceTrimPolicy) -> SemanticInstruction {
    SemanticInstruction::Trim {
        intent: SourceTrimIntent {
            in_frames: 0,
            out_frames,
            slip_frames: 0,
            roll_frames: 0,
            policy,
        },
    }
}

fn replayed(document: &ProjectDocument, planned: &SemanticPlan) -> ProjectDocument {
    let replay =
        replay_compound::<EditError>(document, planned.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        *document
    );
    replay.document
}

#[test]
fn a_recorded_trim_repeats_its_exact_values_on_the_next_beat() {
    let document = document();
    // Shorten the first beat's Out by 5 frames, step to the next beat and
    // apply the same trim: two ripple trims in one transaction.
    let planned = plan(
        &document,
        "left",
        vec![
            trim(-5, SourceTrimPolicy::Ripple),
            SemanticInstruction::MoveBeats {
                forward: true,
                count: NonZeroU32::new(1).unwrap(),
            },
            trim(-5, SourceTrimPolicy::Ripple),
        ],
    )
    .unwrap();
    let after = replayed(&document, &planned);
    assert_eq!(after.duration().unwrap(), frames(30));
    assert_eq!(planned.context.cursor, ProjectFrame(15));
    assert_eq!(
        planned.trace[2].resolved_range,
        Some(FrameRange::new(ProjectFrame(15), ProjectFrame(30)).unwrap())
    );
    // Overwrite keeps the duration with a filler after the trimmed beat.
    let overwritten = plan(
        &document,
        "left",
        vec![trim(-5, SourceTrimPolicy::Overwrite)],
    )
    .unwrap();
    assert_eq!(
        replayed(&document, &overwritten).duration().unwrap(),
        frames(40)
    );
}

#[test]
fn a_trim_without_enough_source_refuses_instead_of_clamping() {
    let document = document();
    // The second beat shows Original frames 50..70 of 100: 30 frames remain.
    assert!(plan(&document, "right", vec![trim(30, SourceTrimPolicy::Ripple)]).is_ok());
    assert!(plan(&document, "right", vec![trim(31, SourceTrimPolicy::Ripple)]).is_err());
    assert!(plan(&document, "right", vec![trim(0, SourceTrimPolicy::Ripple)]).is_err());
}

fn first_shown(document: &ProjectDocument, node: &str) -> ExactRatio {
    let NodeKind::Source { source } = &document.nodes()[&id(node)].kind else {
        panic!()
    };
    let SourceVideoMapping::SelectedPlacement { start, .. } = source.video_mapping else {
        panic!()
    };
    start
}

#[test]
fn a_recorded_slip_moves_the_selected_beat_by_exact_frames_or_refuses() {
    let document = document();
    let slip = |frames| SemanticInstruction::Slip { frames };
    let planned = plan(&document, "right", vec![slip(5)]).unwrap();
    let after = replayed(&document, &planned);
    // Later Original material: the placement starts 5 frames earlier.
    assert_eq!(first_shown(&after, "right"), ExactRatio::integer(-55));
    assert_eq!(after.duration().unwrap(), frames(40));
    assert_eq!(planned.context.selected_child, Some(id("right")));
    // 30 frames of Original remain after the second beat; 31 refuse.
    assert!(plan(&document, "right", vec![slip(30)]).is_ok());
    let refused = plan(&document, "right", vec![slip(31)]).unwrap_err();
    assert!(
        refused.message.contains("at most +30"),
        "{}",
        refused.message
    );
    assert!(plan(&document, "right", vec![slip(0)]).is_err());
}
