use super::*;

use std::collections::BTreeMap;

use deadpan_core::{
    AudioTimingId, BeatNode, CommandRequest, EditErrorCode, SourceTrimEdge, SourceTrimMode, Subtree,
};
use deadpan_media::source_import_timing::derive_source_moment;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn trim_request(
    current: &ProjectDocument,
    next: &str,
    target: &str,
    edge: SourceTrimEdge,
    delta_frames: i64,
    wrapper: Option<&str>,
) -> CommandRequest {
    CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command: Command::TrimSource {
            parent: node("root"),
            node: node(target),
            edge,
            delta_frames,
            mode: SourceTrimMode::Ripple,
            wrapper: wrapper.map(node),
            timing: AudioTimingId {
                allocation: revision(next),
                ordinal: 0,
            },
        },
    }
}

fn ready_trim(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let (path, mut store) = project(parent)?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "registered", "camera", None)?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    let current = store.snapshot()?;
    let receipt = store.registered_source(current.revision_id(), &id("camera"))?;
    let timing = derive_source_moment(
        receipt.snapshot().video().unwrap().index(),
        receipt.snapshot().audio(),
        10..13,
        current.presentation_basis().frame_rate,
    )?;
    let source = timing.source_node(id("camera"));
    store.commit(&CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision("ready"),
        command: Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("clip"),
                nodes: BTreeMap::from([(
                    node("clip"),
                    BeatNode {
                        label: "Selected Original".into(),
                        framing: None,
                        audio_treatments: Default::default(),
                        audio_editorial_edges: Default::default(),
                        audio_edges: Default::default(),
                        kind: NodeKind::Source { source },
                        cutaways: Vec::new(),
                        captions: Vec::new(),
                    },
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    })?;
    Ok((path, store))
}

#[test]
fn trim_preview_checks_noop_metadata_and_commits_one_receipt_bound_edit() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready_trim(scratch.path())?;
    let before = store.snapshot()?;
    let original_counts = counts(&path)?;

    let noop = trim_request(&before, "noop", "clip", SourceTrimEdge::In, 0, None);
    let noop_preview = store.preview_source_trim(&noop)?;
    assert_eq!(noop_preview.resolution.applied_delta_frames, 0);
    assert!(!noop_preview.resolution.needs_wrapper);
    assert_eq!(
        noop_preview.resolution.before,
        noop_preview.resolution.after
    );
    assert!(noop_preview.edit.is_none());
    assert!(matches!(
        store.preview(&noop),
        Err(StoreError::Edit(error)) if error.code == EditErrorCode::InvalidCommand
    ));
    assert!(matches!(
        store.commit(&noop),
        Err(StoreError::Edit(error)) if error.code == EditErrorCode::InvalidCommand
    ));
    assert_eq!(counts(&path)?, original_counts);

    let mut missing_noop_wrapper = noop.clone();
    let Command::TrimSource { wrapper, .. } = &mut missing_noop_wrapper.command else {
        unreachable!()
    };
    *wrapper = Some(node("unused-wrapper"));
    assert!(store.preview_source_trim(&missing_noop_wrapper).is_err());
    let mut wrong_noop_timing = noop.clone();
    let Command::TrimSource { timing, .. } = &mut wrong_noop_timing.command else {
        unreachable!()
    };
    timing.allocation = revision("other-revision");
    assert!(store.preview_source_trim(&wrong_noop_timing).is_err());
    assert_eq!(counts(&path)?, original_counts);

    let mut missing_wrapper = trim_request(&before, "trimmed", "clip", SourceTrimEdge::In, 1, None);
    assert!(store.preview_source_trim(&missing_wrapper).is_err());
    missing_wrapper.new_revision = revision("trimmed-collision");
    let Command::TrimSource {
        wrapper, timing, ..
    } = &mut missing_wrapper.command
    else {
        unreachable!()
    };
    *wrapper = Some(node("root"));
    timing.allocation = revision("trimmed-collision");
    assert!(store.preview_source_trim(&missing_wrapper).is_err());
    let mut wrong_timing = trim_request(
        &before,
        "trimmed-bad-timing",
        "clip",
        SourceTrimEdge::In,
        1,
        Some("trim-wrapper"),
    );
    let Command::TrimSource { timing, .. } = &mut wrong_timing.command else {
        unreachable!()
    };
    timing.allocation = revision("not-the-new-revision");
    assert!(store.preview_source_trim(&wrong_timing).is_err());
    assert_eq!(counts(&path)?, original_counts);

    let request = trim_request(
        &before,
        "trimmed",
        "clip",
        SourceTrimEdge::In,
        1,
        Some("trim-wrapper"),
    );
    let preview = store.preview_source_trim(&request)?;
    assert_eq!(preview.resolution.applied_delta_frames, 1);
    assert!(preview.resolution.needs_wrapper);
    assert_eq!(preview.resolution.duration_delta_frames, -1);
    let transaction = preview.edit.expect("nonzero trim has an edit");
    assert_eq!(store.preview(&request)?, transaction);
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.edit, transaction);
    let after = store.snapshot()?;
    assert_eq!(after, transaction.forward.apply(&before)?);
    assert_eq!(after.duration()?.frames(), before.duration()?.frames() - 1);
    let NodeKind::Sequence { children } = &after.nodes()[&node("root")].kind else {
        unreachable!()
    };
    assert_eq!(children, &[node("trim-wrapper")]);
    let Some(NodeKind::Source { source: physical }) = after
        .nodes()
        .get(&preview.resolution.physical_source)
        .map(|entry| &entry.kind)
    else {
        unreachable!("the retained physical Source stays addressable")
    };
    assert_eq!(physical, &preview.resolution.after);
    let wrapper = &after.nodes()[&node("trim-wrapper")];
    assert_eq!(
        wrapper.audio_editorial_edges,
        deadpan_core::AudioEditorialEdges {
            start: true,
            end: false
        }
    );
    assert!(
        after.nodes()[&node("clip")]
            .audio_editorial_edges
            .is_empty()
    );
    let NodeKind::Retime {
        child,
        duration,
        mapping,
        purpose: deadpan_core::RetimePurpose::Partition,
        ..
    } = &wrapper.kind
    else {
        panic!("direct Source crop uses a neutral Partition")
    };
    assert_eq!(child, &preview.resolution.physical_source);
    assert_eq!(*mapping, preview.resolution.allocation_after);
    assert_eq!(*duration, preview.resolution.allocation_after.duration());
    assert_eq!(
        counts(&path)?,
        (
            original_counts.0 + 1,
            original_counts.1 + 1,
            original_counts.2
        )
    );

    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("undo-trim"))?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(undone.assets(), before.assets());
    assert_ne!(undone.revision_id(), before.revision_id());
    store.validate()?;

    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(undone.revision_id(), revision("redo-trim"))?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), after.nodes());
    assert_eq!(redone.assets(), after.assets());
    assert_ne!(redone.revision_id(), after.revision_id());
    assert_ne!(redone.revision_id(), undone.revision_id());
    let receipt = store.registered_source(redone.revision_id(), &id("camera"))?;
    store.validate()?;

    let no_receipt = trim_request(
        &redone,
        "missing-receipt",
        "trim-wrapper",
        SourceTrimEdge::In,
        0,
        None,
    );
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute("DELETE FROM source_qualifications", [])?;
    let rows = counts(&path)?;
    assert!(store.preview_source_trim(&no_receipt).is_err());
    assert_eq!(counts(&path)?, rows);
    assert_eq!(
        receipt.id(),
        redone.assets()[&id("camera")]
            .source_qualification
            .as_ref()
            .unwrap()
    );
    Ok(())
}

#[test]
fn trimmed_edge_hard_policy_survives_reopen_and_two_durable_history_steps() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready_trim(scratch.path())?;
    let initial = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision("with-neighbor"),
        command: Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("previous"),
                nodes: BTreeMap::from([(node("previous"), initial.nodes()[&node("clip")].clone())]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    })?;
    let before = store.snapshot()?;
    store.commit(&trim_request(
        &before,
        "trimmed",
        "clip",
        SourceTrimEdge::In,
        1,
        Some("trim-wrapper"),
    ))?;
    let trimmed = store.snapshot()?;
    assert_eq!(
        trimmed.nodes()[&node("previous")].audio_editorial_edges,
        deadpan_core::AudioEditorialEdges {
            start: false,
            end: true
        }
    );
    assert!(
        before.nodes()[&node("previous")]
            .audio_editorial_edges
            .is_empty()
    );
    store.commit(&CommandRequest {
        project_id: trimmed.project_id().clone(),
        expected_revision: trimmed.revision_id().clone(),
        new_revision: revision("hard-edge"),
        command: Command::SetAudioEdge {
            node: node("trim-wrapper"),
            edge: deadpan_core::AudioBoundaryKind::NodeStart,
            policy: deadpan_core::AudioEdgePolicy::Hard,
        },
    })?;
    let hardened = store.snapshot()?;
    let owner = &hardened.nodes()[&node("trim-wrapper")];
    assert!(owner.audio_editorial_edges.start);
    assert!(!owner.audio_editorial_edges.end);
    assert_eq!(
        owner.audio_edges.node_start,
        deadpan_core::AudioEdgePolicy::Hard
    );
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, hardened);
    store.undo(hardened.revision_id(), revision("undo-hard"))?;
    let automatic = store.snapshot()?;
    assert_eq!(automatic.nodes(), trimmed.nodes());
    store.undo(automatic.revision_id(), revision("undo-trim"))?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(undone.revision_id(), revision("redo-trim"))?;
    let restored_trim = store.snapshot()?;
    assert_eq!(restored_trim.nodes(), trimmed.nodes());
    store.redo(restored_trim.revision_id(), revision("redo-hard"))?;
    let restored_hard = store.snapshot()?;
    assert_eq!(restored_hard.nodes(), hardened.nodes());
    assert_ne!(restored_hard.revision_id(), hardened.revision_id());
    store.validate()?;
    Ok(())
}
