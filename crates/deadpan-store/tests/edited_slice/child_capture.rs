//! Exact structural selectors retain store history and historical authority.
use super::*;
use deadpan_store::slice_preview::SliceViewIdentities;
use std::sync::atomic::AtomicBool;

#[path = "child_capture/range_compat.rs"]
mod range_compat;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "child_capture/retained_asset.rs"]
mod retained_asset;

fn capture_child(
    document: &ProjectDocument,
    parent: &str,
    child: &str,
) -> Result<CapturedEditSlice> {
    Ok(CapturedEditSlice::capture_selection(
        document,
        &node(parent),
        &SliceCaptureSelection::Child { node: node(child) },
        timing("child-capture"),
    )?)
}

fn identities(slice: &CapturedEditSlice, name: &str) -> Result<SlicePasteIdentities> {
    let count = slice.identity_requirements()?;
    Ok(SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..count.nodes)
                .map(|i| node(&format!("{name}-node-{i}")))
                .collect(),
            marks: (0..count.marks)
                .map(|i| MarkId::new(format!("{name}-mark-{i}")))
                .collect::<std::result::Result<_, _>>()?,
        },
        aliases: (0..count.aliases)
            .map(|i| node(&format!("{name}-alias-{i}")))
            .collect(),
    })
}

fn seam(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
    index: usize,
) -> Result<CommandRequest> {
    Ok(request(
        document,
        name,
        Command::SpliceSlice {
            parent: document.root().clone(),
            index,
            slice: slice.clone(),
            identities: identities(slice, name)?,
            timing: timing(name),
        },
    ))
}

fn view_ids(slice: &CapturedEditSlice, name: &str) -> Result<SliceViewIdentities> {
    Ok(SliceViewIdentities {
        empty_revision: revision(&format!("{name}-empty")),
        view_revision: revision(name),
        root: node(&format!("{name}-root")),
        paste: identities(slice, name)?,
    })
}

fn structural_document(timed: bool) -> Result<ProjectDocument> {
    let mut wire = serde_json::to_value(document()?)?;
    for child in ["empty-left", "empty-mid", "empty-right", "inside"] {
        wire["nodes"][child] = serde_json::to_value(BeatNode::sequence("Same label", vec![]))?;
    }
    wire["nodes"]["empty-mid"]["kind"] = serde_json::to_value(NodeKind::Sequence {
        children: vec![node("inside")],
    })?;
    wire["nodes"]["empty-mid"]["framing"] =
        serde_json::to_value(Framing::static_pose(FramingPose::new(
            ExactRatio::new(1, 3)?,
            ExactRatio::new(2, 3)?,
            ExactRatio::new(3, 2)?,
        )?)?)?;
    wire["nodes"]["empty-mid"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(ClipGain::new(GainDb::new(-3000)?, false, vec![], vec![])?),
    )?;
    let children = if timed {
        vec![
            node("lead"),
            node("empty-left"),
            node("empty-mid"),
            node("empty-right"),
            node("group"),
            node("tail"),
        ]
    } else {
        for key in ["lead", "group", "voice", "repeat", "echo", "tail"] {
            wire["nodes"].as_object_mut().unwrap().remove(key);
        }
        wire["marks"] = serde_json::json!({});
        // Remove lineage belonging to nodes omitted by this zero-time fixture.
        if let Some(lineage) = wire
            .get_mut("audio_lineage")
            .and_then(serde_json::Value::as_object_mut)
        {
            lineage.retain(|key, _| key == "root");
        }
        vec![node("empty-left"), node("empty-mid"), node("empty-right")]
    };
    wire["nodes"]["root"]["kind"] = serde_json::to_value(NodeKind::Sequence { children })?;
    for (key, owner, position, bias, state) in [
        (
            "empty-left-mark",
            "empty-mid",
            0,
            InsertionBias::Left,
            MarkState::Bound,
        ),
        (
            "empty-right-mark",
            "empty-mid",
            0,
            InsertionBias::Right,
            MarkState::Bound,
        ),
        (
            "nested-mark",
            "inside",
            0,
            InsertionBias::Right,
            MarkState::Bound,
        ),
        (
            "unresolved-mark",
            "inside",
            2,
            InsertionBias::Right,
            MarkState::Unresolved {
                reason: MarkLossReason::OutsideHost,
            },
        ),
    ] {
        wire["marks"][key] = serde_json::to_value(Mark {
            owner: node(owner),
            label: key.into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node(owner),
                    position: ExactRatio::integer(position),
                },
                bias,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state,
            fragments: vec![],
        })?;
    }
    let document = ProjectDocument::from_json(&wire.to_string())?;
    if !timed {
        return Ok(document);
    }
    // Existing clocks make a zero paste's no-reanchor promise observable.
    let bindings = capture_unbound_audio_bindings(&document, timing("existing-clocks"))?;
    let mut wire = serde_json::to_value(document)?;
    wire["audio_bindings"] = serde_json::to_value(bindings)?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn history_rows(path: &Path) -> Result<Vec<String>> {
    let db = Connection::open(path.join("project.sqlite"))?;
    Ok(db
        .prepare(
            "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        )?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?)
}

#[test]
fn whole_positive_child_excludes_same_time_empty_neighbors_and_previews_do_not_author() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("positive-child.deadpan");
    let before = structural_document(true)?;
    let store = ProjectStore::create(&path, &before)?;
    let cells = authored(&path)?;
    let slice = capture_child(&before, "root", "group")?;
    assert_eq!(
        slice.selection(),
        &SliceCaptureSelection::Child {
            node: node("group")
        }
    );
    assert_eq!(
        slice.range(),
        FrameRange::new(ProjectFrame(2), ProjectFrame(17))?
    );
    let wire = serde_json::to_value(&slice)?;
    assert_eq!(wire["parts"].as_array().unwrap().len(), 1);
    assert_eq!(wire["parts"][0]["root"], "group");
    assert!(wire["nodes"].get("empty-left").is_none());
    assert!(wire["nodes"].get("empty-mid").is_none());
    assert!(wire["nodes"].get("empty-right").is_none());
    let command = seam(&before, &slice, "positive-paste", 2)?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let preview = reader.preview(&command)?;
    assert_eq!(preview.duration_delta, 15);
    assert_eq!(
        reader.preview_edit_slice(&command).err().unwrap().code(),
        "ProjectReadOnly"
    );
    assert_eq!(
        reader
            .view_edit_slice(&slice, view_ids(&slice, "read-only-source")?)
            .err()
            .unwrap()
            .code(),
        "ProjectReadOnly"
    );
    let proposed = store.preview_edit_slice(&command)?;
    let source = store.view_edit_slice(&slice, view_ids(&slice, "whole-source")?)?;
    assert_eq!(**proposed.document(), preview.forward.apply(&before)?);
    assert_eq!(source.document().duration()?.frames(), 15);
    assert!(
        source
            .document()
            .nodes()
            .values()
            .any(|node| node.label == "Group")
    );
    assert!(
        source
            .document()
            .nodes()
            .values()
            .all(|node| node.label != "Same label")
    );
    assert_eq!(authored(&path)?, cells);
    assert_eq!(store.snapshot()?, before);
    Ok(())
}

#[test]
fn historical_empty_child_paste_is_one_structural_revision_with_exact_slot_and_undo_redo() -> Result
{
    for timed in [false, true] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("empty-child.deadpan");
        let baseline = structural_document(timed)?;
        let mut store = ProjectStore::create(&path, &baseline)?;
        let slice = capture_child(&baseline, "root", "empty-mid")?;
        let slice_json = slice.to_json()?;
        assert_eq!(slice.duration(), FrameDuration::ZERO);
        assert_eq!(slice.identity_requirements()?.timings, 0);
        assert_eq!(slice.identity_requirements()?.marks, 4);
        let at = if timed { 2 } else { 0 };
        assert_eq!(
            slice.range(),
            FrameRange::new(ProjectFrame(at), ProjectFrame(at))?
        );
        let cells = authored(&path)?;
        let source = store.view_edit_slice(&slice, view_ids(&slice, "empty-source")?)?;
        assert_eq!(source.document().duration()?, FrameDuration::ZERO);
        assert_eq!(source.document().marks().len(), 4);
        assert!(source.document().audio_bindings().is_empty());
        assert!(source.sources().is_empty());
        assert_eq!(authored(&path)?, cells);
        store.commit(&request(
            &baseline,
            "delete-empty",
            Command::DeleteRipple {
                node: node("empty-mid"),
                timing: timing("delete-empty"),
            },
        ))?;
        let before = store.snapshot()?;
        assert!(!before.nodes().contains_key(&node("empty-mid")));
        drop(store);
        assert!(source.check_live(&AtomicBool::new(false)).is_err());
        let slice = CapturedEditSlice::from_json(&slice_json)?;
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        let slot = if timed { 2 } else { 1 };
        let mut command = seam(&before, &slice, "paste-empty", slot)?;
        // No timing ordinal is consumed by structural insertion.
        if let Command::SpliceSlice { timing, .. } = &mut command.command {
            timing.ordinal = u32::MAX;
        }
        let cells = authored(&path)?;
        let preview = store.preview(&command)?;
        let view = store.preview_edit_slice(&command)?;
        assert_eq!(preview.duration_delta, 0);
        assert_eq!(authored(&path)?, cells);
        assert_eq!(store.commit(&command)?.edit, preview);
        let after = store.snapshot()?;
        assert_eq!(**view.document(), after);
        assert_eq!(after.duration()?, before.duration()?);
        assert_ne!(after, before);
        assert_eq!(history_rows(&path)?.len(), 2);
        let children: Vec<_> = after.children(after.root()).cloned().collect();
        assert_eq!(children[slot - 1], node("empty-left"));
        assert_eq!(children[slot], node("paste-empty-node-0"));
        assert_eq!(children[slot + 1], node("empty-right"));
        assert_eq!(after.node_duration(&children[slot])?, FrameDuration::ZERO);
        let copied: Vec<_> = after.children(&children[slot]).cloned().collect();
        assert_eq!(copied.len(), 1);
        let copied_node = &after.nodes()[&copied[0]];
        let original_node = &baseline.nodes()[&node("empty-mid")];
        assert_eq!(copied_node.label, original_node.label);
        assert_eq!(copied_node.framing, original_node.framing);
        assert_eq!(copied_node.audio_treatments, original_node.audio_treatments);
        assert_eq!(copied_node.audio_edges, original_node.audio_edges);
        assert_eq!(after.children(&copied[0]).count(), 1);
        assert_eq!(after.audio_bindings(), before.audio_bindings());
        for (node, lineage) in before.audio_lineage() {
            assert_eq!(&after.audio_lineage()[node], lineage);
        }
        for (mark, value) in before.marks() {
            assert_eq!(&after.marks()[mark], value);
        }
        assert_eq!(after.sounds(), before.sounds());
        assert_eq!(after.sound_routes(), before.sound_routes());
        assert_eq!(after.sound_allowances(), before.sound_allowances());
        let again = capture_child(&after, "root", children[slot].as_str())?;
        let again_view =
            store.view_edit_slice(&again, view_ids(&again, "copied-empty-wrapper")?)?;
        assert_eq!(again_view.document().marks().len(), 4);
        assert_eq!(again_view.document().duration()?, FrameDuration::ZERO);
        store.validate()?;
        let saved_history = history_rows(&path)?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.undo(after.revision_id(), revision("undo-empty-paste"))?;
        assert_authored(&store.snapshot()?, &before)?;
        store.redo(&revision("undo-empty-paste"), revision("redo-empty-paste"))?;
        assert_authored(&store.snapshot()?, &after)?;
        store.validate()?;
        assert_eq!(history_rows(&path)?, saved_history);
        drop(store);
        ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    }
    Ok(())
}

// Rename one address consistently inside a hostile payload. This fabricates a
// structurally valid sibling, rather than relying on a dangling-ID rejection.
fn rename_address(value: &mut serde_json::Value, old: &str, new: &str) {
    match value {
        serde_json::Value::String(text) if text == old => *text = new.into(),
        serde_json::Value::Array(values) => {
            for value in values {
                rename_address(value, old, new);
            }
        }
        serde_json::Value::Object(fields) => {
            if let Some(value) = fields.remove(old) {
                fields.insert(new.into(), value);
            }
            for value in fields.values_mut() {
                rename_address(value, old, new);
            }
        }
        _ => {}
    }
}

#[test]
fn historical_recapture_rejects_forged_same_time_child_parent_bounds_and_contents_without_writes()
-> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("forged-child.deadpan");
    let before = structural_document(true)?;
    let mut store = ProjectStore::create(&path, &before)?;
    let slice = capture_child(&before, "root", "empty-mid")?;
    let cells = authored(&path)?;
    for case in ["sibling", "parent", "bounds", "payload"] {
        let mut wire = serde_json::to_value(&slice)?;
        match case {
            "sibling" => rename_address(&mut wire, "empty-mid", "empty-left"),
            "parent" => wire["parent"] = serde_json::json!("empty-left"),
            "bounds" => {
                wire["range"] =
                    serde_json::to_value(FrameRange::new(ProjectFrame(8), ProjectFrame(8))?)?;
                wire["parts"][0]["source_start"] = serde_json::to_value(ProjectFrame(8))?;
            }
            _ => {
                wire["nodes"]["empty-mid"]["label"] =
                    serde_json::json!("Invented historical content")
            }
        }
        let forged = CapturedEditSlice::from_json(&wire.to_string())?;
        let command = seam(&before, &forged, &format!("forged-{case}"), 3)?;
        let command: CommandRequest = serde_json::from_str(&serde_json::to_string(&command)?)?;
        // Valid structure alone does not prove the named historical selection.
        apply(&before, &command)?;
        assert!(forged.validate_capture(&before).is_err(), "case {case}");
        assert!(store.preview(&command).is_err(), "case {case}");
        assert!(store.preview_edit_slice(&command).is_err(), "case {case}");
        assert!(
            store
                .view_edit_slice(&forged, view_ids(&forged, &format!("view-{case}"))?)
                .is_err(),
            "case {case}"
        );
        assert!(store.commit(&command).is_err(), "case {case}");
        assert_eq!(authored(&path)?, cells);
        assert_eq!(store.snapshot()?, before);
    }
    let mut absent = serde_json::to_value(&slice)?;
    absent.as_object_mut().unwrap().remove("selection");
    assert!(
        CapturedEditSlice::from_json(&absent.to_string()).is_err(),
        "an old empty Range cannot imply Child"
    );
    let mut command = serde_json::to_value(seam(&before, &slice, "missing-selector", 3)?)?;
    command["command"]["slice"] = absent;
    assert!(serde_json::from_value::<CommandRequest>(command).is_err());
    assert_eq!(authored(&path)?, cells);
    store.validate()?;
    Ok(())
}
