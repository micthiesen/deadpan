use super::roll::{ready, same_content, stored};
use super::*;
use deadpan_core::{
    AudioTimingId, ExactRatio, FrameRange, HoldAudio, HoldVideo, PitchPolicy, ProjectFrame,
    RetimePurpose, SourceAudioMapping, SourceTrimCapture, SourceTrimIntent, SourceTrimPolicy,
    SourceTrimResources, SourceVideoMapping, SplitIdentities,
};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn combined(
    current: &ProjectDocument,
    next: &str,
    intent: SourceTrimIntent,
) -> Result<CommandRequest> {
    let resolution =
        current.source_trim_edit(&node("root"), &node("left"), Some(&node("right")), intent)?;
    Ok(CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command: Command::ApplySourceTrim {
            parent: node("root"),
            node: node("left"),
            right: Some(node("right")),
            intent,
            resources: SourceTrimResources {
                target_wrapper: resolution.required_target_wrapper.then(|| node("a-crop")),
                right_wrapper: resolution.required_right_wrapper.then(|| node("b-crop")),
                split: SplitIdentities {
                    nodes: (0..resolution.required_split_nodes)
                        .map(|n| node(&format!("trim-split-{n}")))
                        .collect(),
                },
                fillers: (0..resolution.required_filler_nodes)
                    .map(|n| node(&format!("trim-filler-{n}")))
                    .collect(),
                timing: (resolution.capture != SourceTrimCapture::None).then(|| AudioTimingId {
                    allocation: revision(next),
                    ordinal: 0,
                }),
            },
        },
    })
}

fn mixed(policy: SourceTrimPolicy) -> SourceTrimIntent {
    SourceTrimIntent {
        in_frames: 1,
        out_frames: 1,
        slip_frames: 1,
        roll_frames: 1,
        policy,
    }
}

fn assert_committed_mixed(
    before: &ProjectDocument,
    after: &ProjectDocument,
    policy: SourceTrimPolicy,
) -> Result {
    let NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        panic!("ordinary root")
    };
    let expected = if policy == SourceTrimPolicy::Ripple {
        vec!["a-crop", "b-crop"]
    } else {
        vec!["trim-filler-0", "a-crop", "b-crop"]
    };
    assert_eq!(
        children.iter().map(NodeId::as_str).collect::<Vec<_>>(),
        expected
    );
    for (wrapper, owner, start, end, physical_duration, slip, out) in [
        ("a-crop", "left", 1, 5, 5, 1, 2),
        (
            "b-crop",
            "right",
            if policy == SourceTrimPolicy::Ripple {
                1
            } else {
                2
            },
            3,
            3,
            0,
            0,
        ),
    ] {
        assert_eq!(
            after.nodes()[&node(wrapper)].kind,
            NodeKind::Retime {
                child: node(owner),
                duration: deadpan_core::FrameDuration::new(end - start)?,
                mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end))?,
                pitch: PitchPolicy::FollowSpeed,
                purpose: RetimePurpose::Partition,
            }
        );
        let NodeKind::Source { source: old } = &before.nodes()[&node(owner)].kind else {
            panic!()
        };
        let NodeKind::Source { source: new } = &after.nodes()[&node(owner)].kind else {
            panic!()
        };
        assert_eq!(new.duration.frames(), physical_duration);
        assert_eq!(new.video, old.video);
        assert_eq!(new.audio, old.audio);
        assert_eq!(new.audio_offset, old.audio_offset);
        let old_window = old.edit_window.unwrap();
        let new_window = new.edit_window.unwrap();
        // Positive integral In/Roll crops keep the earlier selected context
        // hidden behind their Partitions. The committed allocations above,
        // rather than a narrowed physical window, remove its visible output.
        assert_eq!(new_window.start(), old_window.start());
        assert_eq!(
            new_window.end(),
            old_window.end().checked_add(ExactRatio::integer(out))?
        );
        let SourceVideoMapping::SelectedPlacement {
            start: old_start,
            frames: old_frames,
            ..
        } = old.video_mapping
        else {
            panic!()
        };
        let SourceVideoMapping::SelectedPlacement { start, frames, .. } = new.video_mapping else {
            panic!()
        };
        assert_eq!(start, old_start.checked_sub(ExactRatio::integer(slip))?);
        assert_eq!(frames, old_frames);
        let SourceAudioMapping::SelectedPlacement {
            start: old_start,
            frames: old_frames,
            ..
        } = old.audio_mapping
        else {
            panic!()
        };
        let SourceAudioMapping::SelectedPlacement { start, frames, .. } = new.audio_mapping else {
            panic!()
        };
        assert_eq!(start, old_start.checked_sub(ExactRatio::integer(slip))?);
        assert_eq!(frames, old_frames);
    }
    if policy == SourceTrimPolicy::Overwrite {
        let NodeKind::Hold { recipe } = &after.nodes()[&node("trim-filler-0")].kind else {
            panic!()
        };
        assert_eq!(recipe.duration.frames(), 1);
        assert_eq!(recipe.video, HoldVideo::Background);
        assert_eq!(recipe.audio, HoldAudio::Silence);
    }
    Ok(())
}

#[test]
fn combined_trim_qualifies_two_originals_and_replays_one_durable_transaction() -> Result {
    for policy in [SourceTrimPolicy::Ripple, SourceTrimPolicy::Overwrite] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = ready(scratch.path(), 10..13)?;
        let before = store.snapshot()?;
        let database = Connection::open(path.join("project.sqlite"))?;
        let rows = stored(&database)?;
        let initial = counts(&path)?;
        let request = combined(&before, "combined", mixed(policy))?;
        let request: CommandRequest = serde_json::from_str(&serde_json::to_string(&request)?)?;
        let preview = store.preview_source_trim_edit(&request)?;
        assert_eq!(
            preview.resolution.geometry.target.output_after.start().0,
            if policy == SourceTrimPolicy::Ripple {
                0
            } else {
                1
            }
        );
        assert_eq!(
            preview.resolution.geometry.target.output_after.end().0,
            if policy == SourceTrimPolicy::Ripple {
                4
            } else {
                5
            }
        );
        assert_eq!(
            preview
                .resolution
                .right_after
                .as_ref()
                .unwrap()
                .output
                .start()
                .0,
            if policy == SourceTrimPolicy::Ripple {
                4
            } else {
                5
            }
        );
        assert_eq!(
            preview
                .resolution
                .right_after
                .as_ref()
                .unwrap()
                .output
                .end()
                .0,
            6
        );
        assert_eq!(
            preview.resolution.fillers.len(),
            usize::from(policy == SourceTrimPolicy::Overwrite)
        );
        assert_eq!(stored(&database)?, rows);
        let transaction = preview.edit.unwrap();
        assert_eq!(store.preview(&request)?, transaction);
        assert_eq!(store.commit(&request)?.edit, transaction);
        let after = store.snapshot()?;
        assert_eq!(after.duration()?.frames(), 6);
        assert_eq!(after.assets(), before.assets());
        assert_committed_mixed(&before, &after, policy)?;
        assert_eq!(transaction.forward.apply(&before)?, after);
        assert_eq!(transaction.inverse.apply(&after)?, before);
        assert_eq!(counts(&path)?, (initial.0 + 1, initial.1 + 1, initial.2));
        store.validate()?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, after);
        store.undo(after.revision_id(), revision("undone"))?;
        let undone = store.snapshot()?;
        same_content(&undone, &before)?;
        assert_ne!(undone.revision_id(), before.revision_id());
        let rows = stored(&database)?;
        assert!(store.preview_source_trim_edit(&request).is_err());
        assert!(store.commit(&request).is_err());
        assert_eq!(stored(&database)?, rows);
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.redo(undone.revision_id(), revision("redone"))?;
        let redone = store.snapshot()?;
        same_content(&redone, &after)?;
        assert_ne!(redone.revision_id(), after.revision_id());
        assert_ne!(redone.revision_id(), undone.revision_id());
        assert_eq!(store.snapshot_at(before.revision_id())?, before);
        assert_eq!(store.snapshot_at(after.revision_id())?, after);
        assert_eq!(counts(&path)?, (initial.0 + 3, initial.1 + 1, initial.2));
        store.validate()?;
    }
    Ok(())
}

#[test]
fn overwrite_admits_and_removes_the_fully_consumed_right_source() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let request = combined(
        &before,
        "retired",
        SourceTrimIntent {
            out_frames: 2,
            roll_frames: 1,
            policy: SourceTrimPolicy::Overwrite,
            ..Default::default()
        },
    )?;
    let preview = store.preview_source_trim_edit(&request)?;
    assert!(preview.resolution.geometry.right.is_some());
    assert!(preview.resolution.right_after.is_none());
    assert!(preview.resolution.fillers.is_empty());
    let transaction = preview.edit.unwrap();
    assert_eq!(store.commit(&request)?.edit, transaction);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 6);
    assert!(!after.nodes().contains_key(&node("right")));
    assert_eq!(after.assets(), before.assets());
    assert_eq!(transaction.inverse.apply(&after)?, before);
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

#[test]
fn zero_preview_requires_empty_resources_and_never_reserves_a_revision() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = stored(&database)?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let zero = combined(&before, "unused", SourceTrimIntent::default())?;
    assert!(reader.preview_source_trim_edit(&zero)?.edit.is_none());
    assert!(store.preview(&zero).is_err());
    assert!(store.commit(&zero).is_err());
    let mut bad = zero.clone();
    let Command::ApplySourceTrim { resources, .. } = &mut bad.command else {
        unreachable!()
    };
    resources.timing = Some(AudioTimingId {
        allocation: revision("unused"),
        ordinal: 0,
    });
    assert!(reader.preview_source_trim_edit(&bad).is_err());
    let mut reused = zero;
    reused.new_revision = revision("right-registered");
    assert_eq!(
        reader.preview_source_trim_edit(&reused).unwrap_err().code(),
        "RevisionReused"
    );
    assert_eq!(stored(&database)?, rows);
    let nonzero = combined(&before, "unused", mixed(SourceTrimPolicy::Ripple))?;
    let preview = reader.preview_source_trim_edit(&nonzero)?.edit.unwrap();
    assert_eq!(store.commit(&nonzero)?.edit, preview);
    store.validate()?;
    Ok(())
}

#[test]
fn combined_refusals_preserve_nonempty_redo_and_every_authored_admission_row() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let initial = store.snapshot()?;
    store.commit(&combined(
        &initial,
        "saved",
        mixed(SourceTrimPolicy::Ripple),
    )?)?;
    store.undo(&revision("saved"), revision("entry"))?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = stored(&database)?;
    let redo: i64 = database.query_row("SELECT count(*) FROM redo", [], |row| row.get(0))?;
    assert_eq!(redo, 1);
    for fault in ["timing", "pool", "collision", "stale", "used"] {
        let policy = if fault == "timing" {
            SourceTrimPolicy::Ripple
        } else {
            SourceTrimPolicy::Overwrite
        };
        let mut request = combined(&before, "rejected", mixed(policy))?;
        let Command::ApplySourceTrim { resources, .. } = &mut request.command else {
            unreachable!()
        };
        match fault {
            "timing" => {
                resources.timing = Some(AudioTimingId {
                    allocation: revision("wrong"),
                    ordinal: 0,
                })
            }
            "pool" => resources.fillers.push(node("extra")),
            "collision" => resources.target_wrapper = Some(node("root")),
            "stale" => request.expected_revision = initial.revision_id().clone(),
            "used" => {
                request.new_revision = revision("saved");
                if let Some(timing) = &mut resources.timing {
                    timing.allocation = revision("saved");
                }
            }
            _ => unreachable!(),
        }
        let error = store.preview_source_trim_edit(&request).unwrap_err();
        if fault == "used" {
            assert_eq!(error.code(), "RevisionReused");
        }
        assert!(store.preview(&request).is_err(), "{fault}");
        assert!(store.commit(&request).is_err(), "{fault}");
        assert_eq!(stored(&database)?, rows, "{fault}");
    }
    store.redo(before.revision_id(), revision("redo-survives"))?;
    store.validate()?;
    Ok(())
}

#[test]
fn every_used_source_requires_its_receipt_and_original_even_when_overwrite_retires_it() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let clean = stored(&database)?;
    let ripple = combined(&before, "rejected-ripple", mixed(SourceTrimPolicy::Ripple))?;
    let retire = combined(
        &before,
        "rejected-overwrite",
        SourceTrimIntent {
            out_frames: 2,
            roll_frames: 1,
            policy: SourceTrimPolicy::Overwrite,
            ..Default::default()
        },
    )?;
    let zero = combined(&before, "zero", SourceTrimIntent::default())?;
    for (asset, other) in [("left", "right"), ("right", "left")] {
        let qualification = before.assets()[&id(asset)]
            .source_qualification
            .as_ref()
            .unwrap();
        let (content, original_ref, receipt): (String, String, Vec<u8>) = database.query_row(
            "SELECT original_content_id,original_ref,snapshot FROM source_qualifications WHERE id=?1",
            [qualification.as_str()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let ownership: String = database.query_row(
            "SELECT record FROM original_media WHERE content_id=?1",
            [&content],
            |row| row.get(0),
        )?;
        let mut wrong_owner: serde_json::Value = serde_json::from_str(&ownership)?;
        wrong_owner["sha256"] = serde_json::to_value([0_u8; 32])?;
        let other_receipt = store.registered_source(before.revision_id(), &id(other))?;
        let ready_row: (String, i64, i64) = database.query_row(
            "SELECT document,depth,json_bound FROM revisions WHERE id='ready'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        for fault in ["missing", "receipt", "ownership", "asset"] {
            match fault {
                "missing" => {
                    database.execute(
                        "DELETE FROM source_qualifications WHERE id=?1",
                        [qualification.as_str()],
                    )?;
                }
                "receipt" => {
                    database.execute(
                        "UPDATE source_qualifications SET snapshot=X'00' WHERE id=?1",
                        [qualification.as_str()],
                    )?;
                }
                "ownership" => {
                    database.execute(
                        "UPDATE original_media SET record=?1 WHERE content_id=?2",
                        rusqlite::params![wrong_owner.to_string(), content],
                    )?;
                }
                "asset" => {
                    let mut document = serde_json::to_value(&before)?;
                    document["assets"][asset]["content_hash"] = serde_json::json!("a".repeat(64));
                    database.execute(
                        "UPDATE revisions SET document=?1,depth=0,json_bound=length(CAST(?1 AS BLOB)) WHERE id='ready'",
                        [document.to_string()],
                    )?;
                    // The open store keeps its validated head in memory; a new session
                    // revalidates the stored revision and rejects the changed asset.
                    assert!(matches!(
                        ProjectStore::open(&path, AccessMode::ReadOnly),
                        Err(StoreError::SourceRegistration(_) | StoreError::History(_))
                    ));
                    database.execute(
                        "UPDATE revisions SET document=?1,depth=?2,json_bound=?3 WHERE id='ready'",
                        rusqlite::params![ready_row.0, ready_row.1, ready_row.2],
                    )?;
                    continue;
                }
                _ => unreachable!(),
            }
            let rows = stored(&database)?;
            assert_eq!(
                store.registered_source(before.revision_id(), &id(other))?,
                other_receipt
            );
            for request in [&ripple, &retire] {
                assert!(
                    matches!(
                        store.preview_source_trim_edit(request),
                        Err(StoreError::SourceRegistration(_))
                    ),
                    "{asset} {fault}"
                );
                assert!(
                    matches!(
                        store.preview(request),
                        Err(StoreError::SourceRegistration(_))
                    ),
                    "{asset} {fault}"
                );
                assert!(
                    matches!(
                        store.commit(request),
                        Err(StoreError::SourceRegistration(_))
                    ),
                    "{asset} {fault}"
                );
                assert_eq!(stored(&database)?, rows, "{asset} {fault}");
            }
            // R=0, no overlay: the available neighbor is only a diagnostic.
            // A zero preview must validate A without requiring unused B media.
            if asset == "left" {
                assert!(matches!(
                    store.preview_source_trim_edit(&zero),
                    Err(StoreError::SourceRegistration(_))
                ));
            } else {
                assert!(store.preview_source_trim_edit(&zero)?.edit.is_none());
            }
            assert_eq!(stored(&database)?, rows);
            database.execute("INSERT OR REPLACE INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
                rusqlite::params![qualification.as_str(), content, original_ref, receipt])?;
            database.execute(
                "UPDATE original_media SET record=?1 WHERE content_id=?2",
                rusqlite::params![ownership, content],
            )?;
            assert_eq!(stored(&database)?, clean);
        }
    }
    store.validate()?;
    Ok(())
}
