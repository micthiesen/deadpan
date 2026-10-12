use super::*;
use crate::*;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn hold(frames: i64) -> HoldRecipe {
    HoldRecipe {
        duration: FrameDuration::new(frames).unwrap(),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
}
fn repeated(name: &str, child: &str, plays: u32, gap: i64) -> BeatNode {
    let mut result = BeatNode::sequence(name, vec![]);
    result.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(revision(name), plays).unwrap(),
        gap: Some(hold(gap)),
        escalation: None,
    };
    result
}
fn fixture(nested: bool) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("pause-plays").unwrap(),
        revision("before"),
        PresentationBasis {
            width: 64,
            height: 36,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    document.nodes.extend([
        (
            id("root"),
            BeatNode::sequence("root", vec![id("outer"), id("tail")]),
        ),
        (
            id("outer"),
            repeated("outer", if nested { "body" } else { "a" }, 3, 2),
        ),
        (id("a"), BeatNode::hold("a", hold(4))),
        (id("tail"), BeatNode::hold("tail", hold(3))),
    ]);
    if nested {
        document.nodes.extend([
            (
                id("body"),
                BeatNode::sequence("body", vec![id("inner"), id("b")]),
            ),
            (id("inner"), repeated("inner", "a", 2, 1)),
            (id("b"), BeatNode::hold("b", hold(2))),
        ]);
    }
    document.validate().unwrap();
    document
}
fn request(document: &ProjectDocument, at: ProjectFrame, name: &str) -> CommandRequest {
    let PauseInsertionTarget::Occurrence(target) = document.pause_insertion_target(at).unwrap()
    else {
        panic!()
    };
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::InsertOccurrenceTime {
            at,
            hold: hold(1),
            id: id(&format!("{name}-pause")),
            ai: false,
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
            identities: OccurrencePauseIdentities {
                isolation: OccurrenceIdentities {
                    nodes: (0..target.isolation_nodes)
                        .map(|n| id(&format!("{name}-clone-{n}")))
                        .collect(),
                    marks: (0..target.isolation_marks)
                        .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                        .collect(),
                },
                split: SplitIdentities {
                    nodes: (0..target.split_nodes)
                        .map(|n| id(&format!("{name}-split-{n}")))
                        .collect(),
                },
                container: target.container.then(|| id(&format!("{name}-container"))),
                gap: target.gap.then(|| id(&format!("{name}-gap"))),
            },
        },
    }
}

#[test]
fn occurrence_pause_all_play_and_gap_boundaries_are_one_reversible_frame() {
    for nested in [false, true] {
        let before = fixture(nested);
        let duration = before.duration().unwrap().frames();
        for at in 1..duration {
            if !matches!(
                before.pause_insertion_target(ProjectFrame(at)).unwrap(),
                PauseInsertionTarget::Occurrence(_)
            ) {
                continue;
            }
            let request = request(&before, ProjectFrame(at), "pause");
            let result = apply_with_result(&before, &request);
            let (transaction, after) =
                result.unwrap_or_else(|error| panic!("nested={nested} at={at}: {error}"));
            assert_eq!(after.duration().unwrap().frames(), duration + 1);
            assert_eq!(transaction.duration_delta, 1);
            assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
            assert_eq!(transaction.forward.apply(&before).unwrap(), after);
            assert_eq!(
                ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
                after
            );
            assert_eq!(after.nodes()[&id("a")], before.nodes()[&id("a")]);
            let proof = derive_scoped_isolation(&before, &request, &after).unwrap();
            proof.record().validate(&before, &request, &after).unwrap();
        }
    }
}

#[test]
fn occurrence_pause_reuses_owned_branches_on_later_insertions() {
    let before = fixture(true);
    let first = request(&before, ProjectFrame(3), "first");
    let (_, middle) = apply_with_result(&before, &first).unwrap();
    let next = request(&middle, ProjectFrame(3), "second");
    let Command::InsertOccurrenceTime { identities, .. } = &next.command else {
        panic!()
    };
    assert!(identities.isolation.nodes.is_empty());
    let (transaction, after) = apply_with_result(&middle, &next).unwrap();
    assert_eq!(
        after.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 2
    );
    assert_eq!(transaction.inverse.apply(&after).unwrap(), middle);
}

#[test]
fn occurrence_pause_keeps_content_marks_in_selected_and_later_plays_and_gaps() {
    for nested in [false, true] {
        let mut before = fixture(nested);
        let total = before.duration().unwrap().frames();
        for frame in 1..total {
            before.marks.insert(
                MarkId::new(format!("frame-{frame}")).unwrap(),
                Mark {
                    owner: id("root"),
                    label: format!("frame {frame}"),
                    boundary: BoundaryAnchor {
                        coordinate: Anchor::Local {
                            node: id("root"),
                            position: ExactRatio::integer(frame),
                        },
                        bias: InsertionBias::Right,
                    },
                    loss_policy: AnchorLossPolicy::KeepUnresolved,
                    state: MarkState::Bound,
                    fragments: vec![],
                },
            );
        }
        before.marks.insert(
            MarkId::new("owned").unwrap(),
            Mark {
                owner: id("a"),
                label: "owned".into(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: id("a"),
                        position: ExactRatio::ONE,
                    },
                    bias: InsertionBias::Right,
                },
                loss_policy: AnchorLossPolicy::KeepUnresolved,
                state: MarkState::Bound,
                fragments: vec![],
            },
        );
        before.validate().unwrap();
        for at in [2, 5, 8] {
            let (_, after) =
                apply_with_result(&before, &request(&before, ProjectFrame(at), "marked")).unwrap();
            for frame in 1..total {
                let marked = &after.marks()[&MarkId::new(format!("frame-{frame}")).unwrap()];
                assert_eq!(
                    marked.state,
                    MarkState::Bound,
                    "nested={nested} at={at} mark={frame}"
                );
                assert_eq!(
                    marked.boundary.coordinate,
                    Anchor::Local {
                        node: id("root"),
                        position: ExactRatio::integer(frame + i64::from(frame >= at))
                    },
                    "nested={nested} at={at} mark={frame}"
                );
            }
            assert_eq!(
                after.marks()[&MarkId::new("owned").unwrap()],
                before.marks()[&MarkId::new("owned").unwrap()]
            );
        }
    }
}

#[test]
fn occurrence_pause_is_compact_at_maximum_repeat_count_and_rejects_bad_pools() {
    let mut before = fixture(false);
    let NodeKind::Repeat { iterations, .. } = &mut before.nodes.get_mut(&id("outer")).unwrap().kind
    else {
        panic!()
    };
    *iterations = IterationOrder::new(revision("many"), u32::MAX).unwrap();
    before.validate().unwrap();
    let at = ProjectFrame(6 * (i64::from(u32::MAX) - 2) + 2);
    let valid = request(&before, at, "bounded");
    let (transaction, after) = apply_with_result(&before, &valid).unwrap();
    assert!(after.nodes().len() < 16);
    assert_eq!(transaction.duration_delta, 1);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    for case in 0..4 {
        let mut invalid = valid.clone();
        let Command::InsertOccurrenceTime {
            identities,
            timing,
            id: pause_id,
            hold,
            ..
        } = &mut invalid.command
        else {
            panic!()
        };
        match case {
            0 => {
                identities.isolation.nodes.pop();
            }
            1 => {
                *pause_id = identities.isolation.nodes[0].clone();
            }
            2 => {
                timing.ordinal = u32::MAX;
            }
            _ => {
                hold.duration = FrameDuration::ZERO;
            }
        }
        assert!(crate::apply(&before, &invalid).is_err(), "case {case}");
        assert_eq!(before, transaction.inverse.apply(&after).unwrap());
    }
}

#[test]
fn occurrence_pause_preserves_gap_permissions_without_granting_the_new_hold() {
    let mut before = fixture(false);
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: clock,
        },
        SourceTimestamp {
            ticks: 20_000,
            time_base: clock,
        },
    )
    .unwrap();
    let asset = AssetId::new("sound").unwrap();
    before.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "sound".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    let sound = SoundId::new("sound").unwrap();
    before.sounds.insert(
        sound.clone(),
        SoundEvent {
            owner: id("root"),
            label: "sound".into(),
            source: SourceAudio { asset, span },
            mapping: SourceAudioMapping::natural_rate(span, before.presentation_basis().frame_rate)
                .unwrap(),
            offset: AudioSample(137),
            treatments: AudioTreatments::default(),
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        },
    );
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&id("outer")].kind else {
        panic!()
    };
    let issuers: Vec<_> = (0..2)
        .map(|play| SoundHoldIssuer::RepeatGap {
            instance: InstancePath {
                node: id("outer"),
                repeats: vec![],
            },
            gap_after: iterations.at(play).unwrap(),
        })
        .collect();
    before.sound_allowances.insert(
        sound.clone(),
        SoundHoldAllowances::try_from(issuers.clone()).unwrap(),
    );
    before.validate().unwrap();
    let (transaction, after) =
        apply_with_result(&before, &request(&before, ProjectFrame(5), "allowed")).unwrap();
    let permissions = &after.sound_allowances()[&sound];
    assert_eq!(
        permissions.len(),
        3,
        "two split Hold providers and the other default gap"
    );
    assert!(!permissions.contains(&issuers[0]));
    assert!(permissions.contains(&issuers[1]));
    assert!(
        permissions
            .iter()
            .all(|issuer| issuer.instance().node != id("allowed-pause"))
    );
    assert_eq!(
        permissions
            .iter()
            .filter(|issuer| matches!(issuer, SoundHoldIssuer::Node { .. }))
            .count(),
        2
    );
    assert_eq!(after.sounds()[&sound].offset, AudioSample(137));
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
}
