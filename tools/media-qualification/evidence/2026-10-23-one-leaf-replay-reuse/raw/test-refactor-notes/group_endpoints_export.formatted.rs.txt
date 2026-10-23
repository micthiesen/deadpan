//! Verified export after grouping both endpoints of an attached-sound owner.
use super::*;
use std::fs;

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn sound_group_endpoint_splits_preserve_complete_pcm_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("sound group endpoint Render evidence: {}", root.display());
    let fixture = recipes::group_endpoints::build(&root.join("fixture"))?;
    check(&fixture)?;
    if skip_without_physical_mac("endpoint sound group encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = render(&fixture, &root.join("exports"))?;
    let rendered = started.elapsed();
    let (report, passed) = verify(
        &fixture,
        &movie,
        &fixture.revision,
        &["--every", "1", "--samples", "0:193794"],
    )?;
    // Retain a failed report too; checks below must never erase failure evidence.
    fs::write(
        root.join("sound-group-endpoints-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 121);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("full canonical audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["project_samples"], json!([0, 193794]));
    for window in audio {
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["uncovered_samples"], 0);
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    let mut row = summary_row(&fixture, &report);
    row["render_seconds"] = json!(rendered.as_secs_f64());
    row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
    row["movie"] = json!(movie);
    row["package"] = json!(fixture.package);
    row["revision"] = json!(fixture.revision);
    row["notes"] = json!(fixture.notes);
    row["canonical_pcm_oracle"] = json!(root.join("fixture/sound-group-endpoints-before-pcm.json"));
    row["canonical_samples_compared"] = json!(193794);
    row["canonical_channels"] = json!(2);
    row["independent_DSP_claim"] = json!(false);
    row["listening_claim"] = json!(false);
    fs::write(
        root.join("sound-group-endpoints-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    assert!(passed, "{report}");
    Ok(())
}

/// Removing a still-unpaired neutral wrapper preserves actual complete PCM
/// after later edits, through the public semantic CLI and a verified file.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn neutral_unpaired_sound_group_ungroup_preserves_complete_pcm_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("neutral Ungroup Render evidence: {}", root.display());
    let fixture = recipes::group_endpoints::aligned::build_neutral_ungroup(&root.join("fixture"))?;
    check(&fixture)?;
    if skip_without_physical_mac("neutral retained-scope Ungroup encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = render(&fixture, &root.join("exports"))?;
    let rendered = started.elapsed();
    let (report, passed) = verify(
        &fixture,
        &movie,
        &fixture.revision,
        &["--every", "1", "--samples", "0:24024"],
    )?;
    // Retain failure evidence before making any verification assertions.
    fs::write(
        root.join("neutral-ungroup-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 15);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("full canonical audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["project_samples"], json!([0, 24024]));
    for window in audio {
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["uncovered_samples"], 0);
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    let mut row = summary_row(&fixture, &report);
    row["render_seconds"] = json!(rendered.as_secs_f64());
    row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
    row["movie"] = json!(movie);
    row["package"] = json!(fixture.package);
    row["revision"] = json!(fixture.revision);
    row["notes"] = json!(fixture.notes);
    row["canonical_pcm_evidence"] = json!({
        "before":root.join("fixture/neutral-ungroup-before-pcm.json"),
        "after":root.join("fixture/neutral-ungroup-after-pcm.json"),
        "event_off":root.join("fixture/aligned-event-off-pcm.json"),
        "event_off_control":root.join("fixture/aligned-event-control.json"),
        "later_edit_control":root.join("fixture/neutral-ungroup-later-edit-control.json"),
        "history":root.join("fixture/neutral-ungroup-history.json"),
    });
    row["canonical_samples_compared"] = json!(24024);
    row["canonical_channels"] = json!(2);
    row["authored_limited_and_limiter_gain_bits_equal"] = json!(true);
    row["independent_DSP_claim"] = json!(false);
    row["listening_claim"] = json!(false);
    fs::write(
        root.join("neutral-ungroup-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    assert!(passed, "{report}");
    Ok(())
}

/// A new processor born inside an old sound scope retains that input after
/// neutral Split, relocation, a complete editable copy and durable reopening.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn aligned_sound_scope_group_split_move_copy_preserves_canonical_pcm_and_verified_export() -> Result
{
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("aligned sound scope Render evidence: {}", root.display());
    let fixture = recipes::group_endpoints::aligned::build(&root.join("fixture"))?;
    check(&fixture)?;
    if skip_without_physical_mac("aligned sound scope encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = render(&fixture, &root.join("exports"))?;
    let rendered = started.elapsed();
    let (report, passed) = verify(
        &fixture,
        &movie,
        &fixture.revision,
        &["--every", "1", "--samples", "0:30430"],
    )?;
    fs::write(
        root.join("aligned-sound-scope-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 19);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("full canonical audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["project_samples"], json!([0, 30430]));
    for window in audio {
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["uncovered_samples"], 0);
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    let mut row = summary_row(&fixture, &report);
    row["render_seconds"] = json!(rendered.as_secs_f64());
    row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
    row["movie"] = json!(movie);
    row["package"] = json!(fixture.package);
    row["revision"] = json!(fixture.revision);
    row["notes"] = json!(fixture.notes);
    row["canonical_pcm_evidence"] = json!({
        "before":root.join("fixture/aligned-before-pcm.json"),
        "treated":root.join("fixture/aligned-treated-pcm.json"),
        "moved":root.join("fixture/aligned-moved-pcm.json"),
        "final":root.join("fixture/aligned-final-pcm.json"),
    });
    row["neutral_group_and_split_canonical_samples_compared"] = json!(24024);
    row["final_canonical_samples_compared"] = json!(30430);
    row["canonical_channels"] = json!(2);
    row["independent_DSP_claim"] = json!(false);
    row["move_PCM_translation_claim"] = json!(false);
    row["listening_claim"] = json!(false);
    fs::write(
        root.join("aligned-sound-scope-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    assert!(passed, "{report}");
    Ok(())
}

/// One real-media semantic leaf publishes the same exact PCM and durable
/// history with its opaque result borrowed through the native host path.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn one_leaf_replay_qualified_media_commit_export_matches_uncached() -> Result {
    const FRAMES: u64 = 45;
    const SAMPLES: i64 = 72_072;
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("one-leaf qualified media evidence: {}", root.display());
    let (cached_fixture, plain_fixture, mut summary) =
        recipes::group_endpoints::aligned::one_leaf_replay_qualified_media_fixture(&root)?;
    check(&cached_fixture)?;
    check(&plain_fixture)?;
    if skip_without_physical_mac("one-leaf cached and uncached encoded Render") {
        return Ok(());
    }
    let mut evidence = Vec::new();
    for (label, fixture) in [("cached", &cached_fixture), ("uncached", &plain_fixture)] {
        let movie = render(fixture, &root.join(format!("exports-{label}")))?;
        let (report, passed) = verify(
            fixture,
            &movie,
            &fixture.revision,
            &["--every", "1", "--samples", "0:72072"],
        )?;
        fs::write(
            root.join(format!("one-leaf-{label}-verification.json")),
            serde_json::to_vec(&report)?,
        )?;
        check_provenance(fixture, &report)?;
        assert_eq!(report["summary"]["pictures_checked"], FRAMES);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        let audio = report["audio"].as_array().ok_or("complete decoded audio")?;
        assert_eq!(audio.len(), 1);
        assert_eq!(audio[0]["project_samples"], json!([0, SAMPLES]));
        assert_eq!(audio[0]["flags"], json!([]));
        assert_eq!(audio[0]["uncovered_samples"], 0);
        assert_eq!(audio[0]["blocks"]["low_snr_blocks"], 0);
        assert_eq!(audio[0]["offset_status"], "verified_zero");
        assert_eq!(audio[0]["measured_offset_samples"], 0);
        assert!(passed, "{label}: {report}");
        evidence.push(json!({"leg":label,"package":fixture.package,"movie":movie,"revision":fixture.revision,"verification":report}));
    }
    summary["exports"] = json!(evidence);
    fs::write(
        root.join("one-leaf-media-summary.json"),
        serde_json::to_vec(&summary)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        fs::write(path, serde_json::to_vec_pretty(&summary)?)?;
    }
    Ok(())
}
