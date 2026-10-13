use super::*;
use crate::*;
use std::sync::Arc;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn clock(n: i128, d: i128) -> DerivedRetimeClock {
    DerivedRetimeClock {
        extent: ratio(n, d),
        child_start: ExactRatio::ZERO,
        child_end: ratio(2, 1),
    }
}
fn document(derived: bool) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("clocks").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    document.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("first"), id("last")]),
    );
    for (owner, child) in [("first", "left"), ("last", "right")] {
        let mut node = BeatNode::sequence("Retime", vec![]);
        node.kind = NodeKind::Retime {
            child: id(child),
            duration: frames(1),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        };
        document.nodes.insert(id(owner), node);
        document.nodes.insert(
            id(child),
            BeatNode::hold(
                "Hold",
                HoldRecipe {
                    duration: frames(2),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
            ),
        );
    }
    if derived {
        document.derived_clocks =
            BTreeMap::from([(id("first"), clock(1, 2)), (id("last"), clock(5, 2))]);
    }
    document
}

#[test]
fn persisted_clocks_keep_recipe_counts_and_exact_root_separate() {
    let doc = document(true);
    let wire = doc.to_compact_json().unwrap();
    let reopened = ProjectDocument::from_json(&wire).unwrap();
    assert_eq!(reopened, doc);
    assert_eq!(doc.duration().unwrap(), frames(3));
    assert_eq!(doc.structural_duration().unwrap(), frames(3));
    assert_eq!(
        doc.recipe_durations_shared().unwrap()[doc.root()],
        frames(2)
    );
    assert_eq!(doc.exact_durations().unwrap()[&id("last")], ratio(5, 2));
    assert!(doc.node_duration(&id("last")).is_err());
    let validated = ValidatedDocument::new(Arc::new(doc)).unwrap();
    validated.scope(|| {
        assert!(validated.durations().is_err());
        assert!(validated.document().durations().is_err());
        assert!(validated.document().durations_shared().is_err());
        assert_eq!(validated.duration().unwrap(), frames(3));
    });
    validated.check_against_complete_validation().unwrap();
}

#[test]
fn restore_and_inverse_include_clock_only_changes_and_actual_delta() {
    let before = document(false);
    let mut after = document(true);
    after.revision_id = revision("changed");
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: after.revision_id().clone(),
        command: Command::RestoreSnapshot {
            snapshot: Box::new(after.clone()),
        },
    };
    let validated = ValidatedDocument::new(Arc::new(before.clone())).unwrap();
    let (edit, result) = apply_validated(&validated, &request).unwrap();
    assert_eq!(edit.duration_delta, 1);
    assert_eq!(edit.changed_ids, vec![id("first"), id("last")]);
    assert!(edit.forward.nodes.is_empty());
    assert_eq!(edit.forward.derived_clocks.len(), 2);
    assert_eq!(result.document().as_ref(), &after);
    let wire = serde_json::to_string(&edit).unwrap();
    let decoded: EditTransaction = serde_json::from_str(&wire).unwrap();
    assert_eq!(decoded, edit);
    assert_eq!(edit.forward.apply(&before).unwrap(), after);
    assert!(edit.forward.restores(&before, &after).unwrap());
    assert_eq!(edit.inverse.apply(&after).unwrap(), before);
    let mut stale = after.clone();
    stale.derived_clocks.insert(id("first"), clock(3, 2));
    assert!(edit.inverse.apply(&stale).is_err());
    assert!(edit.inverse.restores(&stale, &before).is_err());
    // A cached recipe count must not turn the restore-back delta into zero.
    let restore = CommandRequest {
        project_id: after.project_id().clone(),
        expected_revision: after.revision_id().clone(),
        new_revision: revision("back"),
        command: Command::RestoreSnapshot {
            snapshot: Box::new(before),
        },
    };
    assert_eq!(
        apply_validated(&result, &restore).unwrap().0.duration_delta,
        -1
    );
}

#[test]
fn legacy_edit_refuses_before_mutation() {
    let doc = document(true);
    let request = CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: revision("edit"),
        command: Command::Rename {
            node: id("root"),
            label: "Changed".into(),
        },
    };
    assert!(
        apply(&doc, &request)
            .unwrap_err()
            .message
            .contains("exact clock consumer")
    );
    assert_eq!(doc, document(true));
}

#[test]
fn malformed_effective_clocks_and_unsupported_preserve_fail_admission() {
    for change in 0..8 {
        let mut doc = document(true);
        match change {
            0 => doc.derived_clocks.get_mut(&id("first")).unwrap().extent = ExactRatio::ZERO,
            1 => {
                doc.derived_clocks
                    .get_mut(&id("first"))
                    .unwrap()
                    .child_start = ratio(-1, 2)
            }
            2 => doc.derived_clocks.get_mut(&id("first")).unwrap().child_end = ratio(5, 2),
            3 => doc.derived_clocks.get_mut(&id("first")).unwrap().extent = ratio(1, 3),
            4 => {
                doc.derived_clocks.insert(id("absent"), clock(1, 2));
            }
            5 => {
                doc.derived_clocks.insert(id("left"), clock(1, 2));
            }
            6 => {
                if let NodeKind::Retime { pitch, .. } =
                    &mut doc.nodes.get_mut(&id("last")).unwrap().kind
                {
                    *pitch = PitchPolicy::Preserve;
                }
            }
            7 => doc.derived_clocks.get_mut(&id("first")).unwrap().child_end = ExactRatio::ZERO,
            _ => unreachable!(),
        }
        assert!(doc.validate().is_err(), "case {change}");
        assert!(
            ProjectDocument::from_json(&serde_json::to_string(&doc).unwrap()).is_err(),
            "wire {change}"
        );
    }
}

#[test]
fn repeat_compiles_fractional_children_without_expanding_plays() {
    let mut doc = document(true);
    doc.nodes.get_mut(&id("root")).unwrap().kind = NodeKind::Sequence {
        children: vec![id("repeat"), id("last")],
    };
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("first"),
        iterations: IterationOrder::new(revision("plays"), 1_000_000_001).unwrap(),
        gap: None,
        escalation: None,
    };
    doc.nodes.insert(id("repeat"), repeat);
    assert_eq!(
        doc.exact_durations().unwrap()[&id("repeat")],
        ratio(1_000_000_001, 2)
    );
    assert_eq!(doc.duration().unwrap(), frames(500_000_003));
}

#[test]
fn exact_mapping_checks_unchanged_ancestors_and_nested_clocks() {
    let mut doc = document(true);
    doc.nodes.get_mut(&id("first")).unwrap().kind = NodeKind::Retime {
        child: id("last"),
        duration: frames(1),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    doc.nodes.remove(&id("left"));
    doc.nodes.get_mut(&id("root")).unwrap().kind = NodeKind::Sequence {
        children: vec![id("first")],
    };
    doc.derived_clocks.insert(
        id("first"),
        DerivedRetimeClock {
            extent: ratio(3, 1),
            child_start: ratio(1, 2),
            child_end: ratio(5, 2),
        },
    );
    assert_eq!(doc.duration().unwrap(), frames(3));
    doc.derived_clocks.remove(&id("first"));
    doc.derived_clocks.insert(id("last"), clock(1, 2));
    assert!(
        doc.validate()
            .unwrap_err()
            .message
            .contains("window inside child")
    );
}

#[test]
fn duplicate_clock_keys_and_old_schema_are_rejected() {
    let doc = document(true);
    let wire = serde_json::to_string(&doc).unwrap();
    let value = serde_json::to_string(&clock(1, 2)).unwrap();
    let duplicate = wire.replacen(
        "\"derived_clocks\":{",
        &format!("\"derived_clocks\":{{\"first\":{value},"),
        1,
    );
    assert!(ProjectDocument::from_json(&duplicate).is_err());
    let mut old = serde_json::to_value(doc).unwrap();
    old["schema_version"] = serde_json::json!(DOCUMENT_SCHEMA_VERSION - 1);
    assert!(ProjectDocument::from_json(&old.to_string()).is_err());
}

#[test]
fn fractional_partitions_require_unity_in_the_effective_clock() {
    let mut doc = document(true);
    let NodeKind::Retime {
        mapping, purpose, ..
    } = &mut doc.nodes.get_mut(&id("first")).unwrap().kind
    else {
        unreachable!()
    };
    *mapping = FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap();
    *purpose = RetimePurpose::Partition;
    let clock = doc.derived_clocks.get_mut(&id("first")).unwrap();
    clock.child_start = ratio(1, 4);
    clock.child_end = ratio(3, 4);
    doc.validate().unwrap();
    doc.derived_clocks.get_mut(&id("first")).unwrap().child_end = ratio(1, 1);
    assert!(doc.validate().unwrap_err().message.contains("unity timing"));
}

#[test]
fn effective_speed_refuses_hanging_tails_even_when_recipe_is_unity() {
    let mut doc = document(true);
    let NodeKind::Retime { mapping, .. } = &mut doc.nodes.get_mut(&id("first")).unwrap().kind
    else {
        unreachable!()
    };
    *mapping = FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap();
    let NodeKind::Hold { recipe } = &mut doc.nodes.get_mut(&id("left")).unwrap().kind else {
        unreachable!()
    };
    recipe.audio = HoldAudio::Tail {
        maximum: frames(1),
        effect: TailEffect::Reverb,
        input: TailInput::Original,
    };
    assert!(doc.validate().unwrap_err().message.contains("hanging tail"));
    doc.derived_clocks.get_mut(&id("first")).unwrap().child_end = ratio(1, 2);
    doc.validate().unwrap();
}

#[test]
fn unrepresentable_effective_slope_is_rejected_without_rounding() {
    let mut doc = document(true);
    doc.derived_clocks.get_mut(&id("first")).unwrap().extent = ratio(1, i128::MAX);
    assert_eq!(
        doc.validate().unwrap_err().code,
        DocumentErrorCode::TimingOverflow
    );
}
