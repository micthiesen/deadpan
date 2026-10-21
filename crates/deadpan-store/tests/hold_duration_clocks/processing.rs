//! Metadata-only history proof; decoded input and DSP have separate PCM tests.
use super::*;

fn processing_fixture(pitch: PitchPolicy, output: i64, prebound: bool) -> Result<ProjectDocument> {
    let mut wire = serde_json::to_value(fixed_retime_fixture()?)?;
    wire["nodes"]["rate"]["kind"]["duration"] = output.into();
    wire["nodes"]["rate"]["kind"]["pitch"] = serde_json::to_value(pitch)?;
    let mut before = ProjectDocument::from_json(&wire.to_string())?;
    if prebound {
        let bindings = capture_unbound_audio_bindings(
            &before,
            AudioTimingId {
                allocation: revision("processing-old-clock"),
                ordinal: 0,
            },
        )?;
        wire = serde_json::to_value(&before)?;
        wire["audio_bindings"] = serde_json::to_value(bindings)?;
        before = ProjectDocument::from_json(&wire.to_string())?;
    }
    Ok(before)
}

#[test]
fn processing_hold_duration_clocks_keep_point_scopes_atomic_history_and_unchanged_outputs() -> Result
{
    for (label, pitch, output) in [
        ("preserve", PitchPolicy::Preserve, 2),
        ("shift", PitchPolicy::Shift { semitones: 3 }, 4),
    ] {
        for prebound in [false, true] {
            let scratch = tempfile::tempdir()?;
            let package = scratch.path().join(format!("{label}-{prebound}.deadpan"));
            let initial = processing_fixture(pitch, output, prebound)?;
            let mut store = ProjectStore::create(&package, &initial)?;
            store.commit(&request(&initial, "processing-noop", resize(2)))?;
            let before = store.snapshot()?;
            assert_eq!(before.audio_bindings(), initial.audio_bindings());
            assert_eq!(before.duration()?.frames(), 5 + output);
            let edit = request(&before, "processing-grow", resize(3));
            let preview = store.preview(&edit)?;
            assert_eq!(store.snapshot()?, before);
            let committed = store.commit(&edit)?.edit;
            assert_eq!(committed, preview);
            assert_eq!(committed.duration_delta, 0);
            let after = store.snapshot()?;
            assert_eq!(after.duration()?, before.duration()?);
            for owner in ["rate", "speech", "outside"] {
                assert_eq!(after.nodes()[&node(owner)], before.nodes()[&node(owner)]);
            }
            let speech = &after.audio_bindings().bindings()[&node("speech")];
            assert_eq!(
                speech.lattice.reference.root,
                AudioClockRoot::PreserveInputPointCeil {
                    stage: node("rate")
                }
            );
            // A changing operand does not move its processor's output or the
            // outer suffix. Fresh inventory may capture their initial lattice.
            for owner in ["rate", "outside"] {
                let current = &after.audio_bindings().bindings()[&node(owner)];
                if prebound {
                    assert_eq!(current, &before.audio_bindings().bindings()[&node(owner)]);
                } else {
                    assert!(current.reanchors.is_empty());
                }
            }
            if prebound {
                let previous = &before.audio_bindings().bindings()[&node("speech")];
                assert_eq!(speech.lattice, previous.lattice);
                assert_eq!(speech.resume, previous.resume);
                assert!(speech.reanchors.starts_with(&previous.reanchors));
            }
            assert_eq!(committed.inverse.apply(&after)?, before);
            store.validate()?;
            drop(store);
            let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
            assert_eq!(store.snapshot()?, after);
            store.undo(after.revision_id(), revision("processing-undo"))?;
            let undone = store.snapshot()?;
            assert_eq!(undone.nodes(), before.nodes());
            assert_eq!(undone.audio_bindings(), before.audio_bindings());
            store.redo(undone.revision_id(), revision("processing-redo"))?;
            let redone = store.snapshot()?;
            assert_eq!(redone.nodes(), after.nodes());
            assert_eq!(redone.audio_bindings(), after.audio_bindings());
            let bank = store.registers()?;
            let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
            let counts = || -> Result<(i64, i64, i64)> {
                Ok(database.query_row(
                    "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM transaction_steps)",
                    [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?)
            };
            let baseline = counts()?;
            assert!(
                store
                    .commit(&request(&redone, "processing-invalid", resize(1)))
                    .is_err()
            );
            // A changed valid first leaf must admit before a missing second
            // target proves the complete Compound rolls back atomically.
            store.preview(&request(&redone, "processing-staged-grow", resize(4)))?;
            let late = Command::Compound {
                transaction: ResolvedTransaction::new(
                    bank.version,
                    BTreeMap::new(),
                    vec![
                        ResolvedStep::Edit {
                            edit: LeafEdit::new(revision("processing-staged-grow"), resize(4))?,
                        },
                        ResolvedStep::Edit {
                            edit: LeafEdit::new(
                                revision("processing-missing"),
                                Command::Rename {
                                    node: node("missing"),
                                    label: "Cannot resolve".into(),
                                },
                            )?,
                        },
                    ],
                )?,
            };
            let error = store
                .commit(&request(&redone, "processing-late-refusal", late))
                .expect_err("the missing second target must reject the complete transaction");
            assert_eq!(error.code(), "SelectionUnavailable");
            assert_eq!(store.snapshot()?, redone);
            assert_eq!(store.registers()?, bank);
            assert_eq!(counts()?, baseline);
            store.validate()?;
            drop(store);
            assert_eq!(
                ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
                redone
            );
        }
    }
    Ok(())
}
