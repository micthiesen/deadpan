use super::*;

#[test]
fn mp3_catalog_registration_placement_and_history_preserve_picture_and_exact_audio() -> Result {
    let scratch = tempfile::tempdir()?;
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()?;
    for (name, rate, start, count) in [
        ("stereo-cbr", 48000, 1105, 8197),
        ("mono-vbr", 44100, 1105, 8197),
        ("mono-mpeg25", 8000, 1105, 8197),
        ("stereo-spanning", 48000, 3029, 4868),
        ("stereo-untagged", 48000, 0, 10368),
        ("stereo-id3v4", 48000, 1105, 8197),
    ] {
        let package = scratch.path().join(format!("{name}.deadpan"));
        let path = package.to_str().unwrap();
        success(&[
            "project",
            "create-original",
            path,
            original.to_str().unwrap(),
        ])?;
        let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        let source = original
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("audio-fixtures")
            .join(format!("mp3-{name}.mp3"));
        let retained = success(&["project", "retain-original", path, source.to_str().unwrap()])?;
        let input = save(
            scratch.path(),
            &json!({
                "protocol":1,
                "registration": {"expected_revision":before.revision_id(),"new_revision":"sound-registered",
                    "original":retained["retained_original"]["record"]["object"]["content"],
                    "new_asset_id":"sound", "label":name, "insertion":null},
                "streams":{"type":"audio_only","stream":0}
            }),
        )?;
        registration(&package, &input, true, true)?;
        assert_eq!(
            ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
            before
        );
        registration(&package, &input, false, true)?;
        let registered = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert_eq!(registered.nodes(), before.nodes());
        let asset = &registered.assets()[&AssetId::new("sound")?];
        assert!(asset.video.is_none());
        let audio = asset.audio.unwrap();
        assert_eq!(audio.start().ticks, start, "{name}");
        assert_eq!(audio.end().ticks, start + count, "{name}");
        assert_eq!(
            audio.start().time_base,
            deadpan_core::SourceTimeBase::new(1, rate)?
        );
        let input = save(
            scratch.path(),
            &json!({"protocol":1,"expected_revision":registered.revision_id(),
            "edit":{"type":"place","asset":"sound","at":{"frame":2},"id":"placed"}}),
        )?;
        let placed = success(&["sound", path, "--json", input.to_str().unwrap()])?;
        assert_eq!(placed["committed"], true);
        let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert_eq!(after.nodes(), before.nodes());
        assert_eq!(after.duration()?, before.duration()?);
        assert_eq!(after.sounds().len(), 1);
        assert_eq!(after.sounds().values().next().unwrap().source.span, audio);
        success(&[
            "inspect-audio",
            path,
            "--samples",
            "3400",
            "3656",
            "--limited",
        ])?;
        success(&[
            "project",
            "undo",
            path,
            "--expected",
            after.revision_id().as_str(),
        ])?;
        let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert!(undone.sounds().is_empty());
        success(&[
            "project",
            "redo",
            path,
            "--expected",
            undone.revision_id().as_str(),
        ])?;
        let redone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert_eq!(redone.nodes(), after.nodes());
        assert_eq!(redone.assets(), after.assets());
        assert_eq!(redone.sounds(), after.sounds());
        success(&["project", "validate", path])?;
    }
    Ok(())
}
