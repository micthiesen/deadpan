//! Durable union registers and historical placement. Synthetic structural media
//! metadata exercises ownership only; real decoding belongs to CLI qualification.
use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::Connection;
use serde_json::json;
use std::{collections::BTreeMap, path::Path, sync::Arc};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

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
fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}
fn hold(label: &str, count: i64) -> BeatNode {
    BeatNode::hold(
        label,
        HoldRecipe {
            duration: frames(count),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn request(document: &ProjectDocument, id: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(id),
        command,
    }
}
fn apply_fixture(document: ProjectDocument, id: &str, command: Command) -> Result<ProjectDocument> {
    let request = request(&document, id, command);
    let edit = apply(&document, &request)?;
    let after = edit.forward.apply(&document)?;
    assert_eq!(edit.inverse.apply(&after)?, document);
    Ok(after)
}
fn insert(
    document: ProjectDocument,
    parent: &str,
    index: usize,
    id: &str,
    beat: BeatNode,
) -> Result<ProjectDocument> {
    apply_fixture(
        document,
        &format!("seed-{id}"),
        Command::Insert {
            parent: node(parent),
            index,
            subtree: Subtree {
                root: node(id),
                nodes: BTreeMap::from([(node(id), beat)]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    )
}
fn children(document: &ProjectDocument, parent: &str) -> Vec<NodeId> {
    let NodeKind::Sequence { children } = &document.nodes()[&node(parent)].kind else {
        panic!("ordinary parent")
    };
    children.clone()
}

struct Fixture {
    document: ProjectDocument,
    members: Vec<NodeId>,
    left: NodeId,
    right: NodeId,
}

fn fixture() -> Result<Fixture> {
    let document = ProjectDocument::new(
        ProjectId::new("durable-node-union")?,
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 30)?,
        },
        SourceTimestamp {
            ticks: 10,
            time_base: SourceTimeBase::new(1, 30)?,
        },
    )?;
    let mut wire = serde_json::to_value(document)?;
    wire["assets"]["synthetic"] = serde_json::to_value(AssetRecord {
        label: "Synthetic metadata; no decoding claim".into(),
        content_hash: "a".repeat(64),
        video: Some(span),
        audio: None,
        still_image: false,
        frame_count: None,
        source_qualification: None,
    })?;
    let document = ProjectDocument::from_json(&wire.to_string())?;
    let mut source = hold("Shared marked source", 10);
    source.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(10),
            edit_window: None,
            video: SourceVideo::Stream {
                asset: AssetId::new("synthetic")?,
                span,
            },
            audio: None,
            video_mapping: SourceVideoMapping::FitBeat,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
    let document = insert(document, "root", 0, "source", source)?;
    let document = apply_fixture(
        document,
        "seed-mark",
        Command::SetMark {
            id: MarkId::new("shared")?,
            owner: node("source"),
            label: "One logical mark".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Source {
                    asset: AssetId::new("synthetic")?,
                    moment: SourceMoment::Timestamp {
                        stream: SourceStream::Video,
                        timestamp: SourceTimestamp {
                            ticks: 7,
                            time_base: SourceTimeBase::new(1, 30)?,
                        },
                    },
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::DeleteOwned,
        },
    )?;
    let document = apply_fixture(
        document,
        "seed-split",
        Command::Split {
            node: node("source"),
            at: frames(4),
            identities: SplitIdentities {
                nodes: (0..16)
                    .map(|index| node(&format!("split-{index}")))
                    .collect(),
            },
        },
    )?;
    let parts = children(&document, "root");
    assert_eq!(parts.len(), 2);
    assert_eq!(document.marks()[&MarkId::new("shared")?].binding_count(), 2);
    let document = apply_fixture(
        document,
        "seed-left-group",
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 1,
            id: node("left-group"),
            label: "Left actual parent".into(),
        },
    )?;
    let document = apply_fixture(
        document,
        "seed-right-group",
        Command::Group {
            parent: node("root"),
            start: 1,
            end: 2,
            id: node("right-group"),
            label: "Right actual parent".into(),
        },
    )?;
    let document = insert(document, "root", 1, "gap", hold("Unselected gap", 2))?;
    let document = insert(document, "root", 0, "lead", hold("Lead", 3))?;
    let document = insert(document, "root", 4, "tail", hold("Tail", 3))?;
    let document = insert(
        document,
        "left-group",
        0,
        "empty-a",
        BeatNode::sequence("Empty A", vec![]),
    )?;
    let document = insert(
        document,
        "right-group",
        1,
        "empty-b",
        BeatNode::sequence("Empty B", vec![]),
    )?;
    assert_eq!(document.duration()?.frames(), 18);
    Ok(Fixture {
        document,
        members: vec![
            node("empty-a"),
            parts[0].clone(),
            parts[1].clone(),
            node("empty-b"),
        ],
        left: parts[0].clone(),
        right: parts[1].clone(),
    })
}

fn cells(package: &Path) -> Result<Vec<String>> {
    let database = Connection::open(package.join("project.sqlite"))?;
    let mut result = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
        "SELECT json_array(owner_revision,ordinal,step_revision,document) FROM transaction_steps ORDER BY owner_revision,ordinal",
        "SELECT json_array(singleton,version,bank_digest) FROM register_state",
        "SELECT json_array(name,content_id) FROM registers ORDER BY name",
        "SELECT json_array(id,capture_revision,capture_step,value) FROM register_contents ORDER BY id",
    ] {
        result.extend(
            database
                .prepare(query)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(result)
}

fn capture(
    document: &ProjectDocument,
    members: &[NodeId],
    id: &str,
) -> Result<Arc<CapturedEditSet>> {
    let set = Arc::new(CapturedEditSet::capture(
        document,
        document.root(),
        members,
        timing(id),
    )?);
    set.validate_capture(document)?;
    assert_eq!(
        serde_json::from_value::<CapturedEditSet>(serde_json::to_value(&*set)?)?,
        *set
    );
    for component in set.components() {
        assert_eq!(component.revision_id(), document.revision_id());
        component.validate_capture(document)?;
    }
    Ok(set)
}
fn identities(set: &CapturedEditSet, id: &str) -> Result<EditSetPasteIdentities> {
    let requirements = set.identity_requirements()?;
    Ok(EditSetPasteIdentities {
        group: node(&format!("{id}-group")),
        components: requirements
            .components
            .iter()
            .enumerate()
            .map(|(part, count)| EditSetComponentIdentities {
                nodes: (0..count.nodes)
                    .map(|index| node(&format!("{id}-part-{part}-node-{index}")))
                    .collect(),
                aliases: (0..count.aliases)
                    .map(|index| node(&format!("{id}-part-{part}-alias-{index}")))
                    .collect(),
            })
            .collect(),
        marks: (0..requirements.marks)
            .map(|index| MarkId::new(format!("{id}-mark-{index}")).unwrap())
            .collect(),
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
fn cut_steps(set: &Arc<CapturedEditSet>, prefix: &str) -> Result<Vec<ResolvedStep>> {
    let mut steps = vec![ResolvedStep::Yank {
        name: name('c'),
        value: Arc::new(RegisterValue::EditedSet {
            set: Arc::clone(set),
        }),
    }];
    for (index, component) in set.components().iter().rev().enumerate() {
        let SliceCaptureSelection::Child { node } = component.selection() else {
            panic!("whole node set")
        };
        let id = format!("{prefix}-{index}");
        steps.push(ResolvedStep::Edit {
            edit: LeafEdit::new(
                revision(&id),
                Command::DeleteRipple {
                    node: node.clone(),
                    timing: timing(&id),
                },
            )?,
        });
    }
    Ok(steps)
}
fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn union_copy_cut_paste_reopens_with_one_logical_mark_and_exact_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("node-set.deadpan");
    let fixture = fixture()?;
    let mut store = ProjectStore::create(&package, &fixture.document)?;
    let initial = store.snapshot()?;
    store.commit(&request(
        &initial,
        "temporary",
        Command::Rename {
            node: node("tail"),
            label: "Temporary".into(),
        },
    ))?;
    store.undo(&revision("temporary"), revision("before-copy"))?;
    let before = store.snapshot()?;
    let availability = store.history_availability()?;
    assert!(availability.1);
    let set = capture(&before, &fixture.members, "copy-set")?;
    assert_eq!(set.duration().frames(), 10);
    assert_eq!(set.components().len(), 4);
    assert_eq!(set.components()[0].parent(), &node("left-group"));
    assert_eq!(set.components()[2].parent(), &node("right-group"));
    assert_eq!(set.identity_requirements()?.marks, 1);
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
    let bank = store.save_register_at(
        before.project_id(),
        before.revision_id(),
        0,
        name('a'),
        value,
    )?;
    assert_eq!(bank, preview);
    assert!(Arc::ptr_eq(
        &bank.entries[&name('a')],
        &bank.entries[&RegisterName::unnamed()]
    ));
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.history_availability()?, availability);
    let database = Connection::open(package.join("project.sqlite"))?;
    assert_eq!(
        database.query_row("SELECT count(*) FROM register_contents", [], |row| row
            .get::<_, i64>(0))?,
        1
    );
    store.redo(before.revision_id(), revision("redone-before-cut"))?;
    assert_eq!(store.registers()?, bank);
    store.undo(&revision("redone-before-cut"), revision("before-cut"))?;
    let before_cut = store.snapshot()?;
    let set = capture(&before_cut, &fixture.members, "cut-set")?;
    let cut = compound(
        &before_cut,
        "cut-union",
        bank.version,
        cut_steps(&set, "delete-node")?,
    )?;
    let unchanged = cells(&package)?;
    let preview = store.preview_compound(&cut)?;
    assert_eq!(cells(&package)?, unchanged);
    let committed = store.commit_compound(&cut, None)?;
    assert!(committed.committed.is_some());
    assert_eq!(committed.register_bank, preview.register_bank);
    let bank = committed.register_bank;
    let cut_document = store.snapshot()?;
    assert_eq!(cut_document.duration()?.frames(), 8);
    for selected in &fixture.members {
        assert!(!cut_document.nodes().contains_key(selected));
    }
    assert_eq!(
        cut_document.nodes()[&node("gap")],
        before_cut.nodes()[&node("gap")]
    );
    assert!(cut_document.marks().is_empty());
    let paste = request(
        &cut_document,
        "paste-union",
        Command::SpliceEditSet {
            parent: cut_document.root().clone(),
            index: 3,
            set: (*set).clone(),
            identities: identities(&set, "pasted")?,
            timing: timing("paste-union"),
        },
    );
    let unchanged = cells(&package)?;
    let _ = store.preview_edit_slice(&paste)?;
    assert_eq!(cells(&package)?, unchanged);
    store.commit(&paste)?;
    let pasted = store.snapshot()?;
    assert_eq!(pasted.duration()?.frames(), 18);
    assert_eq!(pasted.marks().len(), 1);
    let mark = pasted.marks().values().next().ok_or("shared pasted mark")?;
    assert_eq!(mark.label, "One logical mark");
    assert_eq!(mark.loss_policy, AnchorLossPolicy::DeleteOwned);
    assert_eq!(mark.binding_count(), 2);
    let expected_states: Vec<_> = before_cut.marks()[&MarkId::new("shared")?]
        .bindings()
        .map(|binding| binding.state)
        .collect();
    assert_eq!(
        mark.bindings()
            .map(|binding| binding.state)
            .collect::<Vec<_>>(),
        expected_states
    );
    assert_eq!(store.registers()?, bank);
    let report = store.validate_report(true)?;
    assert_eq!(report.verified_by_receipt, 0);
    assert_eq!(report.replayed + 1, report.revisions);
    store.undo(pasted.revision_id(), revision("undo-paste"))?;
    same_authored(&store.snapshot()?, &cut_document)?;
    assert_eq!(store.registers()?, bank);
    store.redo(&revision("undo-paste"), revision("redo-paste"))?;
    same_authored(&store.snapshot()?, &pasted)?;
    store.checkpoint()?;
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    same_authored(&store.snapshot()?, &pasted)?;
    assert_eq!(store.registers()?, bank);
    let report = store.validate_report(true)?;
    assert_eq!(report.verified_by_receipt, 0);
    assert_eq!(report.replayed + 1, report.revisions);
    Ok(())
}

#[test]
fn union_capture_checkpoints_and_late_refusals_preserve_entire_bank_and_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("node-set-checkpoints.deadpan");
    let fixture = fixture()?;
    let mut store = ProjectStore::create(&package, &fixture.document)?;
    let before = store.snapshot()?;
    let unchanged = cells(&package)?;
    for members in [
        vec![node("missing")],
        vec![fixture.left.clone(), fixture.left.clone()],
        vec![node("left-group"), fixture.left.clone()],
        vec![node("root")],
    ] {
        assert!(
            CapturedEditSet::capture(&before, before.root(), &members, timing("bad-capture"))
                .is_err()
        );
        assert_eq!(cells(&package)?, unchanged);
    }
    assert!(
        CapturedEditSet::capture(
            &before,
            &node("left-group"),
            std::slice::from_ref(&fixture.right),
            timing("outside-capture")
        )
        .is_err()
    );
    assert_eq!(cells(&package)?, unchanged);
    let set = capture(&before, &fixture.members, "before-late-failure")?;
    let mut steps = cut_steps(&set, "failed-delete")?;
    steps.push(ResolvedStep::Edit {
        edit: LeafEdit::new(
            revision("missing-delete"),
            Command::DeleteRipple {
                node: node("missing"),
                timing: timing("missing-delete"),
            },
        )?,
    });
    let bad = compound(&before, "failed-union", 0, steps)?;
    assert!(store.preview_compound(&bad).is_err());
    assert!(store.commit_compound(&bad, None).is_err());
    assert_eq!(cells(&package)?, unchanged);
    let leaf = LeafEdit::new(
        revision("renamed-stage"),
        Command::Rename {
            node: node("tail"),
            label: "Renamed in the same transaction".into(),
        },
    )?;
    let staged = apply(&before, &leaf.request(&before))?
        .forward
        .apply(&before)?;
    let set = capture(&staged, &fixture.members, "staged-copy")?;
    let mut steps = vec![ResolvedStep::Edit { edit: leaf }];
    steps.extend(cut_steps(&set, "staged-delete")?);
    let command = compound(&before, "checkpointed-union", 0, steps)?;
    let result = store.commit_compound(&command, None)?;
    let captured = store.capture_snapshot_at(&revision("renamed-stage"))?;
    assert_eq!(captured, staged);
    let RegisterValue::EditedSet { set: retained } =
        result.register_bank.entries[&name('c')].as_ref()
    else {
        panic!("union persisted as one value")
    };
    assert_eq!(retained.as_ref(), set.as_ref());
    for component in retained.components() {
        assert_eq!(component.revision_id(), &revision("renamed-stage"));
        component.validate_capture(&captured)?;
    }
    let after = store.snapshot()?;
    let mut bad_identities = identities(&set, "bad-paste")?;
    bad_identities.marks.clear();
    let bad = request(
        &after,
        "bad-union-paste",
        Command::SpliceEditSet {
            parent: after.root().clone(),
            index: 0,
            set: (*set).clone(),
            identities: bad_identities,
            timing: timing("bad-union-paste"),
        },
    );
    let unchanged = cells(&package)?;
    assert!(store.preview_edit_slice(&bad).is_err());
    assert!(store.commit(&bad).is_err());
    assert_eq!(cells(&package)?, unchanged);
    store.checkpoint()?;
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(
        store.capture_snapshot_at(&revision("renamed-stage"))?,
        staged
    );
    assert_eq!(store.registers()?, result.register_bank);
    let report = store.validate_report(true)?;
    assert_eq!(report.verified_by_receipt, 0);
    assert_eq!(report.replayed + 1, report.revisions);
    Ok(())
}

#[test]
fn union_register_validates_a_later_closed_component_against_its_historical_snapshot() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("node-set-forgery.deadpan");
    let fixture = fixture()?;
    let mut store = ProjectStore::create(&package, &fixture.document)?;
    let before = store.snapshot()?;
    let set = capture(&before, &fixture.members, "honest-set")?;
    let mut wire = serde_json::to_value(&*set)?;
    wire["components"][2]["nodes"][fixture.right.as_str()]["label"] = json!("Forged later capsule");
    // The capsule is independently well-formed. Only comparison with the
    // immutable capture document can establish that its authored data is false.
    let forged: CapturedEditSet = serde_json::from_value(wire)?;
    assert!(forged.validate_capture(&before).is_err());
    let value = RegisterValue::EditedSet {
        set: Arc::new(forged),
    };
    let unchanged = cells(&package)?;
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
    assert!(store.registers()?.entries.is_empty());
    Ok(())
}

#[test]
fn equal_time_empty_union_cuts_and_pastes_both_structural_identities() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("empty-node-set.deadpan");
    let fixture = fixture()?;
    let initial = insert(
        fixture.document,
        "left-group",
        1,
        "empty-c",
        BeatNode::sequence("Empty C", vec![]),
    )?;
    let mut store = ProjectStore::create(&package, &initial)?;
    let selected = [node("empty-a"), node("empty-c")];
    let set = capture(&initial, &selected, "empty-union-capture")?;
    assert_eq!(set.duration(), FrameDuration::ZERO);
    assert_eq!(set.components().len(), 2);
    for (component, expected) in set.components().iter().zip(["empty-a", "empty-c"]) {
        assert_eq!(
            component.selection(),
            &SliceCaptureSelection::Child {
                node: node(expected)
            }
        );
        assert_eq!(
            component.range(),
            FrameRange::new(ProjectFrame(3), ProjectFrame(3))?
        );
    }
    let command = compound(
        &initial,
        "empty-union-cut",
        0,
        cut_steps(&set, "empty-delete")?,
    )?;
    let result = store.commit_compound(&command, None)?;
    assert!(
        result.committed.is_some(),
        "zero time still changes ownership"
    );
    let cut = store.snapshot()?;
    assert_eq!(cut.duration()?, initial.duration()?);
    assert_eq!(cut.nodes().len() + 2, initial.nodes().len());
    assert!(!cut.nodes().contains_key(&node("empty-a")));
    assert!(!cut.nodes().contains_key(&node("empty-c")));
    assert!(cut.nodes().contains_key(&node("empty-b")));
    let paste = request(
        &cut,
        "empty-union-paste",
        Command::SpliceEditSet {
            parent: cut.root().clone(),
            index: 0,
            set: (*set).clone(),
            identities: identities(&set, "pasted-empty")?,
            timing: timing("empty-union-paste"),
        },
    );
    store.commit(&paste)?;
    let pasted = store.snapshot()?;
    assert_eq!(pasted.duration()?, initial.duration()?);
    assert_eq!(children(&pasted, "pasted-empty-group").len(), 2);
    let component_roots = children(&pasted, "pasted-empty-group");
    for (root, original) in component_roots.iter().zip(["empty-a", "empty-c"]) {
        // Ordinary slice placement retains its neutral containing Sequence;
        // the selected empty authored node remains a distinct child inside it.
        let retained = children(&pasted, root.as_str());
        assert_eq!(retained.len(), 1);
        assert_ne!(retained[0], node(original));
        assert_eq!(
            pasted.nodes()[&retained[0]],
            initial.nodes()[&node(original)]
        );
        assert!(children(&pasted, retained[0].as_str()).is_empty());
    }
    store.undo(pasted.revision_id(), revision("empty-union-undo"))?;
    same_authored(&store.snapshot()?, &cut)?;
    store.redo(&revision("empty-union-undo"), revision("empty-union-redo"))?;
    same_authored(&store.snapshot()?, &pasted)?;
    assert_eq!(store.registers()?, result.register_bank);
    store.checkpoint()?;
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    same_authored(&store.snapshot()?, &pasted)?;
    assert_eq!(store.registers()?, result.register_bank);
    let report = store.validate_report(true)?;
    assert_eq!(report.verified_by_receipt, 0);
    assert_eq!(report.replayed + 1, report.revisions);
    Ok(())
}
