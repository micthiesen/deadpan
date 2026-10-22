use super::*;
use deadpan_core::{
    BeatNode, FrameDuration, FrameRange, HoldAudio, HoldRecipe, HoldVideo, ProjectFrame,
    SemanticNodeSetMember, SemanticTrace, Subtree,
};

fn node(id: &str) -> NodeId {
    NodeId::new(id).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

struct Fixture {
    _directory: tempfile::TempDir,
    store: ProjectStore,
    prepared: deadpan_cli::macros::Prepared,
    id: Id,
    runtime: Bank,
}
impl Fixture {
    fn prepare(selector: SemanticSelector) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let document = ProjectDocument::new_automatic(
            ProjectId::new("union-provenance").unwrap(),
            RevisionId::new("initial").unwrap(),
            node("root"),
        )
        .unwrap();
        let mut store =
            ProjectStore::create(&directory.path().join("union.deadpan"), &document).unwrap();
        for (index, name) in ["a", "b", "c"].into_iter().enumerate() {
            let current = store.snapshot().unwrap();
            store
                .commit(&CommandRequest {
                    project_id: current.project_id().clone(),
                    expected_revision: current.revision_id().clone(),
                    new_revision: RevisionId::new(format!("insert-{name}")).unwrap(),
                    command: Command::Insert {
                        parent: node("root"),
                        index,
                        subtree: Subtree {
                            root: node(name),
                            nodes: BTreeMap::from([(
                                node(name),
                                BeatNode::hold(
                                    name,
                                    HoldRecipe {
                                        picture_context: None,
                                        duration: FrameDuration::new(10).unwrap(),
                                        video: HoldVideo::Background,
                                        audio: HoldAudio::Silence,
                                    },
                                ),
                            )]),
                            overrides: BTreeMap::new(),
                            gap_overrides: BTreeMap::new(),
                        },
                    },
                })
                .unwrap();
        }
        let document = store.snapshot().unwrap();
        let stored = store.registers().unwrap();
        let request = deadpan_cli::macros::Request {
            protocol: deadpan_cli::macros::PROTOCOL_VERSION,
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            expected_bank_version: stored.version,
            dry_run: false,
            operation: deadpan_cli::macros::Operation::Apply {
                program: Arc::new(
                    SemanticProgram::new(vec![SemanticInstruction::Yank {
                        selector,
                        register: RegisterName::unnamed(),
                    }])
                    .unwrap(),
                ),
                parent: node("root"),
                cursor: ProjectFrame(10),
                selected_child: Some(node("b")),
                visual_selection: None,
                original: None,
                scoped: None,
                new_revision: None,
            },
        };
        let prepared = deadpan_cli::macros::prepare(&store, &request).unwrap();
        let id = Id {
            transcript_guard: None,
            session: 1,
            project: document.project_id().clone(),
            revision: document.revision_id().clone(),
            bank_version: stored.version,
            request: 1,
        };
        let runtime = Bank {
            session: 1,
            project: id.project.clone(),
            version: stored.version,
            entries: BTreeMap::new(),
        };
        Self {
            _directory: directory,
            store,
            prepared,
            id,
            runtime,
        }
    }
    fn ranges() -> Self {
        use deadpan_core::{SemanticFrameOffset as Offset, SemanticRangeOffsets as Pair};
        Self::prepare(SemanticSelector::Ranges {
            ranges: vec![
                Pair {
                    start: Offset(-8),
                    end: Offset(-4),
                },
                Pair {
                    start: Offset(2),
                    end: Offset(6),
                },
                Pair {
                    start: Offset(12),
                    end: Offset(16),
                },
            ],
        })
    }
    fn bank(&self) -> Result<Bank> {
        prepare_runtime_bank(
            &self.id,
            &self.runtime,
            &self.prepared.bank,
            &self.prepared.final_bank,
            self.prepared.plan.as_ref().unwrap(),
        )
    }
}

#[test]
fn edited_union_runtime_preparation_accepts_real_range_and_whole_child_captures() {
    for fixture in [
        Fixture::ranges(),
        Fixture::prepare(SemanticSelector::Nodes {
            nodes: vec![node("a"), node("c")],
        }),
    ] {
        let before = fixture.store.snapshot().unwrap();
        let before_bank = fixture.store.registers().unwrap();
        let runtime = fixture.bank().unwrap();
        let expected = fixture
            .prepared
            .final_bank
            .entries
            .get(&RegisterName::unnamed())
            .unwrap();
        assert!(
            matches!((runtime.entries.get(&'"'),expected.as_ref()),(Some(Value::EditedSet(actual)),RegisterValue::EditedSet {set}) if actual==set)
        );
        assert_eq!(runtime.version, fixture.prepared.final_bank.version);
        assert_eq!(fixture.store.snapshot().unwrap(), before);
        assert_eq!(fixture.store.registers().unwrap(), before_bank);
    }
}

#[test]
fn range_union_runtime_preparation_rejects_tampered_trace_geometry_mode_and_metadata() {
    let mut fixture = Fixture::ranges();
    let before = fixture.store.snapshot().unwrap();
    let before_bank = fixture.store.registers().unwrap();
    let original = fixture.prepared.plan.as_ref().unwrap().trace.clone();
    type TraceChange = (&'static str, fn(&mut SemanticTrace));
    let changes: &[TraceChange] = &[
        ("range order", |trace| trace.resolved_ranges.reverse()),
        ("range endpoint", |trace| {
            trace.resolved_ranges[0] = range(2, 7)
        }),
        ("whole-child mode", |trace| {
            trace.resolved_nodes = vec![SemanticNodeSetMember {
                node: node("a"),
                parent: node("root"),
                range: range(2, 6),
                navigation_child: node("a"),
            }]
        }),
        ("capture revision", |trace| {
            trace.before_revision = RevisionId::new("different").unwrap()
        }),
        ("scope parent", |trace| {
            trace.capture_set.as_mut().unwrap().parent = node("other")
        }),
        ("entry parent", |trace| trace.before.parent = node("other")),
        ("capture timing", |trace| {
            trace.capture_set.as_mut().unwrap().timing.ordinal += 1
        }),
        ("capture bounds", |trace| {
            trace.capture_set.as_mut().unwrap().bounds = range(0, 29)
        }),
        ("entry bounds", |trace| trace.before_scope = range(0, 29)),
        ("scope metadata", |trace| {
            trace
                .capture_set
                .as_mut()
                .unwrap()
                .scope_labels
                .push("forged".into())
        }),
        ("scope path", |trace| {
            let capture = trace.capture_set.as_mut().unwrap();
            capture.scope.push(node("wrong"));
            capture.scope_labels.push("wrong".into());
        }),
        ("scalar capture", |trace| {
            trace.capture = trace.capture_set.clone()
        }),
        ("scalar range", |trace| {
            trace.resolved_range = Some(range(2, 26))
        }),
        ("scalar parent", |trace| {
            trace.resolved_parent = Some(node("root"))
        }),
        ("scalar selection", |trace| {
            trace.resolved_selection = Some(SliceCaptureSelection::Range {
                range: range(2, 26),
            })
        }),
        ("replacement metadata", |trace| {
            trace.removed_range = Some(range(2, 26))
        }),
        ("child metadata", |trace| {
            trace.captured_child_label = Some("forged".into())
        }),
    ];
    for (name, change) in changes {
        let plan = fixture.prepared.plan.as_mut().unwrap();
        plan.plan.trace = original.clone();
        change(&mut plan.plan.trace[0]);
        assert!(fixture.bank().is_err(), "accepted tampered {name}");
        assert_eq!(fixture.store.snapshot().unwrap(), before, "{name}");
        assert_eq!(fixture.store.registers().unwrap(), before_bank, "{name}");
    }
}
