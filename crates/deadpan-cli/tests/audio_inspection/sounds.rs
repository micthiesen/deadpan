use super::*;
use deadpan_core::{AudioEdgePolicy, SoundEvent, SoundId, SoundOverflowPolicy, SourceAudioMapping};

#[test]
fn json_sound_command_reaches_shared_limited_bus_without_changing_picture_time() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "import",
    )?;
    let before = store.snapshot()?;
    let source = SourceAudio {
        asset: asset(),
        span: before.assets()[&asset()].audio.unwrap(),
    };
    let event = SoundEvent {
        owner: before.root().clone(),
        label: "Original echo".into(),
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            before.presentation_basis().frame_rate,
        )?,
        source,
        offset: AudioSample(0),
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Automatic,
        overflow: SoundOverflowPolicy::Reject,
    };
    let command = Command::SetSound {
        id: SoundId::new("echo")?,
        event,
    };
    let request = scratch.path().join("sound-command.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "protocol": 1, "project_id": before.project_id(),
            "expected_revision": before.revision_id(), "new_revision": "sound-added",
            "command": command,
        }))?,
    )?;
    drop(store);
    let before_counts = counts(&path)?;
    for dry_run in [true, false] {
        let mut process = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"));
        process.args([
            "command",
            path.to_str().unwrap(),
            "--json",
            request.to_str().unwrap(),
        ]);
        if dry_run {
            process.arg("--dry-run");
        }
        let output = process.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let wire: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(wire["committed"], !dry_run);
        if dry_run {
            assert_eq!(counts(&path)?, before_counts);
        }
    }
    let store = ProjectStore::open(&path, deadpan_store::AccessMode::ReadOnly)?;
    let after = store.snapshot()?;
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(after.sounds().len(), 1);
    drop(store);
    let retained_counts = counts(&path)?;
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args([
            "inspect-audio",
            path.to_str().unwrap(),
            "--samples",
            "137",
            "393",
            "--limited",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wire: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(wire["audio"]["stage"], "limited_authored_bus_pcm");
    assert_eq!(
        wire["audio"]["processing_order"],
        json!([
            "per_voice_time_pitch_edges_gain",
            "group_mix",
            "stereo_limiter"
        ])
    );
    assert_eq!(wire["audio"]["revision_id"], "sound-added");
    assert_eq!(wire["audio"]["samples"].as_array().unwrap().len(), 256);
    assert_eq!(
        counts(&path)?,
        retained_counts,
        "inspection writes no history"
    );
    Ok(())
}
