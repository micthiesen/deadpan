use super::*;
use deadpan_core::{NodeId, ProjectDocument};

fn store(scratch: &tempfile::TempDir) -> ProjectStore {
    let document = ProjectDocument::new_automatic(
        ProjectId::new("moments-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    ProjectStore::create(&scratch.path().join("moments.deadpan"), &document).unwrap()
}

#[test]
fn moment_names_tags_and_ids_are_validated_and_normalized() {
    for bad in [
        "",
        " ",
        " name",
        "name ",
        "a\nb",
        "a\tb",
        "a\u{2028}b",
        "a\0b",
    ] {
        assert!(MomentName::new(bad).is_err(), "{bad:?}");
    }
    assert!(MomentName::new("é".repeat(64)).is_ok());
    assert!(MomentName::new("é".repeat(65)).is_err());
    assert!(MomentId::new("bad id").is_err());
    for bad in [
        "",
        "Reaction",
        "two words",
        "a,b",
        "a\u{7}",
        &"x".repeat(33),
    ] {
        assert!(MomentTag::new(bad).is_err(), "{bad:?}");
    }
    assert_eq!(
        MomentTag::normalize(" Réaction ").unwrap().as_str(),
        "réaction"
    );
    let tags = MomentTags::from_input(["Breath", "reaction", "breath"]).unwrap();
    assert_eq!(
        tags.as_slice()
            .iter()
            .map(MomentTag::as_str)
            .collect::<Vec<_>>(),
        ["breath", "reaction"]
    );
    assert!(
        MomentTags::from_input(
            (0..17)
                .map(|n| format!("t{n}"))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
        )
        .is_err()
    );
    // The wire form is strict: unsorted, duplicated or uppercase tags refuse.
    for wire in [r#"["b","a"]"#, r#"["a","a"]"#, r#"["A"]"#] {
        assert!(serde_json::from_str::<MomentTags>(wire).is_err(), "{wire}");
    }
    assert!(serde_json::from_str::<MomentTags>(r#"["a","b"]"#).is_ok());
}

#[test]
fn generic_projects_list_an_empty_catalog_and_refuse_new_moments() {
    let scratch = tempfile::tempdir().unwrap();
    let mut store = store(&scratch);
    let catalog = store.moment_catalog().unwrap();
    assert_eq!(catalog.version, 0);
    assert!(catalog.entries.is_empty());
    let request = Request {
        project_id: catalog.project_id.clone(),
        expected_version: 0,
        action: Action::Create {
            id: MomentId::new("m-1").unwrap(),
            name: MomentName::new("Laugh").unwrap(),
            tags: MomentTags::default(),
            start: 0,
            end: 4,
        },
    };
    let error = store.apply_moment(&request).unwrap_err();
    assert_eq!(error.code(), "MomentsInvalid");
    assert!(error.to_string().contains("Original is ready"), "{error}");
    assert_eq!(store.moment_catalog().unwrap(), catalog);
}

#[test]
fn moment_catalog_tampering_is_detected() {
    let scratch = tempfile::tempdir().unwrap();
    let store = store(&scratch);
    store
        .connection
        .execute("UPDATE moment_state SET version=3", [])
        .unwrap();
    let error = store.moment_catalog().unwrap_err().to_string();
    assert!(error.contains("digest"), "{error}");
}

/// Generic projects cannot create moments, so seed through the same writer
/// path a Ready project's Create commits.
fn seed(store: &ProjectStore, id: &str, name: &str) {
    let mut catalog = store.moment_catalog().unwrap();
    store
        .connection
        .execute("INSERT INTO moment_ids(id) VALUES(?1)", [id])
        .unwrap();
    catalog.entries.push(Moment {
        id: MomentId::new(id).unwrap(),
        name: MomentName::new(name).unwrap(),
        tags: MomentTags::from_input(["reaction"]).unwrap(),
        start: 2,
        end: 9,
    });
    catalog.entries.sort_by(|a, b| a.id.cmp(&b.id));
    catalog.version += 1;
    write_catalog(&store.connection, &catalog).unwrap();
}

fn request(store: &ProjectStore, action: Action) -> Request {
    let catalog = store.moment_catalog().unwrap();
    Request {
        project_id: catalog.project_id,
        expected_version: catalog.version,
        action,
    }
}

fn id(value: &str) -> MomentId {
    MomentId::new(value).unwrap()
}

#[test]
fn moments_rename_retag_delete_are_versioned_and_never_reuse_ids() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("moments.deadpan");
    let mut store = store(&scratch);
    seed(&store, "a", "Laugh");
    seed(&store, "b", "Sigh");
    let head = store.snapshot().unwrap().revision_id().clone();

    let rename = request(
        &store,
        Action::Rename {
            id: id("a"),
            name: MomentName::new("Big laugh").unwrap(),
        },
    );
    let preview = store.preview_moment(&rename).unwrap();
    assert!(preview.changed);
    assert_eq!(
        store.moment_catalog().unwrap().version,
        2,
        "preview writes nothing"
    );
    assert_eq!(store.apply_moment(&rename).unwrap(), preview);
    // The same request is now stale.
    assert!(store.apply_moment(&rename).is_err());

    let duplicate = request(
        &store,
        Action::Rename {
            id: id("b"),
            name: MomentName::new("Big laugh").unwrap(),
        },
    );
    let before = store.moment_catalog().unwrap();
    assert!(store.apply_moment(&duplicate).is_err());
    assert_eq!(store.moment_catalog().unwrap(), before);

    let retag = request(
        &store,
        Action::Retag {
            id: id("b"),
            tags: MomentTags::from_input(["breath", "detail"]).unwrap(),
        },
    );
    assert!(store.apply_moment(&retag).unwrap().changed);
    let same = request(
        &store,
        Action::Retag {
            id: id("b"),
            tags: MomentTags::from_input(["detail", "breath"]).unwrap(),
        },
    );
    let unchanged = store.apply_moment(&same).unwrap();
    assert!(!unchanged.changed);
    assert_eq!(unchanged.catalog.version, 4);

    store
        .apply_moment(&request(&store, Action::Delete { id: id("a") }))
        .unwrap();
    assert!(
        store
            .apply_moment(&request(&store, Action::Delete { id: id("a") }))
            .is_err()
    );
    let reuse = request(
        &store,
        Action::Create {
            id: id("a"),
            name: MomentName::new("Again").unwrap(),
            tags: MomentTags::default(),
            start: 0,
            end: 1,
        },
    );
    let error = store.apply_moment(&reuse).unwrap_err().to_string();
    assert!(error.contains("already been used"), "{error}");
    // Catalog writes never touch authored history.
    assert_eq!(store.snapshot().unwrap().revision_id(), &head);
    let catalog = store.moment_catalog().unwrap();
    drop(store);
    let reopened = ProjectStore::open(&path, crate::AccessMode::ReadOnly).unwrap();
    assert_eq!(reopened.moment_catalog().unwrap(), catalog);
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].name.as_str(), "Sigh");
    assert_eq!(
        catalog.entries[0].tags,
        MomentTags::from_input(["breath", "detail"]).unwrap()
    );
}

#[test]
fn moments_refuse_other_projects_and_read_only_writes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("moments.deadpan");
    let store = store(&scratch);
    seed(&store, "a", "Laugh");
    let mut foreign = request(&store, Action::Delete { id: id("a") });
    foreign.project_id = ProjectId::new("another").unwrap();
    assert!(store.preview_moment(&foreign).is_err());
    let delete = request(&store, Action::Delete { id: id("a") });
    drop(store);
    let mut readonly = ProjectStore::open(&path, crate::AccessMode::ReadOnly).unwrap();
    assert!(readonly.preview_moment(&delete).unwrap().changed);
    assert!(readonly.apply_moment(&delete).is_err());
    assert_eq!(readonly.moment_catalog().unwrap().entries.len(), 1);
}

#[test]
fn moments_backup_replacement_carries_reservations_and_a_newer_version() {
    let scratch = tempfile::tempdir().unwrap();
    let document = ProjectDocument::new_automatic(
        ProjectId::new("moments-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut live = store(&scratch);
    let mut copy = ProjectStore::create(&scratch.path().join("copy.deadpan"), &document).unwrap();
    seed(&live, "old", "Old");
    seed(&copy, "old", "Old");
    seed(&live, "later", "Later");
    live.apply_moment(&request(&live, Action::Delete { id: id("later") }))
        .unwrap();
    let old_version = live.moment_catalog().unwrap().version;
    crate::retired::carry_forward(&live.connection, &mut copy.connection).unwrap();
    let restored = copy.moment_catalog().unwrap();
    assert_eq!(restored.version, old_version + 1);
    assert_eq!(restored.entries.len(), 1);
    let reserved: bool = copy
        .connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM moment_ids WHERE id='later')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(reserved);
    copy.validate_full().unwrap();
}
