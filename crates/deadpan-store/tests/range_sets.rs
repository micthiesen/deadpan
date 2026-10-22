//! Durable range unions use synthetic structural metadata, not decoded media.
//! Fixtures and mark loss use public commands; only forgery tests alter wire data.

use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
type DatabaseCells = Vec<(String, Vec<Vec<String>>)>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn name(value: char) -> RegisterName {
    RegisterName::new(value).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn ranges() -> [FrameRange; 2] {
    [range(2, 6), range(18, 22)]
}

fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}

fn request(document: &ProjectDocument, id: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(id),
        command,
    }
}

fn edit(document: &ProjectDocument, id: &str, command: Command) -> Result<ProjectDocument> {
    let transaction = apply(document, &request(document, id, command))?;
    let after = transaction.forward.apply(document)?;
    assert_eq!(transaction.inverse.apply(&after)?, *document);
    Ok(after)
}

fn hold(count: i64) -> BeatNode {
    BeatNode::hold(
        "Temporary mark host",
        HoldRecipe {
            duration: frames(count),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn fixture() -> Result<ProjectDocument> {
    let document = ProjectDocument::new(
        ProjectId::new("durable-range-union")?,
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let time_base = SourceTimeBase::new(1001, 30_000)?;
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 24,
            time_base,
        },
    )?;
    let asset = AssetId::new("synthetic")?;
    let document = edit(
        &document,
        "fixture-asset",
        Command::AddAsset {
            id: asset.clone(),
            asset: AssetRecord {
                label: "Synthetic source metadata; no media qualification claim".into(),
                content_hash: "a".repeat(64),
                video: Some(span),
                audio: None,
                still_image: false,
                frame_count: Some(frames(24)),
                source_qualification: None,
            },
        },
    )?;
    let mut source = hold(24);
    source.label = "Complete immutable source recipe".into();
    source.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(24),
            edit_window: None,
            video: SourceVideo::Stream { asset, span },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
    let document = edit(
        &document,
        "fixture-source",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("source"),
                nodes: BTreeMap::from([(node("source"), source)]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?;
    let document = edit(
        &document,
        "fixture-host",
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("temporary-host"),
                nodes: BTreeMap::from([(node("temporary-host"), hold(1))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?;
    let document = edit(
        &document,
        "fixture-mark",
        Command::SetMark {
            id: MarkId::new("shared")?,
            owner: node("source"),
            label: "One logical cue with a deleted external host".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("temporary-host"),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )?;
    assert_eq!(
        document.marks()[&MarkId::new("shared")?].state,
        MarkState::Bound
    );
    let document = edit(
        &document,
        "fixture-delete-host",
        Command::DeleteRipple {
            node: node("temporary-host"),
            timing: timing("fixture-delete-host"),
        },
    )?;
    let mark = &document.marks()[&MarkId::new("shared")?];
    assert_eq!(mark.owner, node("source"));
    assert_eq!(mark.binding_count(), 1);
    assert!(matches!(mark.state, MarkState::Unresolved { .. }));
    assert!(!document.nodes().contains_key(&node("temporary-host")));
    assert_eq!(document.duration()?, frames(24));
    Ok(document)
}

/// Compare every table's typed values, including register contents, immutable
/// checkpoints and redo state. A rejected preview or write publishes no SQL.
fn cells(package: &Path) -> Result<DatabaseCells> {
    let database = Connection::open_with_flags(
        package.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let tables = database
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut result = Vec::new();
    for table in tables {
        let query = format!("SELECT * FROM \"{}\"", table.replace('"', "\"\""));
        let mut statement = database.prepare(&query)?;
        let columns = statement.column_count();
        let mut values = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| Ok(format!("{:?}", row.get_ref(column)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values.sort();
        result.push((table, values));
    }
    Ok(result)
}

fn history_count(package: &Path) -> Result<i64> {
    let database = Connection::open_with_flags(
        package.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    Ok(database.query_row("SELECT count(*) FROM history", [], |row| row.get(0))?)
}

fn capture(document: &ProjectDocument, id: &str) -> Result<Arc<CapturedEditSet>> {
    let set = CapturedEditSet::capture_ranges(document, document.root(), &ranges(), timing(id))?;
    set.validate_capture(document)?;
    assert_eq!(set.duration(), frames(8));
    assert_eq!(set.components().len(), 2);
    for (ordinal, (component, selected)) in set.components().iter().zip(ranges()).enumerate() {
        component.validate_capture(document)?;
        assert_eq!(component.parent(), document.root());
        assert_eq!(component.range(), selected);
        assert_eq!(
            component.selection(),
            &SliceCaptureSelection::Range { range: selected }
        );
        assert_eq!(component.capture_timing().allocation, revision(id));
        assert_eq!(component.capture_timing().ordinal, u32::try_from(ordinal)?);
        assert_eq!(component.revision_id(), document.revision_id());
    }
    assert_eq!(set.identity_requirements()?.marks, 1);
    assert_eq!(CapturedEditSet::from_json(&set.to_json()?)?, set);
    Ok(Arc::new(set))
}

fn identities(set: &CapturedEditSet, prefix: &str) -> Result<EditSetPasteIdentities> {
    let requirements = set.identity_requirements()?;
    Ok(EditSetPasteIdentities {
        group: node(&format!("{prefix}-group")),
        components: requirements
            .components
            .iter()
            .enumerate()
            .map(|(part, count)| {
                assert_eq!(
                    count.marks, 0,
                    "logical marks are allocated once for the union"
                );
                EditSetComponentIdentities {
                    nodes: (0..count.nodes)
                        .map(|i| node(&format!("{prefix}-part-{part}-node-{i}")))
                        .collect(),
                    aliases: (0..count.aliases)
                        .map(|i| node(&format!("{prefix}-part-{part}-alias-{i}")))
                        .collect(),
                }
            })
            .collect(),
        marks: (0..requirements.marks)
            .map(|i| MarkId::new(format!("{prefix}-mark-{i}")))
            .collect::<std::result::Result<_, _>>()?,
    })
}

fn compound(
    document: &ProjectDocument,
    id: &str,
    version: u64,
    steps: Vec<ResolvedStep>,
) -> Result<CommandRequest> {
    Ok(request(
        document,
        id,
        Command::Compound {
            transaction: ResolvedTransaction::new(version, BTreeMap::new(), steps)?,
        },
    ))
}

/// All captures precede deletion. Descending global ranges remain valid while
/// earlier content is untouched; each split pool comes from that staged input.
fn cut_steps(
    document: &ProjectDocument,
    set: &Arc<CapturedEditSet>,
    prefix: &str,
) -> Result<(Vec<ResolvedStep>, ProjectDocument)> {
    let mut staged = document.clone();
    let mut steps = vec![ResolvedStep::Yank {
        name: name('c'),
        value: Arc::new(RegisterValue::EditedSet {
            set: Arc::clone(set),
        }),
    }];
    for (index, component) in set.components().iter().rev().enumerate() {
        let selected = component.range();
        let required = staged
            .range_deletion(document.root(), selected)?
            .required_ids;
        assert!(required > 0, "both selected windows have partial endpoints");
        let id = format!("{prefix}-{index}");
        let leaf = LeafEdit::new(
            revision(&id),
            Command::DeleteRange {
                parent: document.root().clone(),
                range: selected,
                identities: SplitIdentities {
                    nodes: (0..required)
                        .map(|i| node(&format!("{id}-split-{i}")))
                        .collect(),
                },
                timing: timing(&id),
            },
        )?;
        let transaction = apply(&staged, &leaf.request(&staged))?;
        let next = transaction.forward.apply(&staged)?;
        assert_eq!(transaction.inverse.apply(&next)?, staged);
        staged = next;
        steps.push(ResolvedStep::Edit { edit: leaf });
    }
    assert_eq!(staged.duration()?, frames(16));
    Ok((steps, staged))
}

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

fn check_copied_mark(document: &ProjectDocument, source: &ProjectDocument, prefix: &str) -> Result {
    let original = &source.marks()[&MarkId::new("shared")?];
    let copied = &document.marks()[&MarkId::new(format!("{prefix}-mark-0"))?];
    assert_eq!(copied.binding_count(), 2);
    assert_eq!(copied.label, original.label);
    assert_eq!(copied.loss_policy, original.loss_policy);
    assert_eq!(copied.boundary.bias, original.boundary.bias);
    let owners = copied
        .bindings()
        .map(|binding| {
            assert_eq!(binding.coordinate, original.boundary.coordinate);
            assert_eq!(binding.state, original.state);
            assert!(matches!(
                document.nodes()[&binding.owner].kind,
                NodeKind::Source { .. }
            ));
            assert_ne!(binding.owner, original.owner);
            binding.owner
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        owners.len(),
        2,
        "equal source-side bindings need two fresh physical owners"
    );
    let components = document
        .children(&node(&format!("{prefix}-group")))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(components.len(), 2);
    for (component, expected) in components.iter().zip(ranges()) {
        let parts = document.children(component).collect::<Vec<_>>();
        assert_eq!(parts.len(), 1);
        let NodeKind::Retime {
            child,
            duration,
            mapping,
            purpose,
            ..
        } = &document.nodes()[parts[0]].kind
        else {
            panic!("a partial component retains its transparent partition");
        };
        assert_eq!(*purpose, RetimePurpose::Partition);
        assert_eq!(*mapping, expected);
        assert_eq!(*duration, expected.duration());
        assert_eq!(document.nodes()[child], source.nodes()[&node("source")]);
        assert!(owners.contains(child));
    }
    Ok(())
}

#[test]
fn range_union_copy_and_atomic_cut_paste_reopen_with_two_bindings_and_one_undo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("range-union.deadpan");
    let initial = fixture()?;
    let mut store = ProjectStore::create(&package, &initial)?;
    let before = store.snapshot()?;
    let availability = store.history_availability()?;
    let set = capture(&before, "entry-capture")?;
    let value = RegisterValue::EditedSet {
        set: Arc::clone(&set),
    };
    let unchanged = cells(&package)?;
    let preview = store.preview_register(
        before.project_id(),
        before.revision_id(),
        0,
        name('a'),
        value.clone(),
    )?;
    assert_eq!(cells(&package)?, unchanged);
    let copied_bank = store.save_register_at(
        before.project_id(),
        before.revision_id(),
        0,
        name('a'),
        value,
    )?;
    assert_eq!(copied_bank, preview);
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.history_availability()?, availability);
    assert_eq!(history_count(&package)?, 0);

    let (mut steps, cut) = cut_steps(&before, &set, "cut-range")?;
    steps.push(ResolvedStep::Paste {
        name: name('c'),
        edit: LeafEdit::new(
            revision("paste-range-union"),
            Command::SpliceEditSet {
                parent: cut.root().clone(),
                index: cut.children(cut.root()).count(),
                set: (*set).clone(),
                identities: identities(&set, "pasted")?,
                timing: timing("paste-range-union"),
            },
        )?,
    });
    let command = compound(&before, "atomic-range-edit", copied_bank.version, steps)?;
    let unchanged = cells(&package)?;
    let preview = store.preview_compound(&command)?;
    assert_eq!(cells(&package)?, unchanged);
    let outcome = store.commit_compound(&command, None)?;
    assert!(outcome.committed.is_some());
    assert_eq!(outcome.register_bank, preview.register_bank);
    let bank = outcome.register_bank;
    let after = store.snapshot()?;
    assert_eq!(after.duration()?, frames(24));
    assert_eq!(history_count(&package)?, 1);
    check_copied_mark(&after, &before, "pasted")?;
    assert_eq!(store.capture_snapshot_at(before.revision_id())?, before);
    set.validate_capture(&store.capture_snapshot_at(before.revision_id())?)?;
    assert_eq!(bank.entries[&name('a')], copied_bank.entries[&name('a')]);
    assert_eq!(
        bank.entries[&name('c')],
        bank.entries[&RegisterName::unnamed()]
    );

    store.undo(after.revision_id(), revision("undo-entire-range-edit"))?;
    same_authored(&store.snapshot()?, &before)?;
    assert_eq!(
        store.registers()?,
        bank,
        "timeline Undo retains the independent union copy"
    );
    assert!(!store.history_availability()?.0);
    store.checkpoint()?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.redo(
        &revision("undo-entire-range-edit"),
        revision("redo-entire-range-edit"),
    )?;
    same_authored(&store.snapshot()?, &after)?;
    check_copied_mark(&store.snapshot()?, &before, "pasted")?;
    assert_eq!(store.registers()?, bank);
    store.checkpoint()?;
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    same_authored(&store.snapshot()?, &after)?;
    assert_eq!(store.registers()?, bank);
    let report = store.validate_report(true)?;
    assert_eq!(report.verified_by_receipt, 0);
    assert_eq!(report.replayed + 1, report.revisions);
    Ok(())
}

#[test]
fn range_union_late_failure_rolls_back_every_table_and_staged_capture_reopens() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("range-checkpoint.deadpan");
    let mut store = ProjectStore::create(&package, &fixture()?)?;
    let before = store.snapshot()?;
    let set = capture(&before, "failed-capture")?;
    let (mut steps, _) = cut_steps(&before, &set, "failed-delete")?;
    steps.push(ResolvedStep::Edit {
        edit: LeafEdit::new(
            revision("missing-leaf"),
            Command::DeleteRipple {
                node: node("missing"),
                timing: timing("missing-leaf"),
            },
        )?,
    });
    let bad = compound(&before, "failed-range-edit", 0, steps)?;
    let unchanged = cells(&package)?;
    assert!(store.preview_compound(&bad).is_err());
    assert!(store.commit_compound(&bad, None).is_err());
    assert_eq!(cells(&package)?, unchanged);
    assert!(store.registers()?.entries.is_empty());

    let leaf = LeafEdit::new(
        revision("staged-source"),
        Command::Rename {
            node: node("source"),
            label: "Source changed before capture in this transaction".into(),
        },
    )?;
    let staged = apply(&before, &leaf.request(&before))?
        .forward
        .apply(&before)?;
    let set = capture(&staged, "staged-range-capture")?;
    let (cut, expected) = cut_steps(&staged, &set, "staged-delete")?;
    let mut steps = vec![ResolvedStep::Edit { edit: leaf }];
    steps.extend(cut);
    let command = compound(&before, "checkpoint-range-edit", 0, steps)?;
    let unchanged = cells(&package)?;
    let preview = store.preview_compound(&command)?;
    assert_eq!(cells(&package)?, unchanged);
    let outcome = store.commit_compound(&command, None)?;
    assert_eq!(outcome.register_bank, preview.register_bank);
    same_authored(&store.snapshot()?, &expected)?;
    assert_eq!(
        store.capture_snapshot_at(&revision("staged-source"))?,
        staged
    );
    let RegisterValue::EditedSet { set: retained } =
        outcome.register_bank.entries[&name('c')].as_ref()
    else {
        panic!("one durable union value");
    };
    assert_eq!(retained, &set);
    retained.validate_capture(&staged)?;
    assert!(retained.validate_capture(&before).is_err());
    assert_eq!(history_count(&package)?, 1);
    store.checkpoint()?;
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(
        store.capture_snapshot_at(&revision("staged-source"))?,
        staged
    );
    assert_eq!(store.registers()?, outcome.register_bank);
    let report = store.validate_report(true)?;
    assert_eq!(report.verified_by_receipt, 0);
    assert_eq!(report.replayed + 1, report.revisions);
    Ok(())
}

#[test]
fn range_union_forged_repeated_recipe_and_mixed_wire_publish_no_sql() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("range-forgery.deadpan");
    let mut store = ProjectStore::create(&package, &fixture()?)?;
    let before = store.snapshot()?;
    let set = capture(&before, "honest-capture")?;
    let unchanged = cells(&package)?;
    let mut wire = serde_json::to_value(&*set)?;
    for component in wire["components"].as_array_mut().ok_or("component array")? {
        component["nodes"]["source"]["label"] = json!("Coherently forged complete source recipe");
    }
    // Both repeated records agree and each capsule is internally valid. Only
    // immutable historical admission can prove that the source recipe is false.
    let forged: CapturedEditSet = serde_json::from_value(wire)?;
    assert!(forged.validate_capture(&before).is_err());
    let value = RegisterValue::EditedSet {
        set: Arc::new(forged),
    };
    assert!(
        store
            .preview_register(
                before.project_id(),
                before.revision_id(),
                0,
                name('a'),
                value.clone()
            )
            .is_err()
    );
    assert!(
        store
            .save_register_at(
                before.project_id(),
                before.revision_id(),
                0,
                name('a'),
                value
            )
            .is_err()
    );
    assert_eq!(cells(&package)?, unchanged);

    let whole = CapturedEditSlice::capture_selection(
        &before,
        before.root(),
        &SliceCaptureSelection::Child {
            node: node("source"),
        },
        AudioTimingId {
            allocation: revision("honest-capture"),
            ordinal: 1,
        },
    )?;
    let mut mixed = serde_json::to_value(&*set)?;
    mixed["components"][1] = serde_json::to_value(whole)?;
    assert!(serde_json::from_value::<CapturedEditSet>(mixed).is_err());
    let mut inconsistent = serde_json::to_value(&*set)?;
    inconsistent["components"][1]["nodes"]["source"]["label"] =
        json!("One inconsistent repeated record");
    assert!(serde_json::from_value::<CapturedEditSet>(inconsistent).is_err());
    assert_eq!(cells(&package)?, unchanged);
    assert!(store.registers()?.entries.is_empty());
    assert_eq!(history_count(&package)?, 0);
    Ok(())
}

#[test]
fn range_union_stale_guards_refuse_but_known_historical_captures_remain_valid() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("range-stale.deadpan");
    let mut store = ProjectStore::create(&package, &fixture()?)?;
    let before = store.snapshot()?;
    let set = capture(&before, "historical-capture")?;
    let value = RegisterValue::EditedSet {
        set: Arc::clone(&set),
    };
    let bank = store.save_register_at(
        before.project_id(),
        before.revision_id(),
        0,
        name('a'),
        value.clone(),
    )?;
    let unchanged = cells(&package)?;
    assert!(
        store
            .preview_register(
                before.project_id(),
                before.revision_id(),
                0,
                name('b'),
                value.clone()
            )
            .is_err()
    );
    assert!(
        store
            .save_register_at(
                before.project_id(),
                before.revision_id(),
                0,
                name('b'),
                value.clone()
            )
            .is_err()
    );
    assert_eq!(cells(&package)?, unchanged);
    store.commit(&request(
        &before,
        "renamed-head",
        Command::Rename {
            node: node("source"),
            label: "Current recipe differs from retained capture".into(),
        },
    ))?;
    let current = store.snapshot()?;
    let unchanged = cells(&package)?;
    assert!(
        store
            .preview_register(
                before.project_id(),
                before.revision_id(),
                bank.version,
                name('b'),
                value.clone()
            )
            .is_err()
    );
    assert!(
        store
            .save_register_at(
                before.project_id(),
                before.revision_id(),
                bank.version,
                name('b'),
                value.clone()
            )
            .is_err()
    );
    assert_eq!(cells(&package)?, unchanged);
    assert!(set.validate_capture(&current).is_err());
    let bank = store.save_register_at(
        current.project_id(),
        current.revision_id(),
        bank.version,
        name('b'),
        value,
    )?;
    let RegisterValue::EditedSet { set: historical } = bank.entries[&name('b')].as_ref() else {
        panic!("historical range union");
    };
    assert_eq!(historical.revision_id(), before.revision_id());
    historical.validate_capture(&store.capture_snapshot_at(before.revision_id())?)?;

    let unrecorded = edit(
        &current,
        "unrecorded-revision",
        Command::Rename {
            node: node("source"),
            label: "This valid source snapshot never reached the store".into(),
        },
    )?;
    let unrecorded = RegisterValue::EditedSet {
        set: capture(&unrecorded, "unrecorded-capture")?,
    };
    let unchanged = cells(&package)?;
    assert!(
        store
            .preview_register(
                current.project_id(),
                current.revision_id(),
                bank.version,
                name('u'),
                unrecorded.clone()
            )
            .is_err()
    );
    assert!(
        store
            .save_register_at(
                current.project_id(),
                current.revision_id(),
                bank.version,
                name('u'),
                unrecorded
            )
            .is_err()
    );
    assert_eq!(cells(&package)?, unchanged);
    assert_eq!(store.registers()?, bank);
    store.validate()?;
    Ok(())
}
