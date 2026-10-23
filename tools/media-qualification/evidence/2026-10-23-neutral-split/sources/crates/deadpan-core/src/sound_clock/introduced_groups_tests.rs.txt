use super::*;

fn reference() -> SoundClockReference {
    SoundClockReference::new(
        AudioTimingId {
            allocation: crate::RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
        NodeId::new("top").unwrap(),
        NodeId::new("owner").unwrap(),
    )
}

#[test]
fn introduced_group_reference_roundtrip_retains_explicit_relation_and_old_identity() {
    let old = reference();
    assert!(
        !serde_json::to_string(&old)
            .unwrap()
            .contains("introduced_groups")
    );
    let groups = BTreeSet::from([NodeId::new("fresh-group").unwrap()]);
    let current = old.clone().with_introduced_groups(groups.clone()).unwrap();
    let restored: SoundClockReference =
        serde_json::from_str(&serde_json::to_string(&current).unwrap()).unwrap();
    assert_eq!(restored, current);
    assert_eq!(restored.timing(), old.timing());
    assert_eq!(restored.scope(), old.scope());
    assert_eq!(restored.owner(), old.owner());
    assert_eq!(restored.repeats(), old.repeats());
    assert_eq!(restored.introduced_groups(), &groups);
    assert!(old.introduced_groups().is_empty());
}

#[test]
fn introduced_group_wire_rejects_duplicate_ids_instead_of_silently_deduplicating() {
    let mut wire = serde_json::to_value(reference()).unwrap();
    wire["introduced_groups"] = serde_json::json!(["fresh", "fresh"]);
    let error = serde_json::from_value::<SoundClockReference>(wire).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("duplicate introduced sound clock group")
    );
}
