//! One emitted MP4 checked against both retained Repeat and exploded revisions.
use super::*;
use std::fs;

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "run --release for encoded media qualification"
)]
fn targeted_explode_add_preserves_every_emitted_picture_and_sample() -> Result {
    run(false)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "run --release for encoded media qualification"
)]
fn targeted_explode_multiply_preserves_every_emitted_picture_and_sample() -> Result {
    run(true)
}

fn retained_verify(
    fixture: &Fixture,
    movie: &Path,
    revision: &str,
    samples: i64,
    root: &Path,
    label: &str,
) -> Result<(Value, bool)> {
    let range = format!("0:{samples}");
    let output = spawn(&[
        "verify-export",
        fixture.package.to_str().ok_or("UTF-8")?,
        "--movie",
        movie.to_str().ok_or("UTF-8")?,
        "--revision",
        revision,
        "--every",
        "1",
        "--samples",
        &range,
    ])?;
    fs::write(root.join(format!("{label}.stdout.json")), &output.stdout)?;
    fs::write(root.join(format!("{label}.stderr.txt")), &output.stderr)?;
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["passed"], output.status.success());
    fs::write(
        root.join(format!("{label}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok((report, output.status.success()))
}

fn complete_audio(report: &Value, samples: i64) -> Result {
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("complete audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["project_samples"], json!([0, samples]));
    assert_eq!(audio[0]["flags"], json!([]));
    assert_eq!(audio[0]["offset_status"], "verified_zero");
    assert_eq!(audio[0]["measured_offset_samples"], 0);
    assert_eq!(audio[0]["uncovered_samples"], 0);
    assert_eq!(audio[0]["blocks"]["low_snr_blocks"], 0);
    Ok(())
}

fn run(multiply: bool) -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("targeted Explode qualification: {}", root.display());
    let fixture = recipes::targeted_explode::build(&root.join("fixture"), multiply)?;
    check(&fixture)?;
    if skip_without_physical_mac("targeted Explode encoded Render") {
        return Ok(());
    }
    let evidence: Value = serde_json::from_slice(&fs::read(
        root.join("fixture/targeted-explode-evidence.json"),
    )?)?;
    let before_revision = evidence["before"]["revision_id"]
        .as_str()
        .ok_or("before revision")?;
    let samples = evidence["samples"].as_i64().ok_or("sample count")?;
    let started = std::time::Instant::now();
    let export = root.join("exports");
    fs::create_dir_all(&export)?;
    let output = spawn(&[
        "render",
        fixture.package.to_str().ok_or("UTF-8")?,
        "--output",
        export.to_str().ok_or("UTF-8")?,
        "--name",
        "targeted-explode.mp4",
        "--expected",
        &fixture.revision,
    ])?;
    fs::write(root.join("render.stdout.jsonl"), &output.stdout)?;
    fs::write(root.join("render.stderr.txt"), &output.stderr)?;
    assert!(
        output.status.success(),
        "render: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    let finished: Value = serde_json::from_str(stdout.lines().last().ok_or("render events")?)?;
    assert_eq!(finished["event"], "finished");
    assert_eq!(finished["status"]["outcome"], "published");
    assert_eq!(finished["status"]["captured_revision"], fixture.revision);
    let movie = PathBuf::from(
        finished["status"]["receipt"]["movie"]
            .as_str()
            .ok_or("movie")?,
    );
    let rendered = started.elapsed();
    let (report, passed) = retained_verify(
        &fixture,
        &movie,
        &fixture.revision,
        samples,
        &root,
        "exploded",
    )?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    complete_audio(&report, samples)?;
    // The same actual file must also match the immutable original Repeat.
    // This uses the application's shared renderer/DSP, not an independent
    // implementation oracle; exact pre/post plan and PCM witnesses are retained.
    let (before, before_passed) =
        retained_verify(&fixture, &movie, before_revision, samples, &root, "before")?;
    assert!(before_passed, "{before}");
    check_provenance(&fixture, &before)?;
    assert_eq!(before["summary"]["pictures_checked"], fixture.frames);
    assert_eq!(before["summary"]["pictures_failed"], 0);
    complete_audio(&before, samples)?;
    let negative_revision = recipes::targeted_explode::center_second(&fixture)?;
    let (negative, negative_passed) = retained_verify(
        &fixture,
        &movie,
        &negative_revision,
        samples,
        &root,
        "negative-centered-second",
    )?;
    assert!(
        !negative_passed,
        "a static center must not match the materialized live Follow"
    );
    assert!(
        negative["summary"]["pictures_failed"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
    complete_audio(&negative, samples)?;
    let mut summary = summary_row(&fixture, &report);
    summary["package"] = json!(fixture.package);
    summary["movie"] = json!(movie);
    summary["revision"] = json!(fixture.revision);
    summary["before_revision"] = json!(before_revision);
    summary["negative_revision"] = json!(negative_revision);
    summary["notes"] = json!(fixture.notes);
    summary["canonical_samples_compared"] = json!(samples);
    summary["full_authored_limited_and_limiter_bits_equal"] = json!(true);
    summary["same_movie_matches_before_and_exploded"] = json!(before_passed && passed);
    summary["static_second_center_negative_failed"] = json!(!negative_passed);
    summary["render_seconds"] = json!(rendered.as_secs_f64());
    summary["verification_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
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
