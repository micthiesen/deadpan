//! A real editable union retains source clocks and shared logical ownership.

#[path = "edit_set/fixture.rs"]
mod fixture;
#[path = "edit_set/semantic.rs"]
mod semantic;

use deadpan_core::*;
use fixture::*;
use std::collections::BTreeMap;

fn pools(set: &CapturedEditSet, prefix: &str) -> EditSetPasteIdentities {
    let required = set.identity_requirements().unwrap();
    EditSetPasteIdentities {
        group: node(&format!("{prefix}-group")),
        components: required
            .components
            .iter()
            .enumerate()
            .map(|(member, required)| {
                assert_eq!(required.marks, 0);
                EditSetComponentIdentities {
                    nodes: (0..required.nodes)
                        .map(|index| node(&format!("{prefix}-member-{member}-node-{index}")))
                        .collect(),
                    aliases: (0..required.aliases)
                        .map(|index| node(&format!("{prefix}-member-{member}-alias-{index}")))
                        .collect(),
                }
            })
            .collect(),
        marks: (0..required.marks)
            .map(|index| MarkId::new(format!("{prefix}-mark-{index}")).unwrap())
            .collect(),
    }
}

fn set(source: &ProjectDocument, ids: &[&str], name: &str) -> CapturedEditSet {
    let value = CapturedEditSet::capture(
        source,
        &node("root"),
        &ids.iter().map(|id| node(id)).collect::<Vec<_>>(),
        timing(name),
    )
    .unwrap();
    value.validate_capture(source).unwrap();
    assert_eq!(
        CapturedEditSet::from_json(&value.to_json().unwrap()).unwrap(),
        value
    );
    assert_eq!(
        serde_json::from_value::<CapturedEditSet>(serde_json::to_value(&value).unwrap()).unwrap(),
        value
    );
    value
}

fn command(value: &CapturedEditSet, parent: &str, index: usize, prefix: &str) -> Command {
    Command::SpliceEditSet {
        parent: node(parent),
        index,
        set: value.clone(),
        identities: pools(value, prefix),
        timing: timing(prefix),
    }
}

#[test]
fn union_paste_omits_the_gap_and_preserves_one_logical_mark_with_both_bindings() {
    let (adjacent, separated) = fixture();
    let combined = capture(
        &adjacent,
        SliceCaptureSelection::Children {
            first: node("left-fragment"),
            last: node("right-fragment"),
        },
        "union-oracle",
    );
    let value = set(
        &separated,
        &["left-fragment", "right-fragment"],
        "union-capture",
    );
    assert_eq!(value.duration(), frames(12));
    assert_eq!(value.components().len(), 2);
    assert_eq!(value.components()[0].range(), range(0, 5));
    assert_eq!(value.components()[1].range(), range(7, 14));
    assert_eq!(value.scope(), &node("root"));
    assert_eq!(value.assets(), separated.assets());
    assert_eq!(value.identity_requirements().unwrap().marks, 1);
    let wire = serde_json::to_value(&value).unwrap();
    let marks: BTreeMap<MarkId, Mark> = serde_json::from_value(wire["marks"].clone()).unwrap();
    assert_eq!(marks.len(), 1);
    assert_eq!(marks.values().next().unwrap().binding_count(), 2);
    for member in value.components() {
        assert!(
            serde_json::to_value(member).unwrap()["nodes"]
                .get("gap")
                .is_none()
        );
        assert_eq!(captured_marks(member).len(), 1);
    }
    let empty = destination(&separated, "union-empty");
    let result = edit(
        &empty,
        "union-paste",
        command(&value, "root", 0, "union-paste"),
    );
    let oracle = paste(
        &destination(&adjacent, "union-oracle-empty"),
        &combined,
        "union-oracle-paste",
        0,
    );
    assert_eq!(result.duration().unwrap(), oracle.duration().unwrap());
    assert_eq!(result.marks().len(), 1);
    let copied = &result.marks()[&MarkId::new("union-paste-mark-0").unwrap()];
    let control = oracle.marks().values().next().unwrap();
    assert_eq!(copied.binding_count(), control.binding_count());
    assert_eq!(copied.binding_count(), 2);
    assert_eq!(copied.label, control.label);
    assert_eq!(copied.loss_policy, control.loss_policy);
    assert_eq!(copied.boundary.bias, control.boundary.bias);
    for (binding, expected) in copied.bindings().zip(control.bindings()) {
        assert_eq!(binding.coordinate, expected.coordinate);
        assert_eq!(binding.state, expected.state);
        assert!(result.nodes().contains_key(&binding.owner));
    }
    assert_eq!(
        result.children(&node("root")).cloned().collect::<Vec<_>>(),
        vec![node("union-paste-group")]
    );
    assert_eq!(result.children(&node("union-paste-group")).count(), 2);
}

#[test]
fn components_can_have_distinct_actual_parents_and_empty_equal_time_owners() {
    let (_, separated) = fixture();
    let nested = edit(
        &separated,
        "union-nested",
        Command::Group {
            parent: node("root"),
            start: 2,
            end: 3,
            id: node("right-group"),
            label: "Right actual parent".into(),
        },
    );
    let mut document = nested;
    for (index, id) in ["empty-a", "empty-b"].into_iter().enumerate() {
        document = edit(
            &document,
            &format!("union-insert-{id}"),
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(id),
                    nodes: BTreeMap::from([(
                        node(id),
                        BeatNode::sequence("Equal-time empty", Vec::new()),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        );
    }
    let value = set(
        &document,
        &["empty-a", "empty-b", "left-fragment", "right-fragment"],
        "union-empty-members",
    );
    assert_eq!(value.components()[0].range(), range(0, 0));
    assert_eq!(value.components()[1].range(), range(0, 0));
    assert_ne!(
        value.components()[0].selection(),
        value.components()[1].selection()
    );
    assert_eq!(value.components()[2].parent(), &node("root"));
    assert_eq!(value.components()[3].parent(), &node("right-group"));
    assert_eq!(value.duration(), frames(12));
    let result = edit(
        &destination(&document, "union-nested-empty"),
        "union-nested-paste",
        command(&value, "root", 0, "union-nested-paste"),
    );
    assert_eq!(result.duration().unwrap(), frames(12));
    assert_eq!(
        result.children(&node("union-nested-paste-group")).count(),
        4
    );
    let empty_only = set(&document, &["empty-a", "empty-b"], "union-empty-only");
    assert_eq!(empty_only.duration(), FrameDuration::ZERO);
    assert_eq!(empty_only.identity_requirements().unwrap().timings, 1);
    let empty_result = edit(
        &destination(&document, "union-empty-destination"),
        "union-empty-paste",
        command(&empty_only, "root", 0, "union-empty-paste"),
    );
    assert_eq!(empty_result.duration().unwrap(), FrameDuration::ZERO);
    assert_eq!(
        empty_result
            .children(&node("union-empty-paste-group"))
            .count(),
        2
    );
}

#[test]
fn invalid_addresses_forged_capsules_and_shared_mark_loss_are_rejected() {
    let (_, document) = fixture();
    for ids in [
        vec![],
        vec![node("right-fragment"), node("left-fragment")],
        vec![node("left-fragment"), node("left-fragment")],
        vec![node("root")],
        vec![node("source")],
        vec![node("missing")],
        vec![node("left-fragment"); 65],
    ] {
        assert!(
            CapturedEditSet::capture(&document, &node("root"), &ids, timing("union-bad")).is_err()
        );
    }
    let value = set(
        &document,
        &["left-fragment", "right-fragment"],
        "union-immutable",
    );
    let mut forged = serde_json::to_value(&value).unwrap();
    forged["components"][1]["nodes"]["right-fragment"]["label"] = serde_json::json!("Forged label");
    let forged = serde_json::from_value::<CapturedEditSet>(forged).unwrap();
    assert!(forged.validate_capture(&document).is_err());
    let mut lost = serde_json::to_value(&value).unwrap();
    lost["marks"]["logical-shared-cue"]["fragments"] = serde_json::json!([]);
    assert!(serde_json::from_value::<CapturedEditSet>(lost).is_err());
    let encoded = value.to_json().unwrap();
    let duplicate = encoded.replacen(
        "\"scope\":\"root\"",
        "\"scope\":\"root\",\"scope\":\"root\"",
        1,
    );
    assert_ne!(duplicate, encoded);
    assert!(CapturedEditSet::from_json(&duplicate).is_err());
}

#[test]
fn placement_requires_exact_fresh_shared_identity_pools_without_mutating_input() {
    let (_, document) = fixture();
    let value = set(
        &document,
        &["left-fragment", "right-fragment"],
        "union-pool-capture",
    );
    let empty = destination(&document, "union-pool-empty");
    let frozen = empty.to_json().unwrap();
    for mutation in 0..3 {
        let mut identities = pools(&value, "union-pool");
        match mutation {
            0 => identities.components[0].nodes[0] = identities.group.clone(),
            1 => identities.marks.clear(),
            2 => {
                identities.components[1].nodes.pop();
            }
            _ => unreachable!(),
        }
        let request = CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: revision("union-pool"),
            command: Command::SpliceEditSet {
                parent: node("root"),
                index: 0,
                set: value.clone(),
                identities,
                timing: timing("union-pool"),
            },
        };
        assert!(apply(&empty, &request).is_err());
        assert_eq!(empty.to_json().unwrap(), frozen);
    }
    assert_eq!(stored(&document).binding_count(), 2);
}

#[test]
fn copied_sequence_marks_are_finished_against_the_complete_union_once() {
    let (_, document) = fixture();
    let document = edit(
        &document,
        "union-sequence-mark",
        Command::SetMark {
            id: MarkId::new("sequence-cue").unwrap(),
            owner: node("source"),
            label: "Final union boundary".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Sequence {
                    frame: ProjectFrame(11),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::DeleteOwned,
        },
    );
    let value = set(
        &document,
        &["left-fragment", "right-fragment"],
        "union-sequence-capture",
    );
    let result = edit(
        &destination(&document, "union-sequence-empty"),
        "union-sequence-paste",
        command(&value, "root", 0, "union-sequence-paste"),
    );
    let copied = result
        .marks()
        .values()
        .find(|mark| mark.label == "Final union boundary")
        .unwrap();
    assert_eq!(copied.state, MarkState::Bound);
    assert_eq!(
        copied.boundary.coordinate,
        Anchor::Sequence {
            frame: ProjectFrame(11)
        }
    );
    assert_eq!(copied.binding_count(), 1);
}

#[test]
fn unsupported_cross_component_coordinate_is_refused_instead_of_losing_the_binding() {
    let (_, document) = fixture();
    let document = edit(
        &document,
        "union-cross-mark",
        Command::SetMark {
            id: MarkId::new("cross-cue").unwrap(),
            owner: node("source"),
            label: "Cross-component coordinate".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("right-source-copy"),
                    position: ExactRatio::integer(8),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    );
    let frozen = document.to_json().unwrap();
    let error = CapturedEditSet::capture(
        &document,
        &node("root"),
        &[node("left-fragment"), node("right-fragment")],
        timing("union-cross-capture"),
    )
    .unwrap_err();
    assert!(error.message.contains("cross-component mark relationship"));
    assert_eq!(document.to_json().unwrap(), frozen);
    assert_eq!(
        document.marks()[&MarkId::new("cross-cue").unwrap()].state,
        MarkState::Bound
    );
}
