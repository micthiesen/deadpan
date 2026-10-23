use super::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn split() -> SoundClockSplit {
    SoundClockSplit::new(
        id("historical"),
        vec![
            SoundClockSplitPiece {
                partition: id("left"),
                full_owner: id("left-full"),
            },
            SoundClockSplitPiece {
                partition: id("right"),
                full_owner: id("right-full"),
            },
        ],
    )
    .unwrap()
}

fn reference() -> SoundClockReference {
    SoundClockReference::new(
        AudioTimingId {
            allocation: crate::RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
        id("scope"),
        id("historical"),
    )
}

#[test]
fn split_reference_roundtrip_retains_exact_ordered_witness() {
    let old = reference();
    assert!(
        !serde_json::to_string(&old)
            .unwrap()
            .contains("introduced_splits")
    );
    let splits = vec![split()];
    let current = old.clone().with_introduced_splits(splits.clone()).unwrap();
    let restored: SoundClockReference =
        serde_json::from_str(&serde_json::to_string(&current).unwrap()).unwrap();
    assert_eq!(restored, current);
    assert_eq!(restored.introduced_splits(), splits);
    assert_eq!(restored.split_work_count(), 3);
    assert_eq!(restored.scope(), old.scope());
    assert_eq!(restored.owner(), old.owner());
    assert!(old.introduced_splits().is_empty());
}

#[test]
fn split_admission_rejects_partial_or_duplicate_piece_identities() {
    let valid = split();
    assert!(SoundClockSplit::new(id("historical"), valid.pieces[..1].to_vec()).is_err());
    for partition_duplicate in [false, true] {
        let mut malformed = valid.clone();
        if partition_duplicate {
            malformed.pieces[1].partition = malformed.pieces[0].partition.clone();
        } else {
            malformed.pieces[1].full_owner = malformed.pieces[0].full_owner.clone();
        }
        assert!(
            reference()
                .with_introduced_splits(vec![malformed.clone()])
                .is_err()
        );
        assert!(
            serde_json::from_value::<SoundClockSplit>(serde_json::to_value(malformed).unwrap())
                .is_err()
        );
    }
}

#[test]
fn split_reference_rejects_duplicate_historical_records() {
    let splits = vec![split(), split()];
    assert!(reference().with_introduced_splits(splits.clone()).is_err());
    let mut wire = serde_json::to_value(reference()).unwrap();
    wire["introduced_splits"] = serde_json::to_value(splits).unwrap();
    let error = serde_json::from_value::<SoundClockReference>(wire).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("duplicate historical sound clock Split")
    );
}

#[test]
fn split_reference_rechecks_public_piece_fields_before_admission() {
    let mut malformed = split();
    malformed.pieces = vec![malformed.pieces[0].clone(); crate::MAX_DOCUMENT_NODES + 1];
    assert!(reference().with_introduced_splits(vec![malformed]).is_err());
}
