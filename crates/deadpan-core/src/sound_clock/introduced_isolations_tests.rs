use super::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn isolation(ordinal: u32) -> SoundClockIsolation {
    SoundClockIsolation {
        live_repeat: id("live-repeat"),
        historical_repeat: id("historical-repeat"),
        iteration: IterationId {
            allocation: crate::RevisionId::new("plays").unwrap(),
            ordinal,
        },
        live_root: id(&format!("owned-{ordinal}")),
    }
}

fn reference() -> SoundClockReference {
    SoundClockReference::new(
        AudioTimingId {
            allocation: crate::RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
        id("scope"),
        id("owner"),
    )
}

fn refuses(records: Vec<SoundClockIsolation>, message: &str) {
    let error = reference()
        .with_introduced_isolations(records.clone())
        .unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::InvalidTree);
    assert!(error.message.contains(message), "{error}");
    let mut wire = serde_json::to_value(reference()).unwrap();
    wire["introduced_isolations"] = serde_json::to_value(records).unwrap();
    let error = serde_json::from_value::<SoundClockReference>(wire).unwrap_err();
    assert!(error.to_string().contains(message), "{error}");
}

#[test]
fn isolation_reference_defaults_to_empty_without_changing_the_old_wire() {
    let reference = reference();
    let wire = serde_json::to_string(&reference).unwrap();
    assert!(!wire.contains("introduced_isolations"));
    assert!(reference.introduced_isolations().is_empty());
    assert_eq!(reference.isolation_work_count(), 0);
    assert_eq!(
        serde_json::from_str::<SoundClockReference>(&wire).unwrap(),
        reference
    );
}

#[test]
fn isolation_reference_roundtrip_retains_each_explicit_effective_play() {
    let old = reference();
    let records = vec![isolation(1), isolation(3)];
    let reference = old
        .clone()
        .with_introduced_isolations(records.clone())
        .unwrap();
    assert_eq!(reference.introduced_isolations(), records);
    assert_eq!(reference.isolation_work_count(), 2);
    assert_eq!(reference.relations().2, records);
    assert_eq!(reference.timing(), old.timing());
    assert_eq!(reference.scope(), old.scope());
    assert_eq!(reference.owner(), old.owner());
    assert_eq!(
        serde_json::from_str::<SoundClockReference>(&serde_json::to_string(&reference).unwrap())
            .unwrap(),
        reference
    );
}

#[test]
fn isolation_reference_rejects_reused_live_play_or_owned_root() {
    let first = isolation(1);
    let mut same_play = first.clone();
    same_play.live_root = id("another-owned-root");
    same_play.historical_repeat = id("another-historical-repeat");
    refuses(
        vec![first.clone(), same_play],
        "duplicate sound clock isolation play",
    );
    let mut same_root = isolation(2);
    same_root.live_root = first.live_root.clone();
    refuses(
        vec![first, same_root],
        "duplicate sound clock isolation root",
    );
}

#[test]
fn isolation_reference_rejects_a_live_repeat_self_edge() {
    let mut record = isolation(1);
    record.live_root = record.live_repeat.clone();
    refuses(vec![record], "sound clock isolation has a self edge");
}

#[test]
fn isolation_wire_requires_all_fields_and_rejects_unknown_fields() {
    let complete = serde_json::to_value(isolation(1)).unwrap();
    for name in ["live_repeat", "historical_repeat", "iteration", "live_root"] {
        let mut incomplete = complete.clone();
        incomplete.as_object_mut().unwrap().remove(name);
        assert!(serde_json::from_value::<SoundClockIsolation>(incomplete).is_err());
    }
    let mut extended = complete;
    extended["trusted"] = serde_json::json!(true);
    assert!(serde_json::from_value::<SoundClockIsolation>(extended).is_err());
}

#[test]
fn isolation_reference_bounds_owned_and_streamed_relation_counts() {
    let records = (0..=crate::MAX_DOCUMENT_NODES)
        .map(|ordinal| isolation(u32::try_from(ordinal).unwrap()))
        .collect::<Vec<_>>();
    let error = reference()
        .with_introduced_isolations(records.clone())
        .unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::LimitExceeded);
    assert!(
        error
            .message
            .contains("sound clock isolation relation limit")
    );

    // Stream the array directly rather than constructing an unbounded JSON
    // Value tree. Every preceding relation is distinct, so the final relation
    // must hit the count bound rather than an earlier duplicate rejection.
    let mut wire = serde_json::to_vec(&reference()).unwrap();
    assert_eq!(wire.pop(), Some(b'}'));
    wire.extend(b",\"introduced_isolations\":");
    serde_json::to_writer(&mut wire, &records).unwrap();
    wire.push(b'}');
    let error = serde_json::from_slice::<SoundClockReference>(&wire).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("sound clock isolation relation limit"),
        "{error}"
    );
}
