//! Real qualification is rechecked inside nested semantic isolation transactions.
use super::*;
use deadpan_core::{
    MarkId, RepeatEditBranch, RepeatEditStep, RepeatInstance, ScopedNodeTarget, SemanticAllocation,
    SemanticAllocationRequest, SemanticContext, SemanticInstruction, SemanticPlan, SemanticProgram,
    SemanticRegisterBank, SemanticScopedContext, plan_semantic,
};

fn nested_context(document: &ProjectDocument) -> Result<SemanticContext> {
    let repeats = ["outer", "inner"]
        .into_iter()
        .map(|name| {
            let node = NodeId::new(name)?;
            let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node].kind else {
                panic!("nested Repeat fixture")
            };
            Ok(RepeatInstance {
                node,
                iteration: iterations.at(1).unwrap(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(SemanticContext {
        hold_duration: None,
        original: None,
        parent: document.root().clone(),
        cursor: ProjectFrame(12),
        selected_child: Some(NodeId::new("outer")?),
        visual_selection: None,
        scoped: Some(SemanticScopedContext {
            root: NodeId::new("outer")?,
            target: ScopedNodeTarget {
                node: NodeId::new("hold")?,
                repeats: repeats
                    .iter()
                    .map(|step| RepeatEditStep {
                        repeat: step.node.clone(),
                        branch: RepeatEditBranch::Play {
                            iteration: step.iteration.clone(),
                        },
                    })
                    .collect(),
            },
            presentation: InstancePath {
                node: NodeId::new("hold")?,
                repeats,
            },
        }),
    })
}

fn plan(
    store: &ProjectStore,
    context: &SemanticContext,
    policies: Vec<HoldAudio>,
    name: &str,
) -> Result<SemanticPlan> {
    let document = store.snapshot()?;
    let bank = store.registers()?;
    Ok(plan_semantic(
        &document,
        context,
        &SemanticProgram::new(
            policies
                .into_iter()
                .map(|audio| SemanticInstruction::SetHoldAudio { audio })
                .collect(),
        )?,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        revision(&format!("{name}-outer")),
        |request| {
            let SemanticAllocationRequest::Isolation {
                step_index,
                nodes,
                marks,
            } = request
            else {
                panic!("Hold audio needs only isolation identities: {request:?}")
            };
            Ok(SemanticAllocation::Isolation {
                new_revision: revision(&format!("{name}-step-{step_index}")),
                identities: OccurrenceIdentities {
                    nodes: (0..nodes)
                        .map(|n| NodeId::new(format!("{name}-node-{step_index}-{n}")))
                        .collect::<std::result::Result<_, _>>()?,
                    marks: (0..marks)
                        .map(|n| MarkId::new(format!("{name}-mark-{step_index}-{n}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
            })
        },
        |_, _| panic!("literal policy already carries its exact qualified source span"),
    )?)
}

fn step_count(database: &Connection) -> Result<i64> {
    Ok(
        database.query_row("SELECT count(*) FROM transaction_steps", [], |row| {
            row.get(0)
        })?,
    )
}

#[test]
fn nested_semantic_hold_audio_rechecks_exact_qualified_samples_atomically_and_reopens_undo_redo()
-> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let registration = request(&store, &original, "import", "camera", Some("clip"))?;
    store.register_source(&registration, &decoded, None, limits(), &active())?;
    insert_hold(&mut store, HoldAudio::Silence)?;
    // Start with a genuinely different, admitted room-tone span so the
    // later Silence -> RoomTone program has two changing semantic steps.
    let mut initial_source = source(&store.snapshot()?)?;
    let end = initial_source.span.end();
    initial_source.span = SourceSpan::new(
        initial_source.span.start(),
        SourceTimestamp {
            ticks: end.ticks - 1,
            time_base: end.time_base,
        },
    )?;
    store.commit(&edit(
        &store.snapshot()?,
        "initial-room",
        setter(
            HoldAudio::RoomTone {
                source: initial_source,
            },
            false,
        )?,
    ))?;
    for (child, owner, gap) in [
        (
            "hold",
            "inner",
            Some(HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            }),
        ),
        ("inner", "outer", None),
    ] {
        store.commit(&edit(
            &store.snapshot()?,
            &format!("wrap-{owner}"),
            Command::WrapRepeat {
                node: NodeId::new(child)?,
                id: NodeId::new(owner)?,
                plays: 2,
                gap,
                anchor_policy: Default::default(),
            },
        ))?;
    }
    // Hold intervals are [0,3), [4,7), [7,10), [11,14).
    // The selected interior frame 12 must remain absolute through both isolates.
    let before = store.snapshot()?;
    let context = nested_context(&before)?;
    let selected = source(&before)?;
    let room = HoldAudio::RoomTone {
        source: selected.clone(),
    };
    let planned = plan(&store, &context, vec![room.clone()], "nested-room")?;
    let request = planned.request.as_ref().unwrap();
    let preview = store.preview_compound(request)?;
    let bank = store.registers()?;
    let baseline_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let baseline_steps = step_count(&database)?;
    let unchanged = |store: &ProjectStore| -> Result {
        assert_eq!(store.snapshot()?, before);
        assert_eq!(store.registers()?, bank);
        assert_eq!(counts(&path)?, baseline_counts);
        assert_eq!(step_count(&database)?, baseline_steps);
        Ok(())
    };
    unchanged(&store)?;
    let (receipt, original_ref): (Vec<u8>, String) = database.query_row(
        "SELECT snapshot,original_ref FROM source_qualifications",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    // Silence reads no qualified source, so the first staged edit remains valid
    // when the second policy's receipt is damaged. The RoomTone span itself
    // is admissible and exact: the selected Original samples 17..113 above.
    let late = plan(
        &store,
        &context,
        vec![HoldAudio::Silence, room.clone()],
        "late-policy",
    )?;
    let late_request = late.request.as_ref().unwrap();
    let Command::Compound { transaction } = &late_request.command else {
        panic!("semantic policies must use a Compound")
    };
    assert_eq!(transaction.steps().len(), 2);
    store.preview_compound(late_request)?;
    unchanged(&store)?;
    for tamper in [
        "UPDATE source_qualifications SET snapshot=X'00'",
        "UPDATE source_qualifications SET original_ref=json_set(original_ref,'$.byte_length',1234)",
    ] {
        database.execute(tamper, [])?;
        // Prove the first changed step still admits on its own: only the
        // second step needs the damaged source qualification.
        store.preview(&transaction.steps()[0].edit().unwrap().request(&before))?;
        assert!(matches!(
            store.preview_compound(late_request),
            Err(StoreError::SourceRegistration(_))
        ));
        assert!(matches!(
            store.commit_compound(late_request, None),
            Err(StoreError::SourceRegistration(_))
        ));
        unchanged(&store)?;
        database.execute(
            "UPDATE source_qualifications SET snapshot=?1,original_ref=?2",
            rusqlite::params![receipt, original_ref],
        )?;
    }
    let mut stale = request.clone();
    stale.expected_revision = revision("stale-before-isolation");
    assert!(store.preview_compound(&stale).is_err());
    assert!(store.commit_compound(&stale, None).is_err());
    unchanged(&store)?;

    let full = before.assets()[&id("camera")].audio.unwrap();
    // Qualification derives the asset span in the native sample clock, and
    // core requires a selection to retain that exact time base. A fractional
    // sample cannot reach store admission for this qualified asset.
    let sample_rate = decoded.snapshot().audio().unwrap().stream().sample_rate;
    assert_eq!(full.start().time_base, SourceTimeBase::new(1, sample_rate)?);
    let half_sample = SourceTimeBase::new(1, full.start().time_base.denominator() * 2)?;
    for span in [
        SourceSpan::new(
            SourceTimestamp {
                ticks: full.start().ticks - 1,
                time_base: full.start().time_base,
            },
            full.end(),
        )?,
        SourceSpan::new(
            SourceTimestamp {
                ticks: full.start().ticks * 2 + 1,
                time_base: half_sample,
            },
            SourceTimestamp {
                ticks: full.start().ticks * 2 + 101,
                time_base: half_sample,
            },
        )?,
    ] {
        let invalid = HoldAudio::RoomTone {
            source: SourceAudio {
                asset: id("camera"),
                span,
            },
        };
        assert!(
            plan(
                &store,
                &context,
                vec![room.clone(), invalid],
                "invalid-span"
            )
            .is_err()
        );
        unchanged(&store)?;
    }

    let result = store.commit_compound(request, None)?;
    assert_eq!(result.register_bank, preview.register_bank);
    let transaction = result.committed.unwrap().edit;
    assert_eq!(Some(transaction.clone()), preview.edit);
    assert_eq!(transaction.duration_delta, 0);
    let toned = store.snapshot()?;
    assert_eq!(toned, planned.document);
    assert_eq!(transaction.inverse.apply(&toned)?, before);
    assert_eq!(toned.duration()?, before.duration()?);
    assert_eq!(
        toned.nodes()[&NodeId::new("clip")?],
        before.nodes()[&NodeId::new("clip")?]
    );
    assert_eq!(
        toned.nodes()[&NodeId::new("hold")?],
        before.nodes()[&NodeId::new("hold")?]
    );
    assert_eq!(planned.context.cursor, context.cursor);
    assert_eq!(planned.context.parent, context.parent);
    assert_eq!(planned.context.selected_child, context.selected_child);
    let mapped = planned.context.scoped.as_ref().unwrap();
    assert_ne!(mapped.target.node, NodeId::new("hold")?);
    assert!(
        mapped
            .target
            .matches_instance(&toned, &mapped.presentation)?
    );
    let NodeKind::Hold { recipe } = &toned.nodes()[&mapped.target.node].kind else {
        panic!()
    };
    assert_eq!(recipe.audio, room);
    assert_eq!(store.registers()?, bank);
    assert_eq!(counts(&path)?.1, baseline_counts.1 + 1);
    store.validate()?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, toned);
    let silence = plan(
        &store,
        &planned.context,
        vec![HoldAudio::Silence],
        "nested-silence",
    )?;
    store.commit_compound(silence.request.as_ref().unwrap(), None)?;
    let silent = store.snapshot()?;
    assert_eq!(silent.audio_bindings(), toned.audio_bindings());
    assert_eq!(silent.duration()?, before.duration()?);
    let target = &silence.context.scoped.as_ref().unwrap().target.node;
    let NodeKind::Hold { recipe } = &silent.nodes()[target].kind else {
        panic!()
    };
    assert_eq!(recipe.audio, HoldAudio::Silence);
    store.undo(silent.revision_id(), revision("undo-nested-silence"))?;
    assert_eq!(store.snapshot()?.nodes(), toned.nodes());
    assert_eq!(store.snapshot()?.audio_bindings(), toned.audio_bindings());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        revision("redo-nested-silence"),
    )?;
    assert_eq!(store.snapshot()?.nodes(), silent.nodes());
    assert_eq!(store.snapshot()?.audio_bindings(), silent.audio_bindings());
    assert_eq!(store.registers()?, bank);
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}
