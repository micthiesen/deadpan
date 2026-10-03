use super::*;
use deadpan_core::{
    AudioEdgePolicy, AudioSample, BeatSound, Command as EditCommand, CommandRequest,
    ProjectDocument, RevisionId, SoundId, SoundOverflowPolicy, SourceAudio, SourceAudioMapping,
    SourceSpan, SourceTimestamp,
};

fn ready(directory: &Path) -> Result<PathBuf> {
    let package = directory.join("beat-sounds.deadpan");
    create(&package, "30000/1001")?;
    let original = retain(
        &package,
        &directory.join("source.mp4"),
        include_bytes!("../../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
        false,
    )?;
    let input = request(
        &package,
        original,
        json!({"type":"video_and_audio","audio_stream":1}),
        "registered",
    )?;
    registration(&package, &save(directory, &input)?, false, true)?;
    Ok(package)
}

fn event(document: &ProjectDocument) -> Result<BeatSound> {
    let asset = AssetId::new("imported-asset")?;
    let full = document.assets()[&asset].audio.unwrap();
    let span = SourceSpan::new(
        full.start(),
        SourceTimestamp {
            ticks: full.start().ticks + (full.end().ticks - full.start().ticks) / 2,
            time_base: full.start().time_base,
        },
    )?;
    Ok(BeatSound {
        label: "Owned overlay".into(),
        source: SourceAudio { asset, span },
        mapping: SourceAudioMapping::natural_rate(span, document.presentation_basis().frame_rate)?,
        offset: AudioSample(7),
        gain_millidecibels: -3000,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    })
}

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn headless_beat_sound_commands_preview_inspect_and_restore_with_fresh_revisions() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = ready(scratch.path())?;
    let path = package.to_str().unwrap();
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let owner = NodeId::new("imported-source")?;
    let id = SoundId::new("overlay")?;
    let event = event(&before)?;
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("owned-sound")?,
        command: EditCommand::SetBeatSound {
            owner: owner.clone(),
            id: id.clone(),
            event: event.clone(),
        },
    };
    let input = json!({"protocol":1,"project_id":request.project_id,
        "expected_revision":request.expected_revision,"new_revision":request.new_revision,
        "command":request.command});
    assert_eq!(input["command"]["command"], "set_beat_sound");
    let file = save(scratch.path(), &input)?;
    let file = file.to_str().unwrap();
    let before_counts = counts(&package)?;
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let preview = success(&["command", path, "--json", file, "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["edit"]["duration_delta"], 0);
    assert_eq!(
        preview["edit"]["forward"]["beat_sounds"][owner.as_str()]["after"][id.as_str()],
        serde_json::to_value(&event)?
    );
    assert_eq!(writer.snapshot()?, before);
    assert_eq!(counts(&package)?, before_counts);
    let hosted = deadpan_cli::live_project::execute_short(
        &mut writer,
        before.project_id(),
        &deadpan_cli::live_project::ShortOperation::Edit {
            request: Box::new(request.clone()),
            dry_run: true,
        },
    )?;
    assert_eq!(hosted.output, preview);
    assert!(hosted.committed_revision.is_none());
    assert!(hosted.committed_registers.is_none());
    drop(writer);
    let committed = success(&["command", path, "--json", file])?;
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    let added = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let dumped = success(&["project", "dump", path, "--json"])?;
    assert_eq!(dumped, serde_json::to_value(&added)?);
    assert_eq!(
        dumped["beat_sounds"][owner.as_str()][id.as_str()],
        serde_json::to_value(&event)?
    );
    assert_eq!(added.nodes(), before.nodes());
    assert_eq!(added.duration()?, before.duration()?);
    assert_eq!(
        failure(&["command", path, "--json", file])?["error"]["code"],
        "RevisionConflict"
    );
    success(&["project", "undo", path, "--expected", "owned-sound"])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_ne!(undone.revision_id(), before.revision_id());
    assert_authored(&undone, &before)?;
    success(&[
        "project",
        "redo",
        path,
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let redone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_ne!(redone.revision_id(), added.revision_id());
    assert_authored(&redone, &added)?;
    let delete = json!({"protocol":1,"project_id":redone.project_id(),
        "expected_revision":redone.revision_id(),"new_revision":"deleted-owned-sound",
        "command":{"command":"delete_beat_sound","owner":owner,"id":id}});
    fs::write(file, delete.to_string())?;
    success(&["command", path, "--json", file])?;
    assert!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .snapshot()?
            .beat_sounds()
            .is_empty()
    );
    success(&["project", "validate", path])?;
    Ok(())
}

#[test]
fn headless_beat_sound_strict_wire_and_missing_qualification_refuse_without_authoring() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = ready(scratch.path())?;
    let path = package.to_str().unwrap();
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let input = json!({"protocol":1,"project_id":before.project_id(),
        "expected_revision":before.revision_id(),"new_revision":"owned-sound",
        "command":{"command":"set_beat_sound","owner":"imported-source","id":"overlay","event":event(&before)?}});
    let file = save(scratch.path(), &input)?;
    let file = file.to_str().unwrap();
    let before_counts = counts(&package)?;
    for (key, value) in [("owner", json!(before.root())), ("unexpected", json!(true))] {
        let mut malformed = input.clone();
        // Ownership is outside the event recipe, never a second address.
        malformed["command"]["event"][key] = value;
        fs::write(file, malformed.to_string())?;
        assert_eq!(
            failure(&["command", path, "--json", file])?["error"]["code"],
            "InvalidInput"
        );
        assert_eq!(counts(&package)?, before_counts);
    }
    fs::write(file, input.to_string())?;
    success(&["command", path, "--json", file, "--dry-run"])?;
    let connection = Connection::open(package.join("project.sqlite"))?;
    connection.execute("DELETE FROM source_qualifications", [])?;
    let before_rows: Vec<String> = connection
        .prepare("SELECT document FROM revisions ORDER BY id")?
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let error = failure(&["command", path, "--json", file])?;
    assert_eq!(error["error"]["code"], "SourceRegistrationInvalid");
    let after_rows: Vec<String> = connection
        .prepare("SELECT document FROM revisions ORDER BY id")?
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(after_rows, before_rows);
    assert_eq!(
        (counts(&package)?.0, counts(&package)?.1),
        (before_counts.0, before_counts.1)
    );
    Ok(())
}
