use super::*;

#[test]
fn wide_pcm_catalog_registration_placement_and_history_keep_exact_audio_and_picture() -> Result {
    let scratch = tempfile::tempdir()?;
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()?;
    let fixtures = original
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("audio-fixtures");
    let manifest: Value =
        serde_json::from_slice(&fs::read(fixtures.join("wide-pcm-manifest.json"))?)?;
    for row in manifest["files"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let rate = row["sample_rate"].as_u64().unwrap() as u32;
        let package = scratch.path().join(format!("{name}.deadpan"));
        let path = package.to_str().unwrap();
        success(&[
            "project",
            "create-original",
            path,
            original.to_str().unwrap(),
        ])?;
        let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        let source = fixtures.join(name);
        let retained = success(&["project", "retain-original", path, source.to_str().unwrap()])?;
        let mut input = json!({"protocol":1,
            "registration":{"expected_revision":before.revision_id(),"new_revision":"sound-registered",
                "original":retained["retained_original"]["record"]["object"]["content"],
                "new_asset_id":"sound","label":name,"insertion":null},
            "streams":{"type":"audio_only","stream":0}});
        if row["mask"] == 0 {
            let request = save(scratch.path(), &input)?;
            for dry in [true, false] {
                let refusal = registration(&package, &request, dry, false)?;
                assert_eq!(
                    refusal["error"]["code"], "AudioLayoutInterpretationRequired",
                    "{name}"
                );
                assert_eq!(
                    ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
                    before
                );
            }
            input["streams"]["interpretation"] = if row["channels"] == 1 {
                "mono"
            } else {
                "stereo_left_right"
            }
            .into();
        }
        let request = save(scratch.path(), &input)?;
        registration(&package, &request, true, true)?;
        assert_eq!(
            ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
            before
        );
        registration(&package, &request, false, true)?;
        let registered = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert_eq!(registered.nodes(), before.nodes());
        let asset = &registered.assets()[&AssetId::new("sound")?];
        assert!(asset.video.is_none());
        let audio = asset.audio.unwrap();
        assert_eq!(
            (audio.start().ticks, audio.end().ticks),
            (0, 8197),
            "{name}"
        );
        assert_eq!(
            audio.start().time_base,
            deadpan_core::SourceTimeBase::new(1, rate)?
        );
        let input = save(
            scratch.path(),
            &json!({"protocol":1,"expected_revision":registered.revision_id(),
            "edit":{"type":"place","asset":"sound","at":{"frame":2},"id":"placed"}}),
        )?;
        assert_eq!(
            success(&["sound", path, "--json", input.to_str().unwrap()])?["committed"],
            true
        );
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

#[test]
fn nonfinite_float_wave_registration_refuses_without_an_authored_or_receipt_write() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("bad-float.deadpan");
    create(&package, "30/1")?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut bytes = include_bytes!(
        "../../../../native/deadpan-source/tests/audio-fixtures/wide-f32-stereo.wav"
    )
    .to_vec();
    let data = bytes.windows(4).position(|v| v == b"data").unwrap() + 8;
    bytes[data..data + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    let original = retain(&package, &scratch.path().join("bad.wav"), &bytes, false)?;
    let input = request(
        &package,
        original,
        json!({"type":"audio_only","stream":0,"interpretation":"stereo_left_right"}),
        "rejected",
    )?;
    let input = save(scratch.path(), &input)?;
    let prior = counts(&package)?;
    for dry in [true, false] {
        let refused = registration(&package, &input, dry, false)?;
        assert!(
            refused["error"]["message"]
                .as_str()
                .unwrap()
                .contains("invalid_samples")
        );
        assert_eq!(counts(&package)?, prior);
        assert_eq!(
            ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
            before
        );
    }
    Ok(())
}
