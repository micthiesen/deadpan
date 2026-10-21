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
fn occurrences(document: &ProjectDocument) -> Vec<(InstancePath, Option<IterationId>)> {
    fn walk(
        document: &ProjectDocument,
        node: &NodeId,
        repeats: &mut Vec<RepeatInstance>,
        out: &mut Vec<(InstancePath, Option<IterationId>)>,
    ) {
        match &document.nodes()[node].kind {
            NodeKind::Source { .. } | NodeKind::Hold { .. } => out.push((
                InstancePath {
                    node: node.clone(),
                    repeats: repeats.clone(),
                },
                None,
            )),
            NodeKind::Sequence { children } => {
                for child in children {
                    walk(document, child, repeats, out);
                }
            }
            NodeKind::Repeat {
                child,
                iterations,
                gap,
                ..
            } => {
                for ordinal in 0..iterations.len() {
                    let iteration = iterations.at(ordinal).unwrap();
                    repeats.push(RepeatInstance {
                        node: node.clone(),
                        iteration: iteration.clone(),
                    });
                    let root = document
                        .overrides()
                        .get(node)
                        .and_then(|entries| entries.get(&iteration))
                        .unwrap_or(child);
                    walk(document, root, repeats, out);
                    repeats.pop();
                    if ordinal + 1 < iterations.len() && gap.is_some() {
                        out.push((
                            InstancePath {
                                node: node.clone(),
                                repeats: repeats.clone(),
                            },
                            Some(iteration),
                        ));
                    }
                }
            }
            NodeKind::Retime { .. } => unreachable!(),
        }
    }
    let mut result = vec![];
    walk(document, document.root(), &mut vec![], &mut result);
    result
}
fn binding(
    state: &AudioBindingState,
    instance: &InstancePath,
    gap: &Option<IterationId>,
) -> ResolvedAudioBinding {
    match gap {
        Some(after) => state.resolve_gap_in(
            &instance.node,
            AudioBindingEnvironment::GapOccurrence { instance, after },
            MAX_DOCUMENT_NODES,
        ),
        None => state.resolve(&instance.node, instance, MAX_DOCUMENT_NODES),
    }
    .unwrap()
}

#[test]
fn ordinary_hold_duration_unbound_and_partially_bound_clocks_keep_every_concrete_old_origin() {
    for shape in 0..4 {
        for overrides in [false, true] {
            for partial in [false, true] {
                let mut before = fixture(shape, overrides);
                let old = capture_unbound_audio_bindings(
                    &before,
                    AudioTimingId {
                        allocation: rev("capture"),
                        ordinal: 0,
                    },
                )
                .unwrap();
                if partial {
                    before.audio_bindings = old.clone();
                    before.audio_bindings.bindings.remove(&id("tail"));
                }
                for length in [1, 3] {
                    let after = resized(&before, "hold", length);
                    for (instance, gap) in occurrences(&before) {
                        let old = binding(&old, &instance, &gap);
                        let current = binding(after.audio_bindings(), &instance, &gap);
                        assert_eq!(
                            current.lattice.origin, old.lattice.origin,
                            "shape={shape} overrides={overrides} partial={partial} length={length} instance={instance:?} gap={gap:?}"
                        );
                        assert_eq!(
                            current.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
                            old.lattice.sample_boundary(ExactRatio::ZERO).unwrap()
                        );
                        if let Some(resume) = current.resume {
                            assert_eq!(resume.local_boundary, ExactRatio::ZERO);
                            assert_eq!(resume.reference_local_delta, ExactRatio::ZERO);
                        }
                    }
                }
            }
        }
    }
}

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
fn ordinary_hold_duration_physical_capture_shares_the_complete_sound_journal_clock() {
    let mut before = fixture(0, false);
    let effect = SoundId::new("effect").unwrap();
    let source = SourceAudio {
        asset: audio().asset,
        span: span(3100, 3200),
    };
    before.beat_sounds.insert(
        id("tail"),
        std::collections::BTreeMap::from([(
            effect.clone(),
            BeatSound {
                label: "Effect".into(),
                mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    before.presentation_basis().frame_rate,
                )
                .unwrap(),
                source,
                offset: AudioSample(37),
                treatments: AudioTreatments::default(),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]),
    );
    before.validate().unwrap();
    let old_layout = FrozenAudioLayout::capture(&before).unwrap();
    let after = resized(&before, "hold", 1);
    assert_eq!(after.beat_sounds, before.beat_sounds);
    let timing = AudioTimingId {
        allocation: rev("resize"),
        ordinal: 0,
    };
    assert_eq!(after.audio_bindings.timings()[&timing], old_layout);
    let journal = &after.audio_bindings.sound_clocks()[&id("tail")][&effect];
    assert_eq!(journal.clocks().len(), 1);
    assert_eq!(journal.clocks()[0].timing(), &timing);
    assert!(after.audio_bindings.bindings().contains_key(&id("after")));
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
    assert!(after.audio_bindings.bindings().contains_key(&id("after")));
}

#[test]
fn ordinary_hold_duration_many_silent_resizes_keep_historical_clock_work_bounded() {
    let mut before = fixture(0, false);
    before.nodes.clear();
    before.marks.clear();
    let mut roots = vec![];
    let mut steps = vec![];
    for index in 0..257 {
        let owner = format!("silent-{index}");
        let wrapper = format!("definition-{index}");
        let mut node = hold(12);
        let NodeKind::Hold { recipe } = &mut node.kind else {
            panic!()
        };
        recipe.audio = HoldAudio::Silence;
        before.nodes.insert(id(&owner), node);
        let mut repeated = repeat(&wrapper, &owner, 1);
        let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
            panic!()
        };
        *gap = None;
        before.nodes.insert(id(&wrapper), repeated);
        roots.push(id(&wrapper));
        steps.push(ResolvedStep::Edit {
            edit: LeafEdit::new(
                rev(&format!("resize-{index}")),
                Command::SetHoldDuration {
                    node: id(&owner),
                    duration: frames(18),
                },
            )
            .unwrap(),
        });
    }
    before
        .nodes
        .insert(id("root"), BeatNode::sequence("Silent copies", roots));
    before.audio_bindings = capture_unbound_audio_bindings(
        &before,
        AudioTimingId {
            allocation: rev("historical-clocks"),
            ordinal: 0,
        },
    )
    .unwrap();
    before.validate().unwrap();
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: rev("all-resized"),
        command: Command::Compound {
            transaction: ResolvedTransaction::new(0, Default::default(), steps).unwrap(),
        },
    };
    let edit = apply(&before, &request).unwrap();
    let after = edit.forward.apply(&before).unwrap();
    assert_eq!(after.duration().unwrap(), frames(257 * 18));
    assert_eq!(after.audio_bindings, before.audio_bindings);
    assert_eq!(edit.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    // Explicitly choosing room tone later retains the already authored lattice;
    // skipping silent transport must not reset or discard that older intent.
    let audio_edit = apply(
        &after,
        &CommandRequest {
            project_id: after.project_id().clone(),
            expected_revision: after.revision_id().clone(),
            new_revision: rev("choose-room-tone"),
            command: Command::SetHoldAudio {
                node: id("silent-256"),
                audio: HoldAudio::RoomTone { source: audio() },
            },
        },
    )
    .unwrap();
    let toned = audio_edit.forward.apply(&after).unwrap();
    assert_eq!(toned.audio_bindings, before.audio_bindings);
    assert_eq!(audio_edit.inverse.apply(&toned).unwrap(), after);
}

#[test]
fn ordinary_hold_duration_silent_owner_skip_keeps_non_silent_source_and_gap_transport() {
    let mut before = fixture(1, true);
    let NodeKind::Hold { recipe } = &mut before.nodes.get_mut(&id("hold")).unwrap().kind else {
        panic!()
    };
    recipe.audio = HoldAudio::Silence;
    before.audio_bindings = capture_unbound_audio_bindings(
        &before,
        AudioTimingId {
            allocation: rev("older-audible-clocks"),
            ordinal: 0,
        },
    )
    .unwrap();
    before.validate().unwrap();
    let after = resized(&before, "hold", 3);
    assert_eq!(
        after.audio_bindings.bindings()[&id("hold")],
        before.audio_bindings.bindings()[&id("hold")]
    );
    assert!(
        after.audio_bindings.bindings()[&id("after")]
            .reanchors
            .len()
            > before.audio_bindings.bindings()[&id("after")]
                .reanchors
                .len()
    );
    assert!(
        after.audio_bindings.gap_bindings()[&id("inner")]
            .reanchors
            .len()
            > before.audio_bindings.gap_bindings()[&id("inner")]
                .reanchors
                .len()
    );
    for (instance, gap) in occurrences(&before) {
        let old = binding(before.audio_bindings(), &instance, &gap);
        let current = binding(after.audio_bindings(), &instance, &gap);
        assert_eq!(current.lattice.origin, old.lattice.origin);
        assert_eq!(
            current.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
            old.lattice.sample_boundary(ExactRatio::ZERO).unwrap()
        );
    }
}
