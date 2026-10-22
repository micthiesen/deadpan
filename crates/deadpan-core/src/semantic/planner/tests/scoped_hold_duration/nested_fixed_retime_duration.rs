//! Explicit ancestry is bounded by the document, not by a two-Repeat special case.
use super::*;

fn nested(depth: usize, pitch: PitchPolicy, crop: i64) -> (ProjectDocument, SemanticContext) {
    let mut d = fixed(pitch, crop, 6);
    let mut c = entry_fixed(&d, 1);
    let mut child = node("stage");
    let mut steps = Vec::new();
    for level in (1..depth).rev() {
        let name = format!("nested-{level}");
        let id = node(&name);
        d.nodes
            .insert(id.clone(), repeated(&name, child.as_str(), 2));
        steps.push(RepeatInstance {
            node: id.clone(),
            iteration: iteration(&name, 1),
        });
        child = id;
    }
    let NodeKind::Repeat { child: body, .. } = &mut d.nodes.get_mut(&node("outer")).unwrap().kind
    else {
        panic!()
    };
    *body = child;
    steps.reverse();
    let scoped = c.scoped.as_mut().unwrap();
    for step in steps {
        scoped.target.repeats.push(RepeatEditStep {
            repeat: step.node.clone(),
            branch: RepeatEditBranch::Play {
                iteration: step.iteration.clone(),
            },
        });
        scoped.presentation.repeats.push(step);
    }
    d.validate().unwrap();
    c.cursor = visible(&d, &c).frames.start();
    (d, c)
}

fn run(
    d: &ProjectDocument,
    c: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> SemanticPlan {
    plan(d, c, instructions, &BTreeMap::new(), |request| {
        super::super::super::allocator(d.revision_id().as_str(), request)
    })
    .unwrap()
}

#[test]
fn nested_fixed_occurrence_duration_two_three_and_five_levels_map_actual_proofs() {
    for depth in [2, 3, 5] {
        for pitch in [
            PitchPolicy::FollowSpeed,
            PitchPolicy::Preserve,
            PitchPolicy::Shift { semitones: -2 },
        ] {
            for crop in [0, 3] {
                let (d, c) = nested(depth, pitch, crop);
                let old = visible(&d, &c);
                let total = d.duration().unwrap();
                let needs = d
                    .occurrence_hold_duration_requirements(
                        &c.scoped.as_ref().unwrap().presentation,
                        FrameDuration::new(4).unwrap(),
                    )
                    .unwrap();
                let copies = 5 * depth + depth * (depth - 1) / 2;
                assert_eq!((needs.nodes, needs.marks), (copies, 0));
                let p = run(&d, &c, vec![duration(4)]);
                assert_eq!(p.document.duration().unwrap(), total);
                assert_eq!(p.document.nodes().len(), d.nodes().len() + copies);
                assert_eq!(p.context.cursor, old.frames.start());
                for (id, original) in d.nodes() {
                    assert_eq!(&p.document.nodes()[id], original);
                }
                let Command::Compound { transaction } = &p.request.as_ref().unwrap().command else {
                    panic!()
                };
                let leaf = transaction.steps()[0].edit().unwrap().request(&d);
                let prepared = crate::prepare_occurrence_hold_duration(&d, &leaf).unwrap();
                assert_eq!(prepared.isolation().steps().len(), depth);
                prepared
                    .isolation()
                    .validate(&d, &leaf, &prepared.document)
                    .unwrap();
                assert_eq!(
                    prepared.presentation,
                    p.context.scoped.as_ref().unwrap().presentation
                );
                assert_eq!(prepared.presentation.repeats.len(), depth);
                assert_eq!(
                    prepared.presentation.repeats[0],
                    c.scoped.as_ref().unwrap().presentation.repeats[0]
                );
                for (before, after) in c
                    .scoped
                    .as_ref()
                    .unwrap()
                    .presentation
                    .repeats
                    .iter()
                    .skip(1)
                    .zip(prepared.presentation.repeats.iter().skip(1))
                {
                    assert_ne!(before.node, after.node);
                    assert_eq!(before.iteration, after.iteration);
                }
                assert_eq!(
                    visible(&p.document, &p.context).exact.start,
                    old.exact.start
                );
                assert_eq!(prepared.boundary.is_none(), crop == 3);
                reversible(&d, &p);
                let next = run(&p.document, &p.context, vec![duration(3), duration(4)]);
                assert_eq!(next.document.nodes().len(), p.document.nodes().len());
                assert_eq!(next.context.scoped, p.context.scoped);
                reversible(&p.document, &next);
            }
        }
    }
}

#[test]
fn nested_fixed_occurrence_duration_staged_isolation_exact_pools_and_invalid_ancestry() {
    let (d, c) = nested(3, PitchPolicy::FollowSpeed, 0);
    let p = run(&d, &c, vec![duration(4), duration(1)]);
    assert_eq!(p.document.nodes().len(), d.nodes().len() + 18);
    assert_eq!(p.document.duration().unwrap(), d.duration().unwrap());
    reversible(&d, &p);
    let good = request(&d, &c.scoped.as_ref().unwrap().presentation, 4);
    let original = d.to_json().unwrap();
    for bad in 0..6 {
        let mut r = good.clone();
        let Command::EditOccurrence { identities, .. } = &mut r.command else {
            panic!()
        };
        match bad {
            0 => {
                identities.nodes.pop();
            }
            1 => identities.nodes.push(node("excess-nested-copy")),
            2 => identities.nodes[0] = node("held"),
            3 => identities.nodes[1] = identities.nodes[0].clone(),
            4 => identities
                .marks
                .push(MarkId::new("unneeded-nested-mark").unwrap()),
            5 => r.expected_revision = revision("stale-nested"),
            _ => unreachable!(),
        }
        assert!(crate::apply(&d, &r).is_err(), "invalid nested pool {bad}");
        assert!(crate::prepare_occurrence_hold_duration(&d, &r).is_err());
        assert_eq!(d.to_json().unwrap(), original);
    }
    for bad in 0..3 {
        let mut wrong = d.clone();
        let mut context = c.clone();
        match bad {
            0 => {
                let NodeKind::Retime {
                    purpose,
                    duration,
                    mapping,
                    ..
                } = &mut wrong.nodes.get_mut(&node("stage")).unwrap().kind
                else {
                    panic!()
                };
                *purpose = RetimePurpose::Partition;
                *duration = mapping.duration();
            }
            1 => {
                context.scoped.as_mut().unwrap().target.repeats[1].branch =
                    RepeatEditBranch::Default;
            }
            2 => {
                context.scoped.as_mut().unwrap().root = node("nested-1");
            }
            _ => unreachable!(),
        }
        wrong.validate().unwrap();
        assert!(
            plan(
                &wrong,
                &context,
                vec![duration(4)],
                &BTreeMap::new(),
                |_| panic!("unsupported nested context allocated")
            )
            .is_err()
        );
    }
}

#[test]
fn nested_fixed_occurrence_duration_partly_owned_sparse_reordered_plays_keep_offsets() {
    let (mut d, mut c) = nested(3, PitchPolicy::FollowSpeed, 0);
    for name in ["outer", "nested-1", "nested-2"] {
        let NodeKind::Repeat { iterations, .. } = &mut d.nodes.get_mut(&node(name)).unwrap().kind
        else {
            panic!()
        };
        // Preserve the selected identity (ordinal 1) while retiring identity 0
        // and moving two independently allocated Plays ahead of it. Its
        // stable ordinal 1 is now physical position 2 at every level.
        *iterations = crate::IterationOrder::from_segments([
            (revision(&format!("sparse-{name}")), 40, 2),
            (iteration(name, 1).allocation, 1, 1),
        ])
        .unwrap();
    }
    d.nodes.insert(node("earlier-owned"), hold(7));
    let NodeKind::Repeat { iterations, .. } = &d.nodes()[&node("outer")].kind else {
        panic!()
    };
    d.overrides.insert(
        node("outer"),
        crate::PlayOverrides::try_from(vec![crate::PlayOverride {
            iteration: iterations.at(0).unwrap(),
            root: node("earlier-owned"),
        }])
        .unwrap(),
    );
    d.validate().unwrap();
    // The selected stage begins after 3 + 7 + 54 + 36 + 12 frames, rather than an
    // ordinal-based multiple of an assumed uniform outer duration.
    assert_eq!(visible(&d, &c).frames, range(113, 115));
    c.cursor = ProjectFrame(113);
    let original = c.scoped.as_ref().unwrap().clone();
    let mut partial_target = original.target.clone();
    partial_target.node = node("nested-1");
    partial_target.repeats.truncate(1);
    let edit = crate::ScopedNodeEdit::Rename {
        label: "Owned family".into(),
    };
    let needs = d.scoped_edit_requirements(&partial_target, &edit).unwrap();
    assert_eq!((needs.nodes, needs.marks), (7, 0));
    let r = CommandRequest {
        project_id: d.project_id().clone(),
        expected_revision: d.revision_id().clone(),
        new_revision: revision("partly-owned-nested"),
        command: Command::EditScoped {
            target: partial_target,
            edit,
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|i| node(&format!("partial-{i}")))
                    .collect(),
                marks: vec![],
            },
        },
    };
    let prepared = crate::prepare_scoped_edit(&d, &r).unwrap();
    let mut owned_context = c.clone();
    let selected = owned_context.scoped.as_mut().unwrap();
    selected.target = prepared.map_target(&d, &original.target).unwrap();
    selected.presentation = prepared.map_instance(&d, &original.presentation).unwrap();
    assert_eq!(prepared.isolation().steps().len(), 1);
    let needs = prepared
        .document
        .occurrence_hold_duration_requirements(
            &selected.presentation,
            FrameDuration::new(4).unwrap(),
        )
        .unwrap();
    assert_eq!((needs.nodes, needs.marks), (11, 0));
    let p = run(&prepared.document, &owned_context, vec![duration(4)]);
    assert_eq!(
        p.document.nodes().len(),
        prepared.document.nodes().len() + 11
    );
    assert_eq!(p.document.duration().unwrap(), d.duration().unwrap());
    assert_eq!(visible(&p.document, &p.context).frames, range(113, 116));
    assert_eq!(
        p.document.nodes()[&node("earlier-owned")],
        d.nodes()[&node("earlier-owned")]
    );
    reversible(&prepared.document, &p);
}
