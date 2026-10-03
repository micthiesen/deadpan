use super::*;

fn captured(doc: &ProjectDocument, first: &str, last: &str) -> Arc<CapturedEditSlice> {
    Arc::new(
        CapturedEditSlice::capture_selection(
            doc,
            &node("root"),
            &SliceCaptureSelection::Children {
                first: node(first),
                last: node(last),
            },
            timing("copy-children"),
        )
        .unwrap(),
    )
}

fn request(doc: &ProjectDocument, first: &str, last: &str) -> CommandRequest {
    CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: revision("cut-children"),
        command: Command::DeleteChildren {
            parent: node("root"),
            first: node(first),
            last: node(last),
            timing: timing("cut-children"),
        },
    }
}

#[test]
fn forest_cut_refuses_same_time_different_identity_then_reopens_through_undo_redo() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("children.deadpan");
    let initial = document();
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    let previous_bank = save(&mut store, 'a', capture(&initial, "first"));
    let slice = captured(&initial, "empty", "last");
    let before = timeline(&store);
    // Both of these remove the same picture interval but leave the selected
    // empty endpoint behind. Neither can be paired with this forest capture.
    for command in [
        Command::DeleteRipple {
            node: node("last"),
            timing: timing("cut-children"),
        },
        Command::DeleteRange {
            parent: node("root"),
            range: slice.range(),
            identities: SplitIdentities::default(),
            timing: timing("cut-children"),
        },
        Command::DeleteChildren {
            parent: node("root"),
            first: node("last"),
            last: node("last"),
            timing: timing("cut-children"),
        },
    ] {
        let mut bad = request(&initial, "empty", "last");
        bad.command = command;
        assert!(
            store
                .cut_to_register(&bad, name('c'), slice.clone(), None)
                .is_err()
        );
        assert_eq!(timeline(&store), before);
        assert_eq!(store.registers().unwrap(), previous_bank);
    }
    let (_, bank) = store
        .cut_to_register(
            &request(&initial, "empty", "last"),
            name('c'),
            slice.clone(),
            None,
        )
        .unwrap();
    assert!(
        !store
            .snapshot()
            .unwrap()
            .nodes()
            .contains_key(&node("empty"))
    );
    assert!(
        !store
            .snapshot()
            .unwrap()
            .nodes()
            .contains_key(&node("last"))
    );
    assert_eq!(bank.entries[&name('a')], previous_bank.entries[&name('a')]);
    assert_eq!(
        bank.entries[&name('c')].as_ref(),
        &RegisterValue::Edited { slice }
    );
    store.validate().unwrap();
    drop(store);
    let mut store = ProjectStore::open(&path, crate::AccessMode::ReadWrite).unwrap();
    assert_eq!(store.registers().unwrap(), bank);
    store
        .undo(&revision("cut-children"), revision("undo-children"))
        .unwrap();
    assert_eq!(store.snapshot().unwrap().nodes(), initial.nodes());
    assert_eq!(store.registers().unwrap(), bank);
    store
        .redo(&revision("undo-children"), revision("redo-children"))
        .unwrap();
    assert!(
        !store
            .snapshot()
            .unwrap()
            .nodes()
            .contains_key(&node("empty"))
    );
    store.validate().unwrap();
}

#[test]
fn all_empty_forest_cut_keeps_exact_zero_time_payload_and_neighbor_clocks() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = serde_json::to_value(document()).unwrap();
    wire["nodes"]["root"]["kind"]["children"] =
        serde_json::json!(["first", "empty", "empty-two", "last"]);
    wire["nodes"]["empty-two"] =
        serde_json::to_value(BeatNode::sequence("Other empty", vec![])).unwrap();
    let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("empty-children.deadpan"), &initial).unwrap();
    let slice = captured(&initial, "empty", "empty-two");
    assert_eq!(slice.duration(), FrameDuration::ZERO);
    let (_, bank) = store
        .cut_to_register(
            &request(&initial, "empty", "empty-two"),
            name('c'),
            slice.clone(),
            None,
        )
        .unwrap();
    let after = store.snapshot().unwrap();
    assert_eq!(after.duration().unwrap(), initial.duration().unwrap());
    assert_eq!(after.audio_bindings(), initial.audio_bindings());
    assert_eq!(after.nodes().len(), initial.nodes().len() - 2);
    assert_eq!(
        bank.entries[&name('c')].as_ref(),
        &RegisterValue::Edited { slice }
    );
    store.validate().unwrap();
}
