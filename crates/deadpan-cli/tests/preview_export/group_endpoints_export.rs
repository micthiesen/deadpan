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
