use super::*;
use deadpan_store::AccessMode;

fn selected(start: i64, end: i64) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: asset(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base,
            },
            SourceTimestamp {
                ticks: end,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn expected_loop(input: &[[f32; 2]], frames: usize) -> Vec<[f32; 2]> {
    assert_eq!(input.len(), 256);
    (0..frames)
        .map(|at| {
            let phase = at % 160;
            if at < 160 || phase >= 96 {
                input[phase]
            } else {
                let weight = phase as f64 / 96.0;
                std::array::from_fn(|channel| {
                    (f64::from(input[160 + phase][channel]) * (1.0 - weight)
                        + f64::from(input[phase][channel]) * weight) as f32
                })
            }
        })
        .collect()
}

fn collect_pcm(
    session: &mut ProjectAudioSession,
    start: i64,
    frames: u32,
) -> Result<Vec<[f32; 2]>> {
    let mut samples = Vec::new();
    for offset in (0..frames).step_by(256) {
        let block = session.read_time_mapped(
            AudioSample(start + i64::from(offset)),
            (frames - offset).min(256),
            &active(),
        )?;
        samples.extend(block.samples);
    }
    Ok(samples)
}

#[test]
fn authored_room_tone_range_and_silence_share_cli_history_and_actual_pcm() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "import",
    )?;
    let original = collect_pcm(&mut ProjectAudioSession::open(&path)?, 0, 512)?;
    assert!(original.iter().flatten().any(|sample| sample.abs() > 0.001));
    for (name, index, frames) in [("ambience", 0, 2), ("silence", 1, 1)] {
        commit(
            &mut store,
            name,
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(
                        node(name),
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                duration: FrameDuration::new(frames)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                                picture_context: None,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        )?;
    }
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    assert_eq!(
        collect_pcm(&mut ProjectAudioSession::open(&path)?, 0, 3200)?,
        vec![[0.0; 2]; 3200]
    );
    let request = scratch.path().join("room-tone.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "protocol": 1, "project_id": before.project_id(),
            "expected_revision": before.revision_id(), "new_revision": "room-tone",
            "command": Command::SetHoldAudio {
                node: node("ambience"), audio: HoldAudio::RoomTone { source: selected(0, 256) },
            },
        }))?,
    )?;
    drop(store);
    let invoke = |dry_run: bool| -> Result<Value> {
        let mut command = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"));
        command.args([
            "command",
            path.to_str().unwrap(),
            "--json",
            request.to_str().unwrap(),
        ]);
        if dry_run {
            command.arg("--dry-run");
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let preview = invoke(true)?;
    assert_eq!(preview["edit"]["duration_delta"], 0);
    assert_eq!(counts(&path)?, before_counts);
    let committed = invoke(false)?;
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let room = store.snapshot()?;
    assert_eq!(room.duration()?, before.duration()?);
    assert_eq!(room.audio_bindings(), before.audio_bindings());
    assert_eq!(room.presentation_basis(), before.presentation_basis());
    for (id, old_node) in before.nodes() {
        let mut current = room.nodes()[id].clone();
        if id == &node("ambience") {
            let NodeKind::Hold { recipe } = &mut current.kind else {
                panic!("Hold changed kind")
            };
            recipe.audio = HoldAudio::Silence;
        }
        assert_eq!(
            &current, old_node,
            "only the selected Hold audio policy changes"
        );
    }
    let expected = expected_loop(&original[..256], 3200);
    let mut session = ProjectAudioSession::open(&path)?;
    assert_eq!(collect_pcm(&mut session, 0, 3200)?, expected);
    assert_eq!(
        session
            .read_time_mapped(AudioSample(3200), 256, &active())?
            .samples,
        vec![[0.0; 2]; 256]
    );
    assert_eq!(
        collect_pcm(&mut session, 4800, 512)?,
        original,
        "following Original speech is untouched"
    );
    // A cold interior read uses the complete loop origin, rather than restarting.
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_time_mapped(AudioSample(777), 256, &active())?
            .samples,
        expected[777..1033]
    );
    commit(
        &mut store,
        "different-range",
        Command::SetHoldAudio {
            node: node("ambience"),
            audio: HoldAudio::RoomTone {
                source: selected(256, 512),
            },
        },
    )?;
    let replacement = expected_loop(&original[256..], 3200);
    assert_ne!(replacement, expected);
    assert_eq!(
        collect_pcm(&mut ProjectAudioSession::open(&path)?, 0, 3200)?,
        replacement
    );
    commit(
        &mut store,
        "digital-silence",
        Command::SetHoldAudio {
            node: node("ambience"),
            audio: HoldAudio::Silence,
        },
    )?;
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_limited(AudioSample(777), 256, &active())?
            .samples,
        vec![[0.0; 2]; 256]
    );
    store.undo(&revision("digital-silence"), revision("undo-silence"))?;
    assert_eq!(
        collect_pcm(&mut ProjectAudioSession::open(&path)?, 0, 3200)?,
        replacement
    );
    store.redo(&revision("undo-silence"), revision("redo-silence"))?;
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_time_mapped(AudioSample(777), 256, &active())?
            .samples,
        vec![[0.0; 2]; 256]
    );
    store.undo(&revision("redo-silence"), revision("restore-room-tone"))?;
    let final_document = store.snapshot()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
        final_document
    );
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_time_mapped(AudioSample(777), 256, &active())?
            .samples,
        replacement[777..1033]
    );
    Ok(())
}
