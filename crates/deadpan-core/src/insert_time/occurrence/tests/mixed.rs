use super::scoped::instance;
use super::*;

fn authority(outer: Option<u32>, inner: Option<u32>) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: id("a"),
        repeats: [("outer", outer), ("inner", inner)]
            .into_iter()
            .map(|(name, play)| RepeatEditStep {
                repeat: id(name),
                branch: play.map_or(RepeatEditBranch::Default, |ordinal| {
                    RepeatEditBranch::Play {
                        iteration: IterationId {
                            allocation: revision(name),
                            ordinal,
                        },
                    }
                }),
            })
            .collect(),
    }
}

fn scoped_request(
    document: &ProjectDocument,
    at: i64,
    authority: ScopedNodeTarget,
    within: InstancePath,
    name: &str,
) -> CommandRequest {
    let target = document
        .pause_insertion_scoped(ProjectFrame(at), &authority, &within)
        .unwrap();
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::InsertOccurrenceTime {
            at: ProjectFrame(at),
            within: Some(within),
            target: Some(authority),
            hold: hold(1),
            id: id(&format!("{name}-pause")),
            ai: false,
            all_plays: false,
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
fn mixed_pause_multiplies_only_default_branches_and_reports_final_inserted_start() {
    let before = fixture(true);
    for outer in [0, 2] {
        for inner in [0, 1] {
            for local in [0, 1, 4] {
                for (outer_default, inner_default, delta) in [
                    (true, false, 3),
                    (false, true, 2),
                    (true, true, 6),
                    (false, false, 1),
                ] {
                    let target = authority(
                        (!outer_default).then_some(outer),
                        (!inner_default).then_some(inner),
                    );
                    let within = instance("a", &[("outer", outer), ("inner", inner)]);
                    let at = i64::from(outer) * 13 + i64::from(inner) * 5 + local;
                    let request = scoped_request(&before, at, target, within, "mixed");
                    let prepared = prepare_occurrence_time(&before, &request).unwrap_or_else(|error| panic!("outer={outer} inner={inner} local={local} defaults={outer_default}/{inner_default}: {error}"));
                    assert_eq!(prepared.transaction.duration_delta, delta);
                    assert_eq!(
                        prepared.document.duration().unwrap().frames(),
                        before.duration().unwrap().frames() + delta
                    );
                    let earlier_outer = if outer_default {
                        i64::from(outer) * if inner_default { 2 } else { 1 }
                    } else {
                        0
                    };
                    let earlier_inner = if inner_default { i64::from(inner) } else { 0 };
                    assert_eq!(
                        prepared.boundary.exact_frame,
                        ExactRatio::integer(at + earlier_outer + earlier_inner)
                    );
                    assert_eq!(prepared.presentation.node, id("mixed-pause"));
                    assert!(
                        prepared
                            .target
                            .matches_instance(&prepared.document, &prepared.presentation)
                            .unwrap()
                    );
                    assert_eq!(prepared.presentation.repeats[0].iteration.ordinal, outer);
                    assert_eq!(prepared.presentation.repeats[1].iteration.ordinal, inner);
                    assert_eq!(
                        prepared
                            .transaction
                            .inverse
                            .apply(&prepared.document)
                            .unwrap(),
                        before
                    );
                    assert_eq!(
                        prepared.transaction.forward.apply(&before).unwrap(),
                        prepared.document
                    );
                    assert_eq!(
                        ProjectDocument::from_json(&prepared.document.to_json().unwrap()).unwrap(),
                        prepared.document
                    );
                    let wire: CommandRequest =
                        serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
                    assert_eq!(wire, request);
                    prepared
                        .isolation()
                        .validate(&before, &request, &prepared.document)
                        .unwrap();
                }
            }
        }
    }
}

#[test]
fn mixed_pause_can_inspect_a_repeat_owner_and_maps_unedited_counterparts() {
    let before = fixture(true);
    // Explicit Default at the outer level, with the concrete inner choice
    // derived below the inspected owner. This is Default/Play, not Default/Default.
    let target = ScopedNodeTarget {
        node: id("inner"),
        repeats: vec![RepeatEditStep {
            repeat: id("outer"),
            branch: RepeatEditBranch::Default,
        }],
    };
    let within = instance("inner", &[("outer", 2)]);
    let request = scoped_request(&before, 32, target, within, "owner");
    let prepared = prepare_occurrence_time(&before, &request).unwrap();
    assert_eq!(prepared.transaction.duration_delta, 3);
    assert_eq!(prepared.boundary.frame, ProjectFrame(34));
    let untouched = authority(Some(0), Some(0));
    let untouched_instance = instance("a", &[("outer", 0), ("inner", 0)]);
    assert_eq!(prepared.map_target(&before, &untouched).unwrap(), untouched);
    assert_eq!(
        prepared.map_instance(&before, &untouched_instance).unwrap(),
        untouched_instance
    );
    assert!(prepared.map_target(&prepared.document, &untouched).is_err());
    assert!(
        prepared
            .map_instance(&prepared.document, &untouched_instance)
            .is_err()
    );
    // The outermost Repeat itself has empty concrete ancestry. Its inspected
    // boundary still descends into concrete plays, without requiring a fake step.
    let target = ScopedNodeTarget {
        node: id("outer"),
        repeats: vec![],
    };
    let request = scoped_request(&before, 32, target, instance("outer", &[]), "outer-owner");
    let prepared = prepare_occurrence_time(&before, &request).unwrap();
    assert_eq!(prepared.transaction.duration_delta, 1);
    assert_eq!(prepared.boundary.frame, ProjectFrame(32));
}

fn own(document: &ProjectDocument, target: ScopedNodeTarget, name: &str) -> ProjectDocument {
    let edit = ScopedNodeEdit::Rename { label: name.into() };
    let requirements = document.scoped_edit_requirements(&target, &edit).unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::EditScoped {
            target,
            edit,
            identities: OccurrenceIdentities {
                nodes: (0..requirements.nodes)
                    .map(|n| id(&format!("{name}-{n}")))
                    .collect(),
                marks: (0..requirements.marks)
                    .map(|n| MarkId::new(format!("{name}-{n}")).unwrap())
                    .collect(),
            },
        },
    };
    prepare_scoped_edit(document, &request).unwrap().document
}

#[test]
fn mixed_pause_excludes_existing_default_overrides_and_keeps_stable_iteration_orders() {
    let before = fixture(true);
    let before = own(&before, authority(Some(1), None), "owned-outer");
    let original_override = before.overrides()[&id("outer")]
        .iter()
        .next()
        .unwrap()
        .1
        .clone();
    let request = scoped_request(
        &before,
        32,
        authority(None, Some(1)),
        instance("a", &[("outer", 2), ("inner", 1)]),
        "default-outer",
    );
    let prepared = prepare_occurrence_time(&before, &request).unwrap();
    assert_eq!(prepared.transaction.duration_delta, 2);
    assert_eq!(
        prepared.document.nodes()[&original_override],
        before.nodes()[&original_override]
    );
    assert_eq!(prepared.boundary.frame, ProjectFrame(33));
    for repeat in ["outer", "inner"] {
        let NodeKind::Repeat {
            iterations: old, ..
        } = &before.nodes()[&id(repeat)].kind
        else {
            panic!()
        };
        let NodeKind::Repeat {
            iterations: new, ..
        } = &prepared.document.nodes()[&id(repeat)].kind
        else {
            panic!()
        };
        assert_eq!(new, old);
    }
    let before = own(&fixture(true), authority(None, Some(0)), "owned-inner");
    let request = scoped_request(
        &before,
        32,
        authority(Some(2), None),
        instance("a", &[("outer", 2), ("inner", 1)]),
        "default-inner",
    );
    let prepared = prepare_occurrence_time(&before, &request).unwrap();
    assert_eq!(prepared.transaction.duration_delta, 1);
    assert_eq!(
        prepared
            .transaction
            .inverse
            .apply(&prepared.document)
            .unwrap(),
        before
    );
}

#[test]
fn mixed_pause_rejects_ambiguous_authority_and_nonexact_identity_pools_atomically() {
    let before = fixture(true);
    let within = instance("a", &[("outer", 2), ("inner", 1)]);
    let request = scoped_request(
        &before,
        32,
        authority(Some(2), None),
        within.clone(),
        "strict",
    );
    for case in 0..6 {
        let mut bad = request.clone();
        let Command::InsertOccurrenceTime {
            target,
            within,
            all_plays,
            identities,
            ..
        } = &mut bad.command
        else {
            panic!()
        };
        match case {
            0 => *all_plays = true,
            1 => *within = None,
            2 => target.as_mut().unwrap().node = id("b"),
            3 => {
                target.as_mut().unwrap().repeats[0].branch = RepeatEditBranch::Play {
                    iteration: IterationId {
                        allocation: revision("outer"),
                        ordinal: 0,
                    },
                }
            }
            4 => {
                identities.isolation.nodes.pop();
            }
            5 => identities.isolation.nodes.push(id("extra")),
            _ => unreachable!(),
        }
        assert!(
            prepare_occurrence_time(&before, &bad).is_err(),
            "case {case}"
        );
    }
    assert_eq!(before, fixture(true));
}

#[test]
fn mixed_pause_root_bus_inserts_at_the_shared_inner_end_not_the_outer_end() {
    let mut before = with_root_sound(fixture(true));
    before
        .sounds
        .get_mut(&SoundId::new("after").unwrap())
        .unwrap()
        .offset = before
        .presentation_basis()
        .frame_rate
        .audio_boundary(ProjectFrame(23))
        .unwrap();
    before.validate().unwrap();
    let request = scoped_request(
        &before,
        19,
        authority(Some(1), None),
        instance("a", &[("outer", 1), ("inner", 1)]),
        "sound-inner",
    );
    let prepared = prepare_occurrence_time(&before, &request).unwrap();
    assert_eq!(prepared.transaction.duration_delta, 2);
    let route = &prepared.document.sound_routes()[&SoundId::new("after").unwrap()];
    assert_eq!(route.edits.len(), 1);
    assert_eq!(
        route.edits[0].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(22),
            duration: FrameDuration::new(2).unwrap()
        }
    );
    assert_eq!(
        prepared
            .transaction
            .inverse
            .apply(&prepared.document)
            .unwrap(),
        before
    );
}

#[test]
fn mixed_pause_retains_local_and_occurrence_marks_with_exact_mark_pools() {
    let mut before = fixture(true);
    for (name, coordinate) in [
        (
            "local",
            Anchor::Local {
                node: id("a"),
                position: ExactRatio::integer(3),
            },
        ),
        (
            "occurrence",
            Anchor::Occurrence {
                instance: instance("a", &[("outer", 2), ("inner", 1)]),
                position: ExactRatio::integer(3),
            },
        ),
    ] {
        before.marks.insert(
            MarkId::new(name).unwrap(),
            Mark {
                owner: id("a"),
                label: name.into(),
                boundary: BoundaryAnchor {
                    coordinate,
                    bias: InsertionBias::Right,
                },
                loss_policy: AnchorLossPolicy::KeepUnresolved,
                state: MarkState::Bound,
                fragments: vec![],
            },
        );
    }
    before.validate().unwrap();
    for target in [authority(None, Some(1)), authority(Some(2), None)] {
        let request = scoped_request(
            &before,
            32,
            target,
            instance("a", &[("outer", 2), ("inner", 1)]),
            "marks",
        );
        let prepared = prepare_occurrence_time(&before, &request).unwrap();
        assert!(
            prepared
                .document
                .marks()
                .values()
                .all(|mark| mark.state == MarkState::Bound)
        );
        assert_eq!(
            prepared
                .transaction
                .inverse
                .apply(&prepared.document)
                .unwrap(),
            before
        );
        let mut bad = request.clone();
        let Command::InsertOccurrenceTime { identities, .. } = &mut bad.command else {
            panic!()
        };
        assert!(!identities.isolation.marks.is_empty());
        identities.isolation.marks.pop();
        assert!(prepare_occurrence_time(&before, &bad).is_err());
    }
}

#[test]
fn prepared_occurrence_pause_maps_materialized_gaps_without_sampling_an_old_cursor() {
    let before = fixture(true);
    for at in [4, 12, 17, 25] {
        let request = request(&before, ProjectFrame(at), "gap-receipt");
        let prepared = prepare_occurrence_time(&before, &request).unwrap();
        assert_eq!(prepared.boundary.frame, ProjectFrame(at));
        assert_eq!(prepared.presentation.node, id("gap-receipt-pause"));
        assert!(
            prepared
                .target
                .matches_instance(&prepared.document, &prepared.presentation)
                .unwrap()
        );
        assert_eq!(
            prepared
                .transaction
                .inverse
                .apply(&prepared.document)
                .unwrap(),
            before
        );
    }
}

#[test]
fn mixed_pause_shared_suffix_supports_direct_repeat_to_repeat_ancestry() {
    let mut before = fixture(true);
    let NodeKind::Repeat { child, .. } = &mut before.nodes.get_mut(&id("outer")).unwrap().kind
    else {
        panic!()
    };
    *child = id("inner");
    before.nodes.remove(&id("body"));
    before.nodes.remove(&id("b"));
    before.validate().unwrap();
    // Outer plays [0,9), [11,20), [22,31), with the tail after31.
    // A shared inner edit in outer play1 must also retain its following gap
    // and the later outer play, despite there being no Sequence between them.
    let request = scoped_request(
        &before,
        17,
        authority(Some(1), None),
        instance("a", &[("outer", 1), ("inner", 1)]),
        "direct",
    );
    let prepared = prepare_occurrence_time(&before, &request).unwrap();
    assert_eq!(prepared.transaction.duration_delta, 2);
    assert_eq!(prepared.boundary.frame, ProjectFrame(18));
    assert_eq!(
        prepared
            .transaction
            .inverse
            .apply(&prepared.document)
            .unwrap(),
        before
    );
    assert_eq!(
        ProjectDocument::from_json(&prepared.document.to_json().unwrap()).unwrap(),
        prepared.document
    );
}
