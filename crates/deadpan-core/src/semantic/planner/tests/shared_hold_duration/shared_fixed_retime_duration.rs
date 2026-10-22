//! All-Default intent retains one exact sampled presentation through fixed stages.
use super::*;
use crate::{PitchPolicy, RetimePurpose};

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn stage(child: &str, output: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    let mut beat = BeatNode::sequence("Fixed stage", vec![]);
    beat.kind = NodeKind::Retime {
        child: node(child),
        duration: FrameDuration::new(output).unwrap(),
        mapping: range(start, end),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    beat
}

fn fixed(pitch: PitchPolicy, crop: i64, output: i64) -> ProjectDocument {
    let mut d = ProjectDocument::new(
        ProjectId::new("shared-fixed-duration").unwrap(),
        revision("fixed-base"),
        crate::basis::default_basis(),
        node("root"),
    )
    .unwrap();
    d.nodes.extend([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("outer"), node("tail")]),
        ),
        (node("lead"), hold(3)),
        (node("tail"), hold(3)),
        (node("outer"), repeated("outer", "stage", 3)),
        (node("stage"), stage("body", output, crop, 8, pitch)),
        (
            node("body"),
            BeatNode::sequence("Body", vec![node("before"), node("held"), node("speech")]),
        ),
        (node("before"), hold(2)),
        (node("held"), hold(2)),
        (node("speech"), hold(6)),
    ]);
    d.validate().unwrap();
    d
}

fn authority(outer: Option<u32>) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("outer")),
        shared_hold_duration: Some(SemanticSharedHoldDurationContext {
            root: node("outer"),
            target: ScopedNodeTarget {
                node: node("held"),
                repeats: vec![RepeatEditStep {
                    repeat: node("outer"),
                    branch: RepeatEditBranch::Default,
                }],
            },
            presentation: outer.map(|outer| instance("held", outer, None)),
        }),
        ..context("root", 10)
    }
}

fn observed(
    d: &ProjectDocument,
    c: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> (SemanticPlan, Vec<SemanticAllocationRequest>) {
    let mut allocations = vec![];
    let p = plan_semantic(
        d,
        c,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("fixed-final"),
        |r| {
            allocations.push(r.clone());
            allocate(r)
        },
        no_original,
    )
    .unwrap();
    (p, allocations)
}

fn check(d: &ProjectDocument, c: &SemanticContext, p: &SemanticPlan) {
    assert_eq!(p.document.duration(), d.duration());
    assert_eq!(p.document.nodes().len(), d.nodes().len());
    assert_eq!(p.document.overrides(), d.overrides());
    assert_eq!(p.document.gap_overrides(), d.gap_overrides());
    assert_eq!(p.context.shared_hold_duration, c.shared_hold_duration);
    assert_eq!(p.context.parent, c.parent);
    assert_eq!(p.context.selected_child, c.selected_child);
    for id in ["stage", "outer", "lead", "tail"] {
        assert_eq!(p.document.nodes()[&node(id)], d.nodes()[&node(id)]);
    }
    reversible(d, p);
}

#[test]
fn shared_fixed_retime_duration_fractional_half_center_ties_all_pitches_grow_shrink() {
    for pitch in [
        PitchPolicy::FollowSpeed,
        PitchPolicy::Preserve,
        PitchPolicy::Shift { semitones: 3 },
    ] {
        let d = fixed(pitch, 0, 6);
        assert_eq!(d.duration().unwrap().frames(), 24);
        for outer in [0, 1, 2] {
            let mut c = authority(Some(outer));
            c.cursor = ProjectFrame(3 + 6 * i64::from(outer) + 2);
            let offset = 3 + 6 * i64::from(outer);
            let old = visible(&d, &c).unwrap();
            assert_eq!(
                old.exact.start,
                ExactRatio::integer(offset)
                    .checked_add(ratio(3, 2))
                    .unwrap()
            );
            assert_eq!(old.exact.end, ExactRatio::integer(offset + 3));
            assert_eq!(old.frames, range(offset + 1, offset + 3));
            for value in [1, 4] {
                let (p, requests) = observed(&d, &c, vec![duration(value)]);
                assert_eq!(
                    requests,
                    [SemanticAllocationRequest::ParameterEdit { step_index: 0 }]
                );
                let after = visible(&p.document, &p.context).unwrap();
                assert_eq!(after.exact.start, old.exact.start);
                assert_eq!(
                    after.exact.end,
                    ExactRatio::integer(offset)
                        .checked_add(if value == 1 { ratio(9, 4) } else { ratio(9, 2) })
                        .unwrap()
                );
                assert_eq!(
                    after.frames,
                    range(offset + 1, offset + if value == 1 { 2 } else { 4 })
                );
                assert_eq!(p.context.cursor, ProjectFrame(offset + 1));
                check(&d, &c, &p);
            }
        }
    }
}

#[test]
fn shared_fixed_retime_duration_nonzero_crop_clips_before_picture_sampling() {
    for (crop, old_start, old_end, new_end, old_frames, new_frames) in [
        (
            1,
            ratio(69, 7),
            ratio(81, 7),
            ratio(93, 7),
            range(10, 12),
            range(10, 13),
        ),
        (
            3,
            ratio(9, 1),
            ratio(51, 5),
            ratio(63, 5),
            range(9, 10),
            range(9, 13),
        ),
    ] {
        let d = fixed(PitchPolicy::FollowSpeed, crop, 6);
        let c = authority(Some(1));
        let old = visible(&d, &c).unwrap();
        assert_eq!(
            (old.exact.start, old.exact.end, old.frames),
            (old_start, old_end, old_frames)
        );
        let (p, _) = observed(&d, &c, vec![duration(4)]);
        let after = visible(&p.document, &p.context).unwrap();
        assert_eq!(
            (after.exact.start, after.exact.end, after.frames),
            (old_start, new_end, new_frames)
        );
        assert_eq!(p.context.cursor, new_frames.start());
        check(&d, &c, &p);
    }
}

fn nested() -> (ProjectDocument, SemanticContext) {
    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    d.nodes.insert(
        node("stage"),
        stage("inner", 6, 1, 9, PitchPolicy::Preserve),
    );
    d.nodes
        .insert(node("inner"), repeated("inner", "inner-stage", 2));
    d.nodes.insert(
        node("inner-stage"),
        stage("body", 5, 0, 8, PitchPolicy::FollowSpeed),
    );
    d.nodes.insert(node("inner-owned"), hold(7));
    d.nodes.insert(node("outer-owned"), hold(4));
    d.nodes.insert(node("outer-gap"), hold(2));
    d.overrides.insert(
        node("inner"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration("inner", 0),
            root: node("inner-owned"),
        }])
        .unwrap(),
    );
    d.overrides.insert(
        node("outer"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration("outer", 0),
            root: node("outer-owned"),
        }])
        .unwrap(),
    );
    let NodeKind::Repeat { gap, .. } = &mut d.nodes.get_mut(&node("outer")).unwrap().kind else {
        panic!()
    };
    let NodeKind::Hold { recipe } = hold(1).kind else {
        panic!()
    };
    *gap = Some(recipe);
    d.gap_overrides.insert(
        node("outer"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration("outer", 0),
            root: node("outer-gap"),
        }])
        .unwrap(),
    );
    d.validate().unwrap();
    assert_eq!(d.duration().unwrap().frames(), 25);
    let mut c = authority(Some(1));
    let a = c.shared_hold_duration.as_mut().unwrap();
    a.target.repeats.push(RepeatEditStep {
        repeat: node("inner"),
        branch: RepeatEditBranch::Default,
    });
    a.presentation = Some(instance("held", 1, Some(1)));
    c.cursor = ProjectFrame(14);
    (d, c)
}

#[test]
fn shared_fixed_retime_duration_nested_repeat_offsets_include_sparse_gaps_and_reorder() {
    let (d, c) = nested();
    let v = visible(&d, &c).unwrap();
    assert_eq!(
        (v.exact.start, v.exact.end, v.frames),
        (ratio(231, 16), ratio(15, 1), range(14, 15))
    );
    let (p, _) = observed(&d, &c, vec![duration(4)]);
    assert_eq!(visible(&p.document, &p.context), Some(v));
    assert_eq!(
        p.document.nodes()[&node("inner-owned")],
        d.nodes()[&node("inner-owned")]
    );
    assert_eq!(
        p.document.nodes()[&node("outer-gap")],
        d.nodes()[&node("outer-gap")]
    );
    check(&d, &c, &p);

    let mut reordered = d.clone();
    for (owner, start, end, destination) in [("outer", 2, 3, 0), ("inner", 1, 2, 0)] {
        let NodeKind::Repeat { iterations, .. } =
            &mut reordered.nodes.get_mut(&node(owner)).unwrap().kind
        else {
            panic!()
        };
        *iterations = iterations.moved(start, end, destination).unwrap();
    }
    reordered.validate().unwrap();
    let v = visible(&reordered, &c).unwrap();
    assert_eq!(
        (v.exact.start, v.exact.end, v.frames),
        (ratio(259, 16), ratio(137, 8), range(16, 17))
    );
    let (p, _) = observed(&reordered, &c, vec![duration(4)]);
    check(&reordered, &c, &p);
}

#[test]
fn shared_fixed_retime_duration_dormant_defaults_and_million_plays_stay_compact() {
    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    for i in 0..3 {
        d.nodes
            .insert(node(&format!("owned-{i}")), hold(4 + i64::from(i)));
    }
    d.overrides.insert(
        node("outer"),
        PlayOverrides::try_from(
            (0..3)
                .map(|i| PlayOverride {
                    iteration: iteration("outer", i),
                    root: node(&format!("owned-{i}")),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    );
    d.validate().unwrap();
    let c = authority(None);
    assert!(visible(&d, &c).is_none());
    let (p, _) = observed(&d, &c, vec![duration(4), duration(1)]);
    assert_eq!(p.context.cursor, c.cursor);
    check(&d, &c, &p);
    let NodeKind::Retime {
        purpose,
        duration: stage_duration,
        mapping,
        ..
    } = &mut d.nodes.get_mut(&node("stage")).unwrap().kind
    else {
        panic!()
    };
    *purpose = RetimePurpose::Partition;
    *stage_duration = FrameDuration::new(10).unwrap();
    *mapping = range(0, 10);
    d.validate().unwrap();
    assert!(
        d.shared_hold_duration_visibility(&c.parent, c.shared_hold_duration.as_ref().unwrap())
            .is_err()
    );

    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    let NodeKind::Repeat { iterations, .. } = &mut d.nodes.get_mut(&node("outer")).unwrap().kind
    else {
        panic!()
    };
    *iterations = IterationOrder::new(repeat_allocation("outer"), 1_000_000).unwrap();
    d.validate().unwrap();
    let c = authority(Some(999_999));
    let v = visible(&d, &c).unwrap();
    assert_eq!(v.frames, range(5_999_998, 6_000_000));
    let (p, _) = observed(&d, &c, vec![duration(4)]);
    assert_eq!(p.context.cursor, ProjectFrame(5_999_998));
    check(&d, &c, &p);
}

#[test]
fn shared_fixed_retime_duration_named_counted_calls_keep_authority_after_clock_capture() {
    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    let media = fixture(true);
    d.assets = media.assets().clone();
    d.presentation_basis = media.presentation_basis().clone();
    let mut speech = media.nodes()[&node("before")].clone();
    let NodeKind::Source { source } = &mut speech.kind else {
        panic!()
    };
    source.duration = FrameDuration::new(6).unwrap();
    d.nodes.insert(node("speech"), speech);
    d.validate().unwrap();
    let c = authority(Some(1));
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![duration(4), duration(2)])),
    ]);
    let mut requests = vec![];
    let p = plan_semantic(
        &d,
        &c,
        &program(vec![call('a', 2)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("fixed-calls"),
        |r| {
            requests.push(r.clone());
            allocate(r)
        },
        no_original,
    )
    .unwrap();
    assert_eq!(
        requests,
        (0..4)
            .map(|step_index| SemanticAllocationRequest::ParameterEdit { step_index })
            .collect::<Vec<_>>()
    );
    assert!(!p.document.audio_bindings().is_empty());
    assert!(p.document.derived_clocks().is_empty());
    assert_eq!(
        visible(&p.document, &p.context).unwrap().frames,
        range(10, 12)
    );
    assert_eq!(p.context.cursor, ProjectFrame(10));
    check(&d, &c, &p);
}

#[test]
fn shared_fixed_retime_duration_hidden_unsampled_and_late_disappearance_refuse_atomically() {
    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 1);
    // [4,6) -> [.4,.6): one picture center; shrink to [4,5)
    // makes its Out exactly .5, excluded by the half-open center rule.
    let NodeKind::Hold { recipe } = &mut d.nodes.get_mut(&node("before")).unwrap().kind else {
        panic!()
    };
    recipe.duration = FrameDuration::new(4).unwrap();
    let NodeKind::Retime { mapping, .. } = &mut d.nodes.get_mut(&node("stage")).unwrap().kind
    else {
        panic!()
    };
    *mapping = range(0, 10);
    d.validate().unwrap();
    let mut c = authority(Some(1));
    c.cursor = ProjectFrame(4);
    assert_eq!(visible(&d, &c).unwrap().frames, range(4, 5));
    let first = observed(&d, &c, vec![duration(4)]).0;
    assert_eq!(
        visible(&first.document, &first.context).unwrap().frames,
        range(4, 5)
    );
    let snapshot = d.to_json().unwrap();
    let bank = BTreeMap::from([(name('a'), macro_value(vec![duration(4), duration(1)]))]);
    let saved = bank.clone();
    let mut allocations = 0;
    let error = plan_semantic(
        &d,
        &c,
        &program(vec![call('a', 1)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("fixed-late"),
        |r| {
            allocations += 1;
            allocate(r)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(allocations, 2);
    assert!(error.message.contains("no visible sampled"));
    assert_eq!(d.to_json().unwrap(), snapshot);
    assert_eq!(bank, saved);

    for hidden in [false, true] {
        let mut invalid = d.clone();
        if hidden {
            let NodeKind::Retime { mapping, .. } =
                &mut invalid.nodes.get_mut(&node("stage")).unwrap().kind
            else {
                panic!()
            };
            *mapping = range(0, 3);
        } else {
            let NodeKind::Hold { recipe } = &mut invalid.nodes.get_mut(&node("held")).unwrap().kind
            else {
                panic!()
            };
            recipe.duration = FrameDuration::new(1).unwrap();
        }
        invalid.validate().unwrap();
        let error = plan_semantic(
            &invalid,
            &c,
            &program(vec![duration(4)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("fixed-invisible"),
            |_| panic!("invisible Hold allocated"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        assert!(error.message.contains(if hidden {
            "hidden"
        } else {
            "no visible sampled"
        }));
    }
}

#[test]
fn shared_fixed_retime_duration_forged_presentations_and_other_scopes_remain_refused() {
    let d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    let c = authority(Some(1));
    let mut cases = vec![];
    let mut wrong = c.clone();
    wrong
        .shared_hold_duration
        .as_mut()
        .unwrap()
        .presentation
        .as_mut()
        .unwrap()
        .node = node("speech");
    cases.push(wrong);
    let mut wrong = c.clone();
    wrong
        .shared_hold_duration
        .as_mut()
        .unwrap()
        .presentation
        .as_mut()
        .unwrap()
        .repeats[0]
        .iteration = iteration("outer", 999);
    cases.push(wrong);
    let mut wrong = c.clone();
    wrong.shared_hold_duration.as_mut().unwrap().presentation = None;
    cases.push(wrong);
    let mut wrong = c.clone();
    wrong.shared_hold_duration.as_mut().unwrap().target.repeats[0].branch =
        RepeatEditBranch::Play {
            iteration: iteration("outer", 1),
        };
    cases.push(wrong);
    for wrong in cases {
        assert!(
            plan_semantic(
                &d,
                &wrong,
                &program(vec![duration(4)]),
                SemanticRegisterBank {
                    entries: &BTreeMap::new(),
                    version: 0
                },
                revision("fixed-forged"),
                |_| panic!("forged authority allocated"),
                no_original
            )
            .is_err()
        );
    }
    let ordinary = crate::SemanticHoldDurationContext {
        root: node("stage"),
        target: node("held"),
        presentation: InstancePath {
            node: node("held"),
            repeats: vec![],
        },
    };
    assert!(
        d.ordinary_hold_duration_visibility(&node("root"), &ordinary)
            .is_err()
    );
    // A second Repeat remains outside the one-play fixed-window increment.
    let (nested_document, nested_context) = nested();
    let path = nested_context
        .shared_hold_duration
        .unwrap()
        .presentation
        .unwrap();
    assert!(
        nested_document
            .occurrence_hold_duration_requirements(&path, FrameDuration::new(4).unwrap())
            .is_err()
    );
    let selected = crate::SemanticSelectedHoldDurationContext {
        root: node("outer"),
        target: ScopedNodeTarget {
            node: node("held"),
            repeats: vec![RepeatEditStep {
                repeat: node("outer"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration("outer", 0),
                },
            }],
        },
        also: vec![ScopedNodeTarget {
            node: node("held"),
            repeats: vec![RepeatEditStep {
                repeat: node("outer"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration("outer", 1),
                },
            }],
        }],
        presentation: Some(instance("held", 0, None)),
    };
    assert!(
        d.selected_hold_duration_visibility(&node("root"), &selected)
            .is_err()
    );
    let mut mixed = fixed(PitchPolicy::FollowSpeed, 0, 6);
    mixed
        .nodes
        .insert(node("inner"), repeated("inner", "body", 2));
    let NodeKind::Retime { child, .. } = &mut mixed.nodes.get_mut(&node("stage")).unwrap().kind
    else {
        panic!()
    };
    *child = node("inner");
    mixed.validate().unwrap();
    let target = ScopedNodeTarget {
        node: node("held"),
        repeats: vec![
            RepeatEditStep {
                repeat: node("outer"),
                branch: RepeatEditBranch::Default,
            },
            RepeatEditStep {
                repeat: node("inner"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration("inner", 0),
                },
            },
        ],
    };
    assert!(
        mixed
            .scoped_hold_duration_requirements(&target, FrameDuration::new(4).unwrap())
            .is_err()
    );
}

#[test]
fn shared_fixed_retime_duration_clocked_stage_is_refused_even_when_default_is_dormant() {
    let d = tree(
        &["stage"],
        vec![
            ("stage", stage("held", 24, 0, 12, PitchPolicy::FollowSpeed)),
            ("held", hold(12)),
        ],
    );
    let crate::PauseInsertionTarget::Retimed(site) =
        d.pause_insertion_target(ProjectFrame(3)).unwrap()
    else {
        panic!()
    };
    let NodeKind::Hold { recipe } = hold(3).kind else {
        panic!()
    };
    let commit = |d: &ProjectDocument, command, label: &str| {
        crate::apply_with_result(
            d,
            &crate::CommandRequest {
                project_id: d.project_id().clone(),
                expected_revision: d.revision_id().clone(),
                new_revision: revision(label),
                command,
            },
        )
        .unwrap()
        .1
    };
    let clocked = commit(
        &d,
        Command::InsertTime {
            at: ProjectFrame(3),
            hold: recipe,
            id: node("inserted"),
            identities: crate::SplitIdentities {
                nodes: (0..site.required_ids)
                    .map(|i| node(&format!("fixed-clock-split-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("fixed-clock-pause"),
                ordinal: 0,
            },
        },
        "fixed-clock-pause",
    );
    assert!(!clocked.derived_clocks().is_empty());
    let mut clocked = commit(
        &clocked,
        Command::WrapRepeat {
            node: node("stage"),
            id: node("outer"),
            plays: 3,
            gap: None,
            anchor_policy: Default::default(),
        },
        "fixed-clock-wrap",
    );
    let NodeKind::Repeat { iterations, .. } = &clocked.nodes()[&node("outer")].kind else {
        panic!()
    };
    let order = iterations.clone();
    let c = SemanticContext {
        selected_child: Some(node("outer")),
        shared_hold_duration: Some(SemanticSharedHoldDurationContext {
            root: node("outer"),
            target: ScopedNodeTarget {
                node: node("inserted"),
                repeats: vec![RepeatEditStep {
                    repeat: node("outer"),
                    branch: RepeatEditBranch::Default,
                }],
            },
            presentation: Some(InstancePath {
                node: node("inserted"),
                repeats: vec![crate::RepeatInstance {
                    node: node("outer"),
                    iteration: order.at(1).unwrap(),
                }],
            }),
        }),
        ..context("root", 3)
    };
    for dormant in [false, true] {
        let mut c = c.clone();
        if dormant {
            let entries = (0..3)
                .map(|i| {
                    let id = node(&format!("clocked-owned-{i}"));
                    clocked.nodes.insert(id.clone(), hold(4));
                    PlayOverride {
                        iteration: order.at(i).unwrap(),
                        root: id,
                    }
                })
                .collect::<Vec<_>>();
            clocked
                .overrides
                .insert(node("outer"), PlayOverrides::try_from(entries).unwrap());
            c.shared_hold_duration.as_mut().unwrap().presentation = None;
        }
        clocked.validate().unwrap();
        let error = plan_semantic(
            &clocked,
            &c,
            &program(vec![duration(4)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("fixed-clock-refuse"),
            |_| panic!("clocked authority allocated"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        assert!(error.message.contains("fixed unclocked Edit Retimes"));
    }
}

#[test]
fn shared_fixed_retime_duration_root_sound_policy_is_unchanged() {
    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    let media = fixture(true);
    d.assets = media.assets().clone();
    d.presentation_basis = media.presentation_basis().clone();
    let NodeKind::Hold { recipe } = &media.nodes()[&node("held")].kind else {
        panic!()
    };
    let HoldAudio::RoomTone { source } = &recipe.audio else {
        panic!()
    };
    d.sounds.insert(
        crate::SoundId::new("fixed-root-sound").unwrap(),
        crate::SoundEvent {
            owner: node("root"),
            label: "Root sound".into(),
            source: source.clone(),
            mapping: SourceAudioMapping::natural_rate(
                source.span,
                d.presentation_basis().frame_rate,
            )
            .unwrap(),
            offset: crate::AudioSample(37),
            treatments: crate::AudioTreatments::default(),
            start_edge: crate::AudioEdgePolicy::Hard,
            end_edge: crate::AudioEdgePolicy::Hard,
            overflow: crate::SoundOverflowPolicy::Reject,
        },
    );
    d.validate().unwrap();
    let c = authority(Some(1));
    assert!(visible(&d, &c).is_some());
    let snapshot = d.to_json().unwrap();
    let error = plan(&d, c, vec![duration(4)], &BTreeMap::new()).unwrap_err();
    assert!(
        error
            .message
            .contains("cannot yet preserve authored sound intervals")
    );
    assert_eq!(d.to_json().unwrap(), snapshot);
}
