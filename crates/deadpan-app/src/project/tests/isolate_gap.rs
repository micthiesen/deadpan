use super::*;
use deadpan_core::{IterationId, RepeatInstance};

fn plays(document: &deadpan_core::ProjectDocument, repeat: &NodeId) -> Vec<IterationId> {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[repeat].kind else {
        panic!("expected a Repeat");
    };
    (0..2).map(|index| iterations.at(index).unwrap()).collect()
}

#[test]
fn nested_isolate_gap_copies_only_the_chosen_outer_play() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("nested-gap.deadpan");
    {
        let mut store = seed_holds(&path, &["body"]);
        seed_command(
            &mut store,
            Command::WrapRepeat {
                node: node("body"),
                id: node("inner"),
                plays: 2,
                gap: Some(hold(3)),
                anchor_policy: Default::default(),
            },
            "wrap-inner",
        );
        seed_command(
            &mut store,
            Command::WrapRepeat {
                node: node("inner"),
                id: node("outer"),
                plays: 2,
                gap: None,
                anchor_policy: Default::default(),
            },
            "wrap-outer",
        );
    }
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let opened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let inner = plays(&opened.document, &node("inner"));
    let outer = plays(&opened.document, &node("outer"));
    let before = opened.document.duration().unwrap();
    let isolated = edited(
        &service,
        &opened,
        ProjectEdit::IsolateGap {
            node: node("inner"),
            iteration: inner[0].clone(),
            outer: vec![RepeatInstance {
                node: node("outer"),
                iteration: outer[1].clone(),
            }],
        },
    )
    .workspace
    .unwrap();
    let document = &isolated.document;
    // The default inner Repeat, heard in the first outer play, keeps no
    // owned gap; the second outer play owns a copy whose first gap is owned.
    assert!(document.gap_overrides().get(&node("inner")).is_none());
    let copy = document.overrides()[&node("outer")].get(&outer[1]).unwrap();
    assert_ne!(copy, &node("inner"));
    assert!(document.gap_overrides()[copy].get(&inner[0]).is_some());
    assert_eq!(document.duration().unwrap(), before);
}

#[test]
fn nested_isolate_gap_refuses_a_target_without_enclosing_plays_through_core() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("flat-gap.deadpan");
    {
        let mut store = seed_holds(&path, &["body"]);
        seed_command(
            &mut store,
            Command::WrapRepeat {
                node: node("body"),
                id: node("inner"),
                plays: 2,
                gap: Some(hold(3)),
                anchor_policy: Default::default(),
            },
            "wrap-inner",
        );
    }
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let opened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let instance = deadpan_core::InstancePath {
        node: node("inner"),
        repeats: Vec::new(),
    };
    assert!(
        opened
            .document
            .occurrence_gap_requirements(&instance)
            .is_err()
    );
}
