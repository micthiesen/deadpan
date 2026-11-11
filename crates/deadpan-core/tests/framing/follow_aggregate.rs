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

// Serialize the repeated small node once. The fixture is still admitted through
// the real document/slice readers, without a large intermediate JSON Value tree.

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
