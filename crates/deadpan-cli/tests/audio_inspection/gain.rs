use super::*;
use deadpan_core::{AudioTreatments, ClipGain, FrozenAudioContext, GainDb};
use deadpan_store::AccessMode;

fn treatments(muted: bool) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-6000).unwrap(), muted, vec![], vec![]).unwrap(),
    )
}

#[test]
fn gain_command_authored_pcm_context_and_undo_share_one_durable_revision() -> Result {
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
    let baseline =
        ProjectAudioSession::open(&path)?.read_edge_faded(AudioSample(137), 256, &active())?;
    assert!(
        baseline
            .samples
            .iter()
            .flatten()
            .any(|value| value.abs() > 0.001)
    );
    let request = json!({"protocol":1, "project_id":before.project_id(), "expected_revision":before.revision_id(),
        "new_revision":"gain", "command":{"command":"set_audio_treatments", "node":"clip", "treatments":treatments(false)}});
    let request_path = scratch.path().join("gain.json");
    fs::write(&request_path, serde_json::to_vec(&request)?)?;
    drop(store);
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args([
            "command",
            path.to_str().unwrap(),
            "--json",
            request_path.to_str().unwrap(),
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut session = ProjectAudioSession::open(&path)?;
    let authored = session.read_authored_bus(AudioSample(137), 256, &active())?;
    let amplitude = 10.0_f64.powf(-6000.0 / 20_000.0);
    let expected: Vec<_> = baseline
        .samples
        .iter()
        .map(|sample| sample.map(|value| (f64::from(value) * amplitude) as f32))
        .collect();
    assert_eq!(authored.samples, expected);
    assert_eq!(authored.stage, "authored_bus_pcm_before_mastering");
    assert_eq!(
        session
            .read_edge_faded(AudioSample(137), 256, &active())?
            .samples,
        baseline.samples
    );
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args([
            "inspect-audio",
            path.to_str().unwrap(),
            "--samples",
            "137",
            "393",
            "--authored-bus",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wire: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(wire["audio"]["revision_id"], "gain");
    assert_eq!(
        serde_json::from_value::<Vec<[f32; 2]>>(wire["audio"]["samples"].clone())?,
        expected
    );

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let gained = store.snapshot()?;
    let context = FrozenAudioContext::capture(&gained)?;
    assert_eq!(
        ProjectAudioSession::open_context(&path, &context)?
            .read_authored_bus(AudioSample(137), 256, &active())?
            .samples,
        expected
    );
    let mut forged = serde_json::to_value(&context)?;
    forged["audio_treatments"]["clip"] = json!(treatments(true));
    assert!(matches!(
        ProjectAudioSession::open_context(
            &path,
            &FrozenAudioContext::from_json(&forged.to_string())?
        ),
        Err(deadpan_cli::audio::ProjectAudioError::ContextMismatch)
    ));
    commit(
        &mut store,
        "mute",
        Command::SetAudioTreatments {
            node: node("clip"),
            treatments: treatments(true),
        },
    )?;
    let muted =
        ProjectAudioSession::open(&path)?.read_authored_bus(AudioSample(137), 256, &active())?;
    assert_eq!(muted.samples, vec![[0.0; 2]; 256]);
    assert!(muted.suppressed.is_empty());
    // An already opened session retains the exact earlier gain revision.
    assert_eq!(
        session
            .read_authored_bus(AudioSample(137), 256, &active())?
            .samples,
        expected
    );
    store.undo(&revision("mute"), revision("undo-mute"))?;
    store.undo(&revision("undo-mute"), revision("undo-gain"))?;
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_authored_bus(AudioSample(137), 256, &active())?
            .samples,
        baseline.samples
    );
    store.redo(&revision("undo-gain"), revision("redo-gain"))?;
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_authored_bus(AudioSample(137), 256, &active())?
            .samples,
        expected
    );
    store.validate()?;
    Ok(())
}
