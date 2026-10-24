//! Actual aggregate limits at raw slice and stored patch ingress.
use super::*;
use std::fmt::Write;

const ANIMATED_CHILDREN: usize = (MAX_FRAMING_RECORDS - 2) / 2;

fn fixed_follow() -> Framing {
    let mut framing = follow();
    if let FramingValue::Follow {
        scale_transition, ..
    } = &mut framing.value
    {
        *scale_transition = None;
    }
    framing
}

fn records<'a>(nodes: impl Iterator<Item = &'a BeatNode>) -> usize {
    nodes
        .filter_map(|node| node.framing.as_ref())
        .map(Framing::record_count)
        .sum()
}

// Serialize the repeated small node once. The fixture is still admitted through
// the real document/slice readers, without a large intermediate JSON Value tree.
fn nodes_wire(before: &ProjectDocument, fixed_children: usize, root: bool) -> String {
    let mut animated = hold(1);
    animated.framing = Some(follow());
    let animated = serde_json::to_string(&animated).unwrap();
    let mut fixed = hold(1);
    fixed.framing = Some(fixed_follow());
    let fixed = serde_json::to_string(&fixed).unwrap();
    let children: Vec<_> = (0..ANIMATED_CHILDREN)
        .map(|n| id(&format!("h{n}")))
        .chain((0..fixed_children).map(|n| id(&format!("fixed{n}"))))
        .collect();
    let mut wire = String::from("{");
    if root {
        write!(
            wire,
            "\"root\":{},",
            serde_json::to_string(&BeatNode::sequence("Root", vec![id("hold"), id("owner")]))
                .unwrap()
        )
        .unwrap();
    }
    write!(
        wire,
        "\"hold\":{},\"owner\":{}",
        serde_json::to_string(&before.nodes()[&id("hold")]).unwrap(),
        serde_json::to_string(&BeatNode::sequence("Owner", children)).unwrap()
    )
    .unwrap();
    for n in 0..ANIMATED_CHILDREN {
        write!(wire, ",\"h{n}\":{animated}").unwrap();
    }
    for n in 0..fixed_children {
        write!(wire, ",\"fixed{n}\":{fixed}").unwrap();
    }
    wire.push('}');
    wire
}

fn replace_nodes(json: &str, nodes: &str) -> String {
    let fields: BTreeMap<String, &serde_json::value::RawValue> =
        serde_json::from_str(json).unwrap();
    assert!(fields.contains_key("nodes"));
    let mut wire = String::from("{");
    for (index, (key, value)) in fields.into_iter().enumerate() {
        if index != 0 {
            wire.push(',');
        }
        write!(wire, "{}:", serde_json::to_string(&key).unwrap()).unwrap();
        wire.push_str(if key == "nodes" { nodes } else { value.get() });
    }
    wire.push('}');
    wire
}

fn append_fixed(patch: &mut DocumentPatch, n: usize) {
    let child_id = id(&format!("fixed{n}"));
    let owner = patch
        .nodes
        .get_mut(&id("owner"))
        .unwrap()
        .after
        .as_mut()
        .unwrap();
    let NodeKind::Sequence { children } = &mut owner.kind else {
        panic!("fixture owner must be a Sequence");
    };
    children.push(child_id.clone());
    let mut child = hold(1);
    child.framing = Some(fixed_follow());
    assert!(
        patch
            .nodes
            .insert(
                child_id,
                ValueChange {
                    before: None,
                    after: Some(child)
                }
            )
            .is_none()
    );
}

#[test]
fn raw_patch_enforces_exact_aggregate_cap_and_retained_result_union() {
    // One unchanged fixed Follow is deliberately absent from the forward patch.
    // 49,999 animated children plus one fixed child add 99,999 patch records.
    let before = edit(
        &targeted(),
        Command::SetFraming {
            node: id("hold"),
            framing: Some(fixed_follow()),
        },
    );
    let before_json = before.to_json().unwrap();
    let cap_json = replace_nodes(&before_json, &nodes_wire(&before, 1, true));
    assert!(cap_json.len() < MAX_DOCUMENT_JSON_BYTES);
    let cap = ProjectDocument::from_json(&cap_json).unwrap();
    assert_eq!(records(cap.nodes().values()), MAX_FRAMING_RECORDS);
    drop(cap_json);
    let tx = apply(
        &before,
        &request(
            &before,
            Command::RestoreSnapshot {
                snapshot: Box::new(cap),
            },
        ),
    )
    .unwrap();
    let exact = tx.forward.apply(&before).unwrap();
    assert_eq!(tx.inverse.apply(&exact).unwrap(), before);
    assert_eq!(records(exact.nodes().values()), MAX_FRAMING_RECORDS);

    {
        let error = CapturedEditSlice::capture(
            &exact,
            exact.root(),
            FrameRange::new(
                ProjectFrame(0),
                ProjectFrame(exact.duration().unwrap().frames()),
            )
            .unwrap(),
            AudioTimingId {
                allocation: RevisionId::new("aggregate-capture").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap_err();
        // This separate work budget prevents actual capture of this many timed
        // Holds. It must not be confused with the raw slice reader's record cap.
        assert_eq!(error.code, EditErrorCode::LimitExceeded);
        assert!(
            error
                .message
                .contains("audio binding capture complexity limit")
        );
        assert_eq!(before.to_json().unwrap(), before_json);
    }

    let mut patch = tx.forward;
    assert!(!patch.nodes.contains_key(&id("hold")));
    assert_eq!(
        records(
            patch
                .nodes
                .values()
                .filter_map(|change| change.after.as_ref())
        ),
        MAX_FRAMING_RECORDS - 1
    );
    {
        let raw = serde_json::to_string(&patch).unwrap();
        assert!(raw.len() < MAX_DOCUMENT_JSON_BYTES);
        let reopened: DocumentPatch = serde_json::from_str(&raw).unwrap();
        assert_eq!(reopened.apply(&before).unwrap(), exact);
    }
    drop(exact);
    // This raw patch's own after inventory is exactly legal. Its result union
    // with the retained fixed Follow is one over, rejected by result validation.
    append_fixed(&mut patch, 1);
    assert_eq!(
        records(
            patch
                .nodes
                .values()
                .filter_map(|change| change.after.as_ref())
        ),
        MAX_FRAMING_RECORDS
    );
    {
        let raw = serde_json::to_string(&patch).unwrap();
        assert!(raw.len() < MAX_DOCUMENT_JSON_BYTES);
        let reopened: DocumentPatch = serde_json::from_str(&raw).unwrap();
        assert_framing_refusal(reopened.apply(&before).unwrap_err());
        assert_eq!(before.to_json().unwrap(), before_json);
    }
    // One more fixed Follow exceeds the patch inventory cap itself.
    append_fixed(&mut patch, 2);
    assert_eq!(
        records(
            patch
                .nodes
                .values()
                .filter_map(|change| change.after.as_ref())
        ),
        MAX_FRAMING_RECORDS + 1
    );
    let raw = serde_json::to_string(&patch).unwrap();
    assert!(raw.len() < MAX_DOCUMENT_JSON_BYTES);
    let reopened: DocumentPatch = serde_json::from_str(&raw).unwrap();
    assert_framing_refusal(reopened.apply(&before).unwrap_err());
    assert_eq!(before.to_json().unwrap(), before_json);
}

// Empty Sequences add authored records without adding audio material or time.
// The original physical Hold keeps its audio binding and eight-frame map.
fn slice_nodes_wire(extra_fixed: bool) -> String {
    let mut animated = BeatNode::sequence("Empty", Vec::new());
    animated.framing = Some(follow());
    let animated = serde_json::to_string(&animated).unwrap();
    let mut fixed = BeatNode::sequence("Empty", Vec::new());
    fixed.framing = Some(fixed_follow());
    let fixed = serde_json::to_string(&fixed).unwrap();
    let children: Vec<_> = (0..ANIMATED_CHILDREN)
        .map(|n| id(&format!("h{n}")))
        .chain(extra_fixed.then(|| id("extra")))
        .collect();
    let mut owner = BeatNode::sequence("Empty owner", children);
    owner.framing = Some(fixed_follow());
    let mut timed = hold(8);
    timed.framing = Some(fixed_follow());
    let mut wire = format!(
        "{{\"owner\":{},\"hold\":{}",
        serde_json::to_string(&owner).unwrap(),
        serde_json::to_string(&timed).unwrap()
    );
    for n in 0..ANIMATED_CHILDREN {
        write!(wire, ",\"h{n}\":{animated}").unwrap();
    }
    if extra_fixed {
        write!(wire, ",\"extra\":{fixed}").unwrap();
    }
    wire.push('}');
    wire
}

#[test]
fn raw_slice_reader_enforces_exact_aggregate_cap_without_claiming_capture_admission() {
    let before = edit(
        &targeted(),
        Command::SetFraming {
            node: id("hold"),
            framing: Some(fixed_follow()),
        },
    );
    let before_json = before.to_json().unwrap();
    let small = CapturedEditSlice::capture(
        &before,
        before.root(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(8)).unwrap(),
        AudioTimingId {
            allocation: RevisionId::new("small-aggregate-envelope").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let envelope = small.to_json().unwrap();
    // Preserve the physical recipe and its audio binding. A second, empty part
    // adds records without changing the closed contiguous eight-frame selection.
    let mut expanded: serde_json::Value = serde_json::from_str(&envelope).unwrap();
    expanded["parts"].as_array_mut().unwrap().push(json!({
        "root": "owner",
        "mapping": FrameRange::new(ProjectFrame(0), ProjectFrame(0)).unwrap(),
        "source_start": ProjectFrame(8),
    }));
    let expanded = expanded.to_string();
    let raw = replace_nodes(&expanded, &slice_nodes_wire(false));
    assert!(raw.len() < MAX_DOCUMENT_JSON_BYTES);
    let slice = CapturedEditSlice::from_json(&raw).unwrap();
    let roundtrip = slice.to_json().unwrap();
    assert_eq!(CapturedEditSlice::from_json(&roundtrip).unwrap(), slice);
    // This is a valid standalone raw slice, not a capture of the small source.
    assert!(slice.validate_capture(&before).is_err());
    let overflow = replace_nodes(&expanded, &slice_nodes_wire(true));
    assert!(overflow.len() < MAX_DOCUMENT_JSON_BYTES);
    assert_framing_refusal(CapturedEditSlice::from_json(&overflow).unwrap_err());
    assert_eq!(slice.to_json().unwrap(), roundtrip);
    assert_eq!(small.to_json().unwrap(), envelope);
    assert_eq!(before.to_json().unwrap(), before_json);
}
