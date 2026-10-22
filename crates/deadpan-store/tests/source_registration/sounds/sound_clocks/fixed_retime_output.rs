//! Real qualified output-owned sound through a changed fixed Retime operand.
use super::*;
use deadpan_core::{
    BeatNode, ExactRatio, FrameRange, LinkRelation, PitchPolicy, RetimePurpose, SourceNode,
    SourceTimeBase, SourceVideo, SourceVideoMapping, Subtree,
};
use deadpan_media::audio_index::AudioLayoutInterpretation;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn span(start: i64, end: i64, rate: u32) -> Result<SourceSpan> {
    let time_base = SourceTimeBase::new(1, rate)?;
    Ok(SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )?)
}

fn register_event(store: &mut ProjectStore) -> Result {
    let wav = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav")
        .canonicalize()?;
    let original = store
        .retain_original(&wav, OriginalOwnership::Managed, limits(), &active())?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits(), &active())?;
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active(),
    )?;
    let audio = AudioSession::open_input(input, 0, AudioSessionLimits::default(), &active())?;
    assert_eq!(audio.index().stream().sample_rate, 44100);
    let decoded = DecodedSourceQualification::for_registration(
        None,
        Some(&audio),
        Some(AudioLayoutInterpretation::Mono),
    )?;
    store.register_source(
        &request(store, &original, "register-event", "event", None)?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    Ok(())
}

fn setup(parent: &Path, prior_journal: bool) -> Result<(PathBuf, ProjectStore)> {
    let (path, mut store) = project(parent)?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    store.register_source(
        &request(&store, &original, "register-original", "camera", None)?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    register_event(&mut store)?;
    let source = SourceAudio {
        asset: id("camera"),
        span: span(12000, 22000, 48000)?,
    };
    let room = |length| {
        BeatNode::hold(
            "RoomTone",
            HoldRecipe {
                duration: frames(length),
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::RoomTone {
                    source: source.clone(),
                },
            },
        )
    };
    let picture = |length| -> Result<BeatNode> {
        let mut result = BeatNode::sequence("Qualified Original", vec![]);
        result.kind = NodeKind::Source {
            source: SourceNode {
                duration: frames(length),
                edit_window: None,
                video: SourceVideo::Stream {
                    asset: id("camera"),
                    span: span(12012, 12012 + length * 1001, 30000)?,
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(source.clone()),
                audio_mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    FrameRate::new(30000, 1001)?,
                )?,
                link: LinkRelation::Independent,
                audio_offset: AudioSample(0),
            },
        };
        Ok(result)
    };
    let mut stage = BeatNode::sequence("Fixed Preserve output", vec![]);
    stage.kind = NodeKind::Retime {
        child: node("body"),
        duration: frames(4),
        mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(8))?,
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    let mut nodes = BTreeMap::from([
        (node("lead"), room(3)),
        (node("processing"), stage),
        (
            node("body"),
            BeatNode::sequence("Input", vec![node("before"), node("hold"), node("after")]),
        ),
        (node("before"), picture(3)?),
        (node("hold"), room(2)),
        (node("after"), picture(4)?),
        (node("tail"), picture(3)?),
    ]);
    for recipe in nodes.values_mut() {
        recipe.audio_edges.node_start = AudioEdgePolicy::Hard;
        recipe.audio_edges.node_end = AudioEdgePolicy::Hard;
    }
    let lead = nodes.remove(&node("lead")).ok_or("lead")?;
    let tail = nodes.remove(&node("tail")).ok_or("tail")?;
    for (index, name, nodes) in [
        (0, "lead", BTreeMap::from([(node("lead"), lead)])),
        (1, "processing", nodes),
        (2, "tail", BTreeMap::from([(node("tail"), tail)])),
    ] {
        store.commit(&edit(
            &store.snapshot()?,
            &format!("insert-{name}"),
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes,
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        ))?;
    }
    let event_source = SourceAudio {
        asset: id("event"),
        span: span(3100, 3200, 44100)?,
    };
    let event = BeatSound {
        label: "Measured 44.1k output event".into(),
        mapping: SourceAudioMapping::natural_rate(event_source.span, FrameRate::new(30000, 1001)?)?,
        source: event_source,
        offset: AudioSample(37),
        treatments: Default::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    store.commit(&edit(
        &store.snapshot()?,
        "attach-output",
        Command::SetBeatSound {
            owner: node("processing"),
            id: SoundId::new("output")?,
            event,
        },
    ))?;
    if prior_journal {
        store.commit(&edit(
            &store.snapshot()?,
            "move-unchanged-stage",
            Command::SetHoldDuration {
                node: node("lead"),
                duration: frames(4),
            },
        ))?;
    }
    Ok((path, store))
}

fn stable_rows(path: &Path) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let db = Connection::open(path.join("project.sqlite"))?;
    let rows = |sql| -> Result<Vec<String>> {
        Ok(db
            .prepare(sql)?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<_, _>>()?)
    };
    Ok((
        rows("SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id")?,
        rows(
            "SELECT json_array(owner_revision,ordinal,step_revision,document) FROM transaction_steps ORDER BY owner_revision,ordinal",
        )?,
        register_rows(path)?,
    ))
}

#[test]
fn fixed_retime_output_sound_hold_duration_preview_commit_inverse_history_and_reopen() -> Result {
    for prior_journal in [false, true] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = setup(scratch.path(), prior_journal)?;
        let baseline = store.snapshot()?;
        let event = baseline.beat_sounds()[&node("processing")][&SoundId::new("output")?].clone();
        assert_eq!(
            event.mapping,
            SourceAudioMapping::natural_rate(
                span(3100, 3200, 44100)?,
                FrameRate::new(30000, 1001)?
            )?
        );
        assert_eq!(event.offset, AudioSample(37));
        let support = event.mapping.selection_frames_with_offset(
            frames(4),
            event.offset,
            FrameRate::new(30000, 1001)?,
        )?;
        assert_eq!(support.start, ExactRatio::new(185, 8008)?);
        let journals = baseline.audio_bindings().sound_clocks().clone();
        if prior_journal {
            let journal = &journals[&node("processing")][&SoundId::new("output")?];
            assert_eq!(journal.scope(), &node("processing"));
            assert_eq!(journal.clocks().len(), 1);
            assert_eq!(
                journal.clocks()[0].timing(),
                &AudioTimingId {
                    allocation: revision("move-unchanged-stage"),
                    ordinal: 0
                }
            );
            assert_eq!(journal.clocks()[0].owner(), &node("processing"));
        } else {
            assert!(journals.is_empty());
        }
        let bank = store.registers()?;
        let old = deadpan_core::capture_unbound_audio_bindings(
            &baseline,
            AudioTimingId {
                allocation: revision("old-lattice"),
                ordinal: 0,
            },
        )?;
        let mut snapshots = vec![baseline.clone()];
        for (name, length) in [("grow-input", 3), ("shrink-input", 1)] {
            let before = store.snapshot()?;
            let request = edit(
                &before,
                name,
                Command::SetHoldDuration {
                    node: node("hold"),
                    duration: frames(length),
                },
            );
            let rows = stable_rows(&path)?;
            let count = counts(&path)?;
            let available = store.history_availability()?;
            let preview = store.preview(&request)?;
            assert_eq!(store.snapshot()?, before);
            assert_eq!(stable_rows(&path)?, rows);
            assert_eq!(counts(&path)?, count);
            assert_eq!(store.history_availability()?, available);
            assert_eq!(store.registers()?, bank);
            assert_eq!(store.commit(&request)?.edit, preview);
            let after = store.snapshot()?;
            assert_eq!(preview.inverse.apply(&after)?, before);
            assert_eq!(after.duration()?, baseline.duration()?);
            assert_eq!(after.node_duration(&node("body"))?, frames(7 + length));
            assert_eq!(
                after.nodes()[&node("processing")],
                baseline.nodes()[&node("processing")]
            );
            assert_eq!(
                after.beat_sounds()[&node("processing")][&SoundId::new("output")?],
                event
            );
            assert_eq!(after.audio_bindings().sound_clocks(), &journals);
            let occurrence = InstancePath {
                node: node("after"),
                repeats: vec![],
            };
            let source = after
                .audio_bindings()
                .resolve(&node("after"), &occurrence, 100_000)?;
            let original = old.resolve(&node("after"), &occurrence, 100_000)?;
            assert_eq!(source.lattice, original.lattice);
            assert_eq!(
                source.lattice.clock,
                deadpan_core::AudioClockRoot::PreserveInputPointCeil {
                    stage: node("processing")
                }
            );
            assert_eq!(source.lattice.grid_origin, ExactRatio::ONE);
            assert_eq!(source.lattice.origin, ExactRatio::integer(5));
            assert_eq!(source.lattice.sample_boundary(ExactRatio::ZERO)?, 6407);
            let live = deadpan_core::FrozenAudioLayout::capture(&after)?.project_scoped(
                &node("body"),
                &occurrence,
                ExactRatio::ZERO,
                None,
                100_000,
            )?;
            assert_eq!(live.origin, ExactRatio::integer(3 + length));
            assert_ne!(live.origin, source.lattice.origin);
            // An unsplit entry at local0 can compact its inert reanchor away.
            // Require the resolved entry/phase, not a redundant stored step.
            let entry = |binding: &deadpan_core::ResolvedAudioBinding| {
                binding.resume.as_ref().map_or(
                    (binding.lattice.local_support.start, ExactRatio::ZERO),
                    |resume| (resume.local_boundary, resume.reference_local_delta),
                )
            };
            assert_eq!(entry(&source), entry(&original));
            snapshots.push(after);
        }
        store.validate_full()?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, snapshots[2]);
        store.undo(snapshots[2].revision_id(), revision("undo-shrink"))?;
        assert_authored(&store.snapshot()?, &snapshots[1])?;
        store.undo(&revision("undo-shrink"), revision("undo-grow"))?;
        assert_authored(&store.snapshot()?, &snapshots[0])?;
        store.redo(&revision("undo-grow"), revision("redo-grow"))?;
        assert_authored(&store.snapshot()?, &snapshots[1])?;
        store.redo(&revision("redo-grow"), revision("redo-shrink"))?;
        assert_authored(&store.snapshot()?, &snapshots[2])?;
        for snapshot in &snapshots {
            assert_eq!(store.snapshot_at(snapshot.revision_id())?, *snapshot);
        }
        assert_eq!(store.registers()?, bank);
        store.validate_full()?;
        let final_snapshot = store.snapshot()?;
        drop(store);
        let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
        assert_eq!(store.snapshot()?, final_snapshot);
        store.validate_full()?;
    }
    Ok(())
}

#[test]
fn fixed_retime_output_sound_late_child_dependency_refusal_preserves_all_rows() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = setup(scratch.path(), true)?;
    let before = store.snapshot()?;
    let first = Command::SetHoldDuration {
        node: node("hold"),
        duration: frames(3),
    };
    store.preview(&edit(&before, "successful-control", first.clone()))?;
    let event = before.beat_sounds()[&node("processing")][&SoundId::new("output")?].clone();
    let bank = store.registers()?;
    let steps = vec![
        ResolvedStep::Edit {
            edit: LeafEdit::new(revision("late-grow"), first)?,
        },
        ResolvedStep::Edit {
            edit: LeafEdit::new(
                revision("late-child-sound"),
                Command::SetBeatSound {
                    owner: node("body"),
                    id: SoundId::new("child")?,
                    event,
                },
            )?,
        },
        ResolvedStep::Edit {
            edit: LeafEdit::new(
                revision("late-shrink"),
                Command::SetHoldDuration {
                    node: node("hold"),
                    duration: frames(1),
                },
            )?,
        },
    ];
    let prefix = edit(
        &before,
        "successful-prefix",
        Command::Compound {
            transaction: ResolvedTransaction::new(
                bank.version,
                BTreeMap::new(),
                steps[..2].to_vec(),
            )?,
        },
    );
    store.preview_compound(&prefix)?;
    let transaction = ResolvedTransaction::new(bank.version, BTreeMap::new(), steps)?;
    let request = edit(&before, "late-outer", Command::Compound { transaction });
    let rows = stable_rows(&path)?;
    let count = counts(&path)?;
    assert!(
        store
            .preview_compound(&request)
            .unwrap_err()
            .to_string()
            .contains("sound clock processing subtree changed")
    );
    assert!(
        store
            .commit_compound(&request, None)
            .unwrap_err()
            .to_string()
            .contains("sound clock processing subtree changed")
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.registers()?, bank);
    assert_eq!(stable_rows(&path)?, rows);
    assert_eq!(counts(&path)?, count);
    assert!(store.snapshot_at(&revision("late-grow")).is_err());
    store.validate_full()?;
    Ok(())
}
