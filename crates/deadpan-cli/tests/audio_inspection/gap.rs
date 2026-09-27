use super::*;
use deadpan_core::ExactRatio;
use deadpan_plan::{AudioDefinitionSelector, AudioRootPlacement, SignalSample};

fn recipe(audio: HoldAudio) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(1).unwrap(),
        video: HoldVideo::Background,
        audio,
    }
}

fn room_tone() -> Result<HoldRecipe> {
    let time_base = SourceTimeBase::new(1, 48_000)?;
    Ok(recipe(HoldAudio::RoomTone {
        source: SourceAudio {
            asset: asset(),
            span: SourceSpan::new(
                SourceTimestamp {
                    ticks: 0,
                    time_base,
                },
                SourceTimestamp {
                    ticks: 256,
                    time_base,
                },
            )?,
        },
    }))
}

fn inspect_gap(
    path: &Path,
    clock: Option<&Path>,
    owner: &str,
    start: &str,
    end: &str,
    retained: Option<&str>,
) -> Result<std::process::Output> {
    let mut command = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"));
    command.args([
        if clock.is_some() {
            "inspect-audio-placement"
        } else {
            "inspect-audio-definition"
        },
        path.to_str().unwrap(),
        "--repeat-gap",
        owner,
    ]);
    if let Some(clock) = clock {
        command.args(["--clock", clock.to_str().unwrap()]);
    }
    command.args(["--samples", start, end]);
    if let Some(retained) = retained {
        command.args(["--revision", retained]);
    }
    Ok(command.output()?)
}

fn successful(output: std::process::Output) -> Result<Value> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn repeat_gap_cli_reads_unplayed_recipe_and_retained_history_without_writes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "registered",
    )?;
    commit(
        &mut store,
        "repeated",
        Command::WrapRepeat {
            node: node("clip"),
            id: node("repeat"),
            plays: 1,
            gap: Some(room_tone()?),
            anchor_policy: Default::default(),
        },
    )?;
    commit(
        &mut store,
        "overridden",
        Command::SetPlayOverride {
            node: node("repeat"),
            iteration: deadpan_core::IterationId {
                allocation: revision("repeated"),
                ordinal: 0,
            },
            subtree: Subtree {
                root: node("silent-child"),
                nodes: BTreeMap::from([(
                    node("silent-child"),
                    BeatNode::hold("Quiet", recipe(HoldAudio::Silence)),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    assert_eq!(before.duration()?.frames(), 1);
    let mut session = ProjectAudioSession::open(&path)?;
    assert_eq!(
        session
            .read_time_mapped(AudioSample(0), 128, &active())?
            .samples,
        vec![[0.0; 2]; 128]
    );

    // There is no rendered gap in this one-play Repeat, and its only played
    // child is silent. The explicit gap definition still owns the real source.
    let definition = successful(inspect_gap(&path, None, "repeat", "0", "128", None)?)?;
    assert_eq!(definition["audio"]["revision_id"], "overridden");
    assert_eq!(
        definition["audio"]["definition"],
        json!({"type":"repeat_gap", "repeat":"repeat"})
    );
    assert_eq!(definition["audio"]["root"], "repeat");
    let pcm: Vec<[f32; 2]> = serde_json::from_value(definition["audio"]["samples"].clone())?;
    assert_eq!(pcm, original[..128]);
    assert!(pcm.iter().flatten().any(|sample| *sample != 0.0));

    let clock = AudioRootPlacement::new(
        ExactRatio::integer(-1),
        ExactRatio::ONE,
        ExactRatio::ZERO..ExactRatio::ONE,
    )?;
    let clock_path = scratch.path().join("gap-clock.json");
    fs::write(&clock_path, serde_json::to_string(&clock)?)?;
    let placed = successful(inspect_gap(
        &path,
        Some(&clock_path),
        "repeat",
        "-1600",
        "-1472",
        None,
    )?)?;
    assert_eq!(
        placed["audio"]["definition"],
        definition["audio"]["definition"]
    );
    assert_eq!(placed["audio"]["placement"], serde_json::to_value(&clock)?);
    assert_eq!(placed["audio"]["gap_after"], Value::Null);
    assert_eq!(
        placed["audio"]["instance"],
        json!({"node":"repeat", "repeats":[]})
    );
    assert_eq!(placed["audio"]["samples"], definition["audio"]["samples"]);
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);

    commit(
        &mut store,
        "silent-gap",
        Command::SetRepeat {
            node: node("repeat"),
            plays: 1,
            gap: Some(recipe(HoldAudio::Silence)),
        },
    )?;
    let after = store.snapshot()?;
    let after_counts = counts(&path)?;
    let silent = successful(inspect_gap(&path, None, "repeat", "0", "128", None)?)?;
    assert_eq!(silent["audio"]["revision_id"], "silent-gap");
    assert_eq!(silent["audio"]["samples"], json!(vec![[0.0_f32; 2]; 128]));
    assert_eq!(
        silent["audio"]["suppressed"],
        json!([{"start":0,"end":128}])
    );
    assert_eq!(
        successful(inspect_gap(
            &path,
            None,
            "repeat",
            "0",
            "128",
            Some("overridden")
        )?)?,
        definition
    );
    assert_eq!(
        successful(inspect_gap(
            &path,
            Some(&clock_path),
            "repeat",
            "-1600",
            "-1472",
            Some("overridden")
        )?)?,
        placed
    );
    assert_eq!(store.snapshot()?, after);
    assert_eq!(counts(&path)?, after_counts);

    for (owner, start, end, code) in [
        ("root", "0", "1", "AudioDefinitionUnavailable"),
        ("clip", "0", "1", "AudioDefinitionUnavailable"),
        ("missing", "0", "1", "AudioDefinitionUnavailable"),
        ("repeat", "1600", "1601", "AudioRangeOutOfRange"),
        ("repeat", "-1", "1", "AudioRangeOutOfRange"),
        ("repeat", "0", "257", "AudioRangeOutOfRange"),
    ] {
        let output = inspect_gap(&path, None, owner, start, end, None)?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["error"]["code"], code);
    }
    commit(
        &mut store,
        "no-gap",
        Command::SetRepeat {
            node: node("repeat"),
            plays: 1,
            gap: None,
        },
    )?;
    let output = inspect_gap(&path, None, "repeat", "0", "1", None)?;
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(error["error"]["code"], "AudioDefinitionUnavailable");
    Ok(())
}

#[test]
fn unplayed_gap_reads_revalidate_original_bytes_before_returning_pcm() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let linked = scratch.path().join("linked.mp4");
    fs::copy(fixture("cfr-bframes.mp4"), &linked)?;
    register(
        &mut store,
        &linked,
        1,
        OriginalOwnership::Linked { bookmark: None },
        "registered",
    )?;
    commit(
        &mut store,
        "repeated",
        Command::WrapRepeat {
            node: node("clip"),
            id: node("repeat"),
            plays: 1,
            gap: Some(room_tone()?),
            anchor_policy: Default::default(),
        },
    )?;
    let before = store.snapshot()?;
    let mut session = ProjectAudioSession::open(&path)?;
    let mut bytes = fs::read(&linked)?;
    bytes[100] ^= 1;
    fs::write(&linked, bytes)?;
    let selector = AudioDefinitionSelector::RepeatGap {
        repeat: node("repeat"),
    };
    assert!(
        session
            .read_definition(selector.clone(), SignalSample(0), 128, &active())
            .is_err()
    );
    assert!(
        session
            .read_placement(
                selector,
                AudioRootPlacement::new(
                    ExactRatio::ZERO,
                    ExactRatio::ONE,
                    ExactRatio::ZERO..ExactRatio::ONE
                )?,
                AudioSample(0),
                128,
                &active(),
            )
            .is_err()
    );
    let output = inspect_gap(&path, None, "repeat", "0", "128", None)?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(error["error"]["code"], "SourceAudioUnavailable");
    assert_eq!(store.snapshot()?, before);
    Ok(())
}

#[test]
fn pause_captures_unplayed_gap_then_growth_and_historical_reads_keep_verified_media() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "registered",
    )?;
    let child_duration = store.snapshot()?.duration()?;
    commit(
        &mut store,
        "repeat",
        Command::WrapRepeat {
            node: node("clip"),
            id: node("repeat"),
            plays: 1,
            gap: Some(room_tone()?),
            anchor_policy: Default::default(),
        },
    )?;
    let mut tail = recipe(HoldAudio::Silence);
    tail.duration = FrameDuration::new(2)?;
    commit(
        &mut store,
        "tail",
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("tail"),
                nodes: BTreeMap::from([(node("tail"), BeatNode::hold("Tail", tail))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?;
    commit(
        &mut store,
        "bound",
        Command::InsertTime {
            at: deadpan_core::ProjectFrame(child_duration.frames() + 1),
            hold: recipe(HoldAudio::Silence),
            id: node("pause"),
            identities: deadpan_core::SplitIdentities {
                nodes: vec![node("left"), node("right"), node("copy")],
            },
            timing: deadpan_core::AudioTimingId {
                allocation: revision("bound"),
                ordinal: 0,
            },
        },
    )?;
    let before = store.snapshot()?;
    assert!(
        before
            .audio_bindings()
            .gap_bindings()
            .contains_key(&node("repeat"))
    );
    let initial_counts = counts(&path)?;
    let captured = successful(inspect_gap(&path, None, "repeat", "0", "128", None)?)?;
    assert_eq!(captured["audio"]["revision_id"], "bound");
    let pcm: Vec<[f32; 2]> = serde_json::from_value(captured["audio"]["samples"].clone())?;
    assert_eq!(pcm, original[..128]);
    assert_eq!(counts(&path)?, initial_counts);

    commit(
        &mut store,
        "grown",
        Command::SetRepeat {
            node: node("repeat"),
            plays: 3,
            gap: Some(room_tone()?),
        },
    )?;
    let mut session = ProjectAudioSession::open(&path)?;
    // The once-final play gained a gap. It uses the retained recipe's canonical
    // local-zero phase, with original-byte/receipt admission still active.
    let first_gap = AudioSample(child_duration.frames() * 1600);
    assert_eq!(
        session.read_time_mapped(first_gap, 128, &active())?.samples,
        original[..128]
    );
    commit(
        &mut store,
        "quiet",
        Command::SetRepeat {
            node: node("repeat"),
            plays: 3,
            gap: Some(recipe(HoldAudio::Silence)),
        },
    )?;
    let final_counts = counts(&path)?;
    assert_eq!(
        successful(inspect_gap(&path, None, "repeat", "0", "128", None)?)?["audio"]["samples"],
        json!(vec![[0.0_f32; 2]; 128])
    );
    assert_eq!(
        successful(inspect_gap(
            &path,
            None,
            "repeat",
            "0",
            "128",
            Some("bound")
        )?)?,
        captured
    );
    assert_eq!(counts(&path)?, final_counts);
    Ok(())
}
