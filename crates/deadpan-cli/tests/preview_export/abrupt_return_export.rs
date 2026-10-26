//! Real emitted-file proof of chosen-boundary return from existing paths.
use super::*;
use std::fs;

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "run --release for encoded media qualification"
)]
fn abrupt_return_existing_retained_envelope_preserves_prefix_and_verified_av() -> Result {
    run(false)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "run --release for encoded media qualification"
)]
fn abrupt_return_existing_retained_follow_preserves_prefix_and_verified_av() -> Result {
    run(true)
}

fn run(follow: bool) -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("existing-path abrupt return evidence: {}", root.display());
    let fixture = recipes::abrupt_return::build(&root.join("fixture"), follow)?;
    check(&fixture)?;
    if skip_without_physical_mac("existing-path abrupt return encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(
        &fixture,
        &movie,
        &fixture.revision,
        &["--every", "1", "--samples", "0:192192"],
    )?;
    fs::write(
        root.join("verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 120);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("complete audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["project_samples"], json!([0, 192192]));
    assert_eq!(audio[0]["flags"], json!([]));
    assert_eq!(audio[0]["offset_status"], "verified_zero");
    assert_eq!(audio[0]["measured_offset_samples"], 0);
    assert_eq!(audio[0]["uncovered_samples"], 0);
    assert_eq!(audio[0]["blocks"]["low_snr_blocks"], 0);
    // The already-verified source path must be observably different after the
    // seam. Testing the same movie against the pre-reset revision is an
    // expected mismatch, not another export or an implementation oracle.
    let evidence: Value =
        serde_json::from_slice(&fs::read(root.join("fixture/abrupt-return-evidence.json"))?)?;
    let previous = evidence["divided"]["revision_id"]
        .as_str()
        .ok_or("split revision")?;
    let (negative, negative_passed) = verify(
        &fixture,
        &movie,
        previous,
        &["--every", "1", "--samples", "0:192192"],
    )?;
    fs::write(
        root.join("negative-old-path.json"),
        serde_json::to_vec_pretty(&negative)?,
    )?;
    assert!(
        !negative_passed,
        "old path must not match the returned suffix"
    );
    assert!(
        negative["summary"]["pictures_failed"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
    let mut summary = summary_row(&fixture, &report);
    summary["package"] = json!(fixture.package);
    summary["movie"] = json!(movie);
    summary["revision"] = json!(fixture.revision);
    summary["notes"] = json!(fixture.notes);
    summary["seam"] = json!(recipes::abrupt_return::SEAM);
    summary["canonical_samples_compared"] = json!(192192);
    summary["canonical_pcm_and_limiter_bits_equal"] = json!(true);
    summary["negative_old_path_mismatch"] = json!(!negative_passed);
    summary["independent_DSP_claim"] = json!(false);
    summary["tracking_accuracy_claim"] = json!(false);
    summary["listening_claim"] = json!(false);
    fs::write(
        root.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    eprintln!("{summary}");
    Ok(())
}
