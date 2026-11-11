use crate::*;

#[path = "tests/fixed_retime.rs"]
mod fixed_retime;
#[path = "tests/processing.rs"]
mod processing;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn rev(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn span(start: i64, end: i64) -> SourceSpan {
    let base = SourceTimeBase::new(1, 44_100).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: base,
        },
        SourceTimestamp {
            ticks: end,
            time_base: base,
        },
    )
    .unwrap()
}
fn audio() -> SourceAudio {
    SourceAudio {
        asset: AssetId::new("audio").unwrap(),
        span: span(3100, 23_100),
    }
}
fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Room",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::RoomTone { source: audio() },
        },
    )
}
fn source(length: i64) -> BeatNode {
    let mut node = BeatNode::sequence("Source", vec![]);
    node.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(length),
            edit_window: None,
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(audio()),
            audio_mapping: SourceAudioMapping::natural_rate(
                audio().span,
                FrameRate::new(30_000, 1001).unwrap(),
            )
            .unwrap(),
            link: LinkRelation::Independent,
            audio_offset: AudioSample(0),
        },
    };
    node
}
fn repeat(owner: &str, child: &str, plays: u32) -> BeatNode {
    let mut result = BeatNode::sequence("Repeat", vec![]);
    result.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(rev(owner), plays).unwrap(),
        gap: Some(match hold(1).kind {
            NodeKind::Hold { recipe } => recipe,
            _ => unreachable!(),
        }),
        escalation: None,
    };
    result
}
fn fixture(shape: u8, overrides: bool) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("duration-clocks").unwrap(),
        rev("before"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    document.assets.insert(
        AssetId::new("audio").unwrap(),
        AssetRecord {
            label: "Audio".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span(0, 50_000)),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    document.nodes.extend([
        (id("lead"), hold(3)),
        (id("before"), source(2)),
        (id("hold"), hold(2)),
        (id("after"), source(2)),
        (id("tail"), source(3)),
        (
            id("body"),
            BeatNode::sequence("Body", vec![id("before"), id("hold"), id("after")]),
        ),
    ]);
    let child = match shape {
        0 => "body",
        1 => {
            document
                .nodes
                .insert(id("inner"), repeat("inner", "body", 3));
            "inner"
        }
        2 => {
            document
                .nodes
                .insert(id("inner"), repeat("inner", "body", 3));
            document
                .nodes
                .insert(id("outer"), repeat("outer", "inner", 2));
            "outer"
        }
        3 => {
            document
                .nodes
                .insert(id("inner"), repeat("inner", "body", 3));
            document.nodes.insert(id("outer-room"), hold(2));
            document.nodes.insert(
                id("outer-body"),
                BeatNode::sequence("Outer body", vec![id("inner"), id("outer-room")]),
            );
            document
                .nodes
                .insert(id("outer"), repeat("outer", "outer-body", 2));
            "outer"
        }
        _ => unreachable!(),
    };
    document.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id(child), id("tail")]),
    );
    if shape > 0 && overrides {
        document.nodes.extend([
            (id("owned-before"), source(2)),
            (id("owned-hold"), hold(4)),
            (id("owned-after"), source(2)),
            (
                id("owned-body"),
                BeatNode::sequence(
                    "Owned",
                    vec![id("owned-before"), id("owned-hold"), id("owned-after")],
                ),
            ),
        ]);
        document.overrides.insert(
            id("inner"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: rev("inner"),
                    ordinal: 1,
                },
                root: id("owned-body"),
            }])
            .unwrap(),
        );
    }
    document.marks.insert(
        MarkId::new("suffix-mark").unwrap(),
        Mark {
            owner: id("after"),
            label: "Suffix".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: id("after"),
                    position: ExactRatio::ONE,
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
            fragments: vec![],
        },
    );
    document.validate().unwrap();
    document
}
fn request(document: &ProjectDocument, target: &str, length: i64) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: rev("resize"),
        command: Command::SetHoldDuration {
            node: id(target),
            duration: frames(length),
        },
    }
}
fn resized(document: &ProjectDocument, target: &str, length: i64) -> ProjectDocument {
    let edit = apply(document, &request(document, target, length)).unwrap();
    let after = edit.forward.apply(document).unwrap();
    assert_eq!(edit.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    assert_eq!(after.marks, document.marks);
    after
}

// The fixtures are deliberately tiny. Only the regression collector expands
// them; the production edit must also handle the large compact test below.

#[test]
fn ordinary_hold_duration_exact_noop_terminal_and_dormant_definition_keep_bindings_unchanged() {
    let before = fixture(0, false);
    assert_eq!(
        resized(&before, "hold", 2).audio_bindings,
        before.audio_bindings
    );
    let mut terminal = before.clone();
    let NodeKind::Sequence { children } = &mut terminal.nodes.get_mut(&id("root")).unwrap().kind
    else {
        panic!()
    };
    children.pop();
    terminal.nodes.remove(&id("tail"));
    terminal.nodes.remove(&id("after")).unwrap();
    let NodeKind::Sequence { children } = &mut terminal.nodes.get_mut(&id("body")).unwrap().kind
    else {
        panic!()
    };
    children.pop();
    terminal.marks.clear();
    terminal.validate().unwrap();
    assert_eq!(
        resized(&terminal, "hold", 3).audio_bindings,
        terminal.audio_bindings
    );
    let mut dormant = fixture(1, true);
    let NodeKind::Repeat { iterations, .. } =
        &mut dormant.nodes.get_mut(&id("inner")).unwrap().kind
    else {
        panic!()
    };
    *iterations = IterationOrder::new(rev("inner"), 1).unwrap();
    let owned = dormant.overrides.get_mut(&id("inner")).unwrap();
    let root = owned
        .remove(&IterationId {
            allocation: rev("inner"),
            ordinal: 1,
        })
        .unwrap();
    owned.insert(
        IterationId {
            allocation: rev("inner"),
            ordinal: 0,
        },
        root,
    );
    dormant.validate().unwrap();
    let after = resized(&dormant, "hold", 3);
    assert_eq!(after.duration().unwrap(), dormant.duration().unwrap());
    assert_eq!(after.audio_bindings, dormant.audio_bindings);
}

#[test]
fn ordinary_hold_duration_shared_million_play_clock_work_stays_compact() {
    let mut before = fixture(2, true);
    for owner in ["inner", "outer"] {
        let NodeKind::Repeat { iterations, .. } =
            &mut before.nodes.get_mut(&id(owner)).unwrap().kind
        else {
            panic!()
        };
        *iterations = IterationOrder::new(rev(owner), 1_000_000).unwrap();
    }
    before.validate().unwrap();
    let after = resized(&before, "hold", 3);
    assert_eq!(
        after.duration().unwrap().frames() - before.duration().unwrap().frames(),
        999_999_000_000
    );
    assert!(after.audio_bindings.timings().len() <= 1);
    assert!(after.audio_bindings.to_json().unwrap().len() < 50_000);
    assert_eq!(after.nodes().len(), before.nodes().len());
}

#[test]
fn ordinary_hold_duration_retains_tail_clamp_and_invalid_request_admission() {
    let mut before = fixture(0, false);
    let NodeKind::Hold { recipe } = &mut before.nodes.get_mut(&id("hold")).unwrap().kind else {
        panic!()
    };
    recipe.audio = HoldAudio::Tail {
        maximum: frames(2),
        effect: Default::default(),
        input: Default::default(),
    };
    before.validate().unwrap();
    let original = before.clone();
    let after = resized(&before, "hold", 1);
    let NodeKind::Hold { recipe } = &after.nodes()[&id("hold")].kind else {
        panic!()
    };
    assert!(matches!(recipe.audio, HoldAudio::Tail { maximum, .. } if maximum == frames(1)));
    assert!(apply(&before, &request(&before, "after", 1)).is_err());
    assert!(apply(&before, &request(&before, "hold", 0)).is_err());
    assert_eq!(before, original);
}

#[test]
fn ordinary_hold_duration_shortening_transforms_marks_after_structural_preflight() {
    let mut before = fixture(0, false);
    let mark = MarkId::new("cut-away").unwrap();
    before.marks.insert(
        mark.clone(),
        Mark {
            owner: id("hold"),
            label: "Removed interior".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: id("hold"),
                    position: ExactRatio::new(3, 2).unwrap(),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
            fragments: vec![],
        },
    );
    before.validate().unwrap();
    let edit = apply(&before, &request(&before, "hold", 1)).unwrap();
    let after = edit.forward.apply(&before).unwrap();
    assert!(matches!(
        after.marks[&mark].state,
        MarkState::Unresolved { .. }
    ));
    assert_eq!(edit.inverse.apply(&after).unwrap(), before);
}
