//! Metadata-only proof of exact speed updates and their durable binding history.
//! Actual decoded Source PCM and emitted media are qualified separately.
use super::*;

fn speed_fixture() -> Result<ProjectDocument> {
    let initial = fixture()?;
    let mut speech = initial.nodes()[&node("speech")].clone();
    let NodeKind::Source { source } = &mut speech.kind else {
        unreachable!("parent metadata fixture owns a Source")
    };
    source.duration = frames(12);
    let slow = BeatNode {
        kind: NodeKind::Retime {
            child: node("speech"),
            duration: frames(24),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(12))?,
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        },
        ..BeatNode::sequence("Tape speed", vec![])
    };
    let mut wire = serde_json::to_value(&initial)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("slow"), node("outside")]),
        ),
        (node("lead"), hold(2)),
        (node("slow"), slow),
        (node("speech"), speech),
        (node("outside"), initial.nodes()[&node("speech")].clone()),
    ]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn pause(document: &ProjectDocument) -> CommandRequest {
    let NodeKind::Hold { recipe } = hold(3).kind else {
        unreachable!()
    };
    request(
        document,
        "speed-pause",
        Command::InsertTime {
            at: ProjectFrame(5),
            hold: recipe,
            id: node("inserted-pause"),
            identities: SplitIdentities {
                nodes: (0..5)
                    .map(|index| node(&format!("split-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("speed-pause"),
                ordinal: 0,
            },
        },
    )
}

fn speed(output: i64) -> Command {
    Command::SetRetime {
        node: node("slow"),
        duration: frames(output),
        pitch: PitchPolicy::FollowSpeed,
    }
}

fn resolved_outside(document: &ProjectDocument) -> Result<ResolvedAudioBinding> {
    Ok(document.audio_bindings().resolve(
        &node("outside"),
        &InstancePath {
            node: node("outside"),
            repeats: vec![],
        },
        MAX_DOCUMENT_NODES,
    )?)
}

fn outside_origin(document: &ProjectDocument) -> Result<ExactRatio> {
    Ok(FrozenAudioLayout::capture(document)?
        .project(
            &InstancePath {
                node: node("outside"),
                repeats: vec![],
            },
            ExactRatio::ZERO,
            None,
            MAX_DOCUMENT_NODES,
        )?
        .origin)
}

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut wire = serde_json::to_value(actual)?;
    wire["revision_id"] = serde_json::to_value(expected.revision_id())?;
    assert_eq!(wire, serde_json::to_value(expected)?);
    Ok(())
}

#[test]
fn clocked_speed_update_keeps_exact_input_preview_and_durable_atomic_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("speed-update.deadpan");
    let initial = speed_fixture()?;
    assert_eq!(initial.duration()?.frames(), 28);
    let mut store = ProjectStore::create(&package, &initial)?;
    store.commit(&pause(&initial))?;
    let before = store.snapshot()?;
    assert_eq!(
        before.exact_durations()?[before.root()],
        ExactRatio::integer(31)
    );
    assert_eq!(
        before.derived_clocks()[&node("slow")].child_end,
        ExactRatio::new(27, 2)?
    );
    let bank = store.registers()?;
    let update = request(&before, "speed-update", speed(15));
    let preview = store.preview(&update)?;
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.registers()?, bank);
    let committed = store.commit(&update)?.edit;
    assert_eq!(committed, preview);
    assert_eq!(committed.duration_delta, -12);
    let after = store.snapshot()?;
    assert_eq!(
        after.exact_durations()?[after.root()],
        ExactRatio::integer(19)
    );
    let clock = &after.derived_clocks()[&node("slow")];
    assert_eq!(
        (clock.extent, clock.child_start, clock.child_end),
        (
            ExactRatio::integer(15),
            ExactRatio::ZERO,
            ExactRatio::new(27, 2)?
        )
    );
    for (owner, previous) in before.derived_clocks() {
        if owner != &node("slow") {
            assert_eq!(after.derived_clocks().get(owner), Some(previous));
        }
    }
    for owner in ["speech", "split-2"] {
        let previous = &before.audio_bindings().bindings()[&node(owner)];
        assert_eq!(&after.audio_bindings().bindings()[&node(owner)], previous);
    }
    assert_eq!(
        after.nodes()[&node("inserted-pause")],
        before.nodes()[&node("inserted-pause")]
    );
    let old_outside = &before.audio_bindings().bindings()[&node("outside")];
    let new_outside = &after.audio_bindings().bindings()[&node("outside")];
    assert_eq!(outside_origin(&before)?, ExactRatio::integer(29));
    assert_eq!(outside_origin(&after)?, ExactRatio::integer(17));
    assert_eq!(new_outside.lattice, old_outside.lattice);
    assert_eq!(new_outside.resume, old_outside.resume);
    assert!(new_outside.reanchors.starts_with(&old_outside.reanchors));
    let old_phase = resolved_outside(&before)?;
    let new_phase = resolved_outside(&after)?;
    assert_eq!(new_phase.lattice, old_phase.lattice);
    assert_eq!(new_phase.resume, old_phase.resume);
    assert_eq!(committed.inverse.apply(&after)?, before);
    assert_eq!(store.registers()?, bank);
    store.validate()?;
    drop(store);

    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("speed-undo"))?;
    let undone = store.snapshot()?;
    same_authored(&undone, &before)?;
    store.redo(undone.revision_id(), revision("speed-redo"))?;
    let redone = store.snapshot()?;
    same_authored(&redone, &after)?;
    assert_eq!(store.registers()?, bank);
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let counts = || -> Result<(i64, i64, i64)> {
        Ok(database.query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM transaction_steps)",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?)
    };
    let baseline = counts()?;
    let stale = request(&before, "speed-stale", speed(17));
    assert_eq!(
        store.preview(&stale).unwrap_err().code(),
        "RevisionConflict"
    );
    assert_eq!(store.commit(&stale).unwrap_err().code(), "RevisionConflict");
    assert_eq!(store.snapshot()?, redone);
    assert_eq!(store.registers()?, bank);
    assert_eq!(counts()?, baseline);
    let valid_first = request(&redone, "speed-late-first", speed(17));
    assert_ne!(store.preview(&valid_first)?.duration_delta, 0);
    let late = Command::Compound {
        transaction: ResolvedTransaction::new(
            bank.version,
            BTreeMap::new(),
            vec![
                ResolvedStep::Edit {
                    edit: LeafEdit::new(revision("speed-late-first"), speed(17))?,
                },
                ResolvedStep::Edit {
                    edit: LeafEdit::new(
                        revision("speed-late-missing"),
                        Command::Rename {
                            node: node("missing"),
                            label: "Cannot resolve".into(),
                        },
                    )?,
                },
            ],
        )?,
    };
    let failed = request(&redone, "speed-late-outer", late);
    assert_eq!(
        store.preview(&failed).unwrap_err().code(),
        "SelectionUnavailable"
    );
    assert_eq!(
        store.commit(&failed).unwrap_err().code(),
        "SelectionUnavailable"
    );
    assert_eq!(store.snapshot()?, redone);
    assert_eq!(store.registers()?, bank);
    assert_eq!(counts()?, baseline);
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}

#[test]
fn clocked_speed_update_persists_a_nontrivial_outside_entry_reanchor() -> Result {
    let initial = speed_fixture()?;
    let paused = apply(&initial, &pause(&initial))?.forward.apply(&initial)?;
    // Explicit metadata control: the outside owner resumes at a non-entry
    // boundary. This requires transport rather than an inert-step elision.
    let mut wire = serde_json::to_value(paused)?;
    wire["audio_bindings"]["bindings"]["outside"]["resume"] = serde_json::to_value(AudioResume {
        local_boundary: ExactRatio::new(1, 2)?,
        phase: AudioLocalPhase {
            constant: ExactRatio::new(1, 3)?,
            terms: vec![],
        },
    })?;
    let before = ProjectDocument::from_json(&wire.to_string())?;
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("speed-resumed-suffix.deadpan");
    let mut store = ProjectStore::create(&package, &before)?;
    let update = request(&before, "speed-resumed", speed(15));
    let preview = store.preview(&update)?;
    assert_eq!(store.commit(&update)?.edit, preview);
    let after = store.snapshot()?;
    let old = &before.audio_bindings().bindings()[&node("outside")];
    let new = &after.audio_bindings().bindings()[&node("outside")];
    assert_eq!(new.lattice, old.lattice);
    assert_eq!(new.resume, old.resume);
    assert!(new.reanchors.starts_with(&old.reanchors));
    assert_eq!(new.reanchors.len(), old.reanchors.len() + 1);
    assert_eq!(
        new.reanchors
            .last()
            .unwrap()
            .placement
            .reference
            .timing
            .allocation,
        revision("speed-resumed")
    );
    assert_eq!(
        resolved_outside(&after)?.resume.unwrap().local_boundary,
        ExactRatio::ZERO
    );
    store.validate()?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("resumed-undo"))?;
    same_authored(&store.snapshot()?, &before)?;
    let undone = store.snapshot()?;
    store.redo(undone.revision_id(), revision("resumed-redo"))?;
    same_authored(&store.snapshot()?, &after)?;
    Ok(())
}
