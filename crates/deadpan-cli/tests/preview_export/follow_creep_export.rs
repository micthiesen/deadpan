//! Emitted-file verification for animated Follow on a real A/V Original.

use super::*;
use std::fs;

fn retained_render(fixture: &Fixture, root: &Path) -> Result<PathBuf> {
    let output = root.join("exports");
    fs::create_dir_all(&output)?;
    let result = spawn(&[
        "render",
        fixture.package.to_str().ok_or("UTF-8")?,
        "--output",
        output.to_str().ok_or("UTF-8")?,
        "--name",
        "follow-creep.mp4",
        "--expected",
        &fixture.revision,
    ])?;
    fs::write(
        root.join("follow-creep-render.stdout.jsonl"),
        &result.stdout,
    )?;
    fs::write(root.join("follow-creep-render.stderr.txt"), &result.stderr)?;
    assert!(
        result.status.success(),
        "render: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout)?;
    let finished: Value = serde_json::from_str(stdout.lines().last().ok_or("render events")?)?;
    assert_eq!(finished["event"], "finished");
    assert_eq!(finished["status"]["outcome"], "published");
    assert_eq!(finished["status"]["captured_revision"], fixture.revision);
    Ok(PathBuf::from(
        finished["status"]["receipt"]["movie"]
            .as_str()
            .ok_or("published movie")?,
    ))
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn follow_creep_preserves_live_motion_lost_correction_fallback_and_verified_av() -> Result {
    run(false, false)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn ranged_follow_creep_preserves_prefix_live_motion_and_verified_complete_av() -> Result {
    run(true, false)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn retuned_follow_creep_preserves_live_motion_and_verified_av() -> Result {
    run(false, true)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn retuned_ranged_follow_creep_preserves_window_and_verified_av() -> Result {
    run(true, true)
}

fn run(ranged: bool, retuned: bool) -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("follow creep Render evidence: {}", root.display());
    let fixture = if retuned {
        recipes::follow_creep::build_retuned(&root.join("fixture"), ranged)?
    } else if ranged {
        recipes::follow_creep::build_ranged(&root.join("fixture"))?
    } else {
        recipes::follow_creep::build(&root.join("fixture"))?
    };
    check(&fixture)?;
    if skip_without_physical_mac("target-following creep encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = retained_render(&fixture, &root)?;
    let rendered = started.elapsed();
    let output = spawn(&[
        "verify-export",
        fixture.package.to_str().ok_or("UTF-8")?,
        "--movie",
        movie.to_str().ok_or("UTF-8")?,
        "--revision",
        &fixture.revision,
        "--every",
        "1",
        "--samples",
        "0:192192",
    ])?;
    fs::write(
        root.join("follow-creep-verification.stdout.json"),
        &output.stdout,
    )?;
    fs::write(
        root.join("follow-creep-verification.stderr.txt"),
        &output.stderr,
    )?;
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["passed"], output.status.success());
    // Preserve the actual report and raw streams before asserting its content.
    fs::write(
        root.join("follow-creep-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    check_provenance(&fixture, &report)?;
    assert_eq!(
        report["summary"]["pictures_checked"],
        recipes::follow_creep::FRAMES
    );
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("full canonical audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(
        audio[0]["project_samples"],
        json!([0, recipes::follow_creep::SAMPLES])
    );
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
        "before":root.join("fixture/follow-creep-before-pcm.json"),
        "after":root.join("fixture/follow-creep-after-pcm.json"),
        "history_and_independent_pose_oracle":root.join("fixture/follow-creep-authored-evidence.json"),
        "portable_copy":root.join("fixture/follow-creep-portable-copy.json"),
    });
    row["canonical_samples_compared"] = json!(recipes::follow_creep::SAMPLES);
    row["canonical_channels"] = json!(2);
    row["authored_limited_and_limiter_gain_bits_equal"] = json!(true);
    row["independent_pose_oracle"] = json!(true);
    row["independent_DSP_claim"] = json!(false);
    row["tracking_accuracy_claim"] = json!(false);
    row["listening_claim"] = json!(false);
    fs::write(
        root.join("follow-creep-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    assert!(output.status.success(), "{report}");
    Ok(())
}
