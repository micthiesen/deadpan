#![cfg(target_os = "macos")]

#[path = "preview_export/recipes.rs"]
mod recipes;

use recipes::{Expected, Fixture, Result, success};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

fn ratio(value: &Value) -> Result<(i128, i128)> {
    let part = |key: &str| -> Result<i128> {
        Ok(value[key]
            .as_str()
            .ok_or_else(|| format!("{key} of {value}"))?
            .parse()?)
    };
    Ok((part("numerator")?, part("denominator")?))
}

/// Source ticks per Original frame; `cfr-bframes.mp4` is CFR from zero.
const TICKS_PER_FRAME: i128 = 1001;

/// The decoded Original ordinal a plan picture selects, by the plan's rule:
/// the frame whose half-open PTS interval contains the point, with an
/// adjacent-held point past the selection showing its last selected frame.
fn plan_picture(picture: &Value) -> Result<Expected> {
    let ordinal_of = |point: &Value| -> Result<i128> {
        assert_eq!(point["time_base"]["numerator"], 1);
        assert_eq!(point["time_base"]["denominator"], 30_000);
        let (numerator, denominator) = ratio(&point["ticks"])?;
        Ok(numerator.div_euclid(denominator * TICKS_PER_FRAME))
    };
    Ok(match picture["type"].as_str().ok_or("picture type")? {
        "source" => {
            let mut ordinal = ordinal_of(&picture["point"])?;
            let (end, end_denominator) = ratio(&picture["selection"]["end"]["ticks"])?;
            let last = (end - 1).div_euclid(end_denominator * TICKS_PER_FRAME);
            let first = ordinal_of(&picture["selection"]["start"])?;
            assert_eq!(picture["endpoints"], "hold_adjacent");
            ordinal = ordinal.clamp(first, last);
            Expected::Original {
                source_ordinal: u64::try_from(ordinal)?,
            }
        }
        "freeze" => Expected::Original {
            source_ordinal: u64::try_from(ordinal_of(&picture["point"])?)?,
        },
        "background" => Expected::Background,
        "accepted" => Expected::Generated {
            sampled_frame: picture["frame"].as_u64().ok_or("accepted frame")?,
        },
        other => return Err(format!("unexpected plan picture {other}").into()),
    })
}

fn check(fixture: &Fixture) -> Result {
    check_with_picture(fixture, plan_picture)
}

fn check_with_picture(fixture: &Fixture, picture: impl Fn(&Value) -> Result<Expected>) -> Result {
    let path = fixture.package.to_str().ok_or("UTF-8 package")?;
    success(&["project", "validate", path])?;
    let plan = success(&["inspect-plan", path])?;
    assert!(!fixture.section8_rows.is_empty(), "{}", fixture.name);
    for (frame, expected) in &fixture.expectations {
        let sample = success(&["inspect-plan", path, "--frame", &frame.to_string()])?;
        assert_eq!(sample["sample"]["revision_id"], fixture.revision.as_str());
        let actual = picture(&sample["sample"]["picture"])?;
        assert_eq!(
            actual, *expected,
            "{} frame {frame}: {}",
            fixture.name, sample["sample"]["picture"]
        );
    }
    for (frame, expected) in &fixture.captions {
        let sample = success(&["inspect-plan", path, "--frame", &frame.to_string()])?;
        let shown: Vec<&str> = sample["sample"]["captions"]
            .as_array()
            .map(|captions| {
                captions
                    .iter()
                    .filter_map(|caption| caption["text"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            &shown, expected,
            "{} captions at frame {frame}",
            fixture.name
        );
    }
    for (start, loud) in &fixture.audio {
        let end = (start + 256).to_string();
        let audio = success(&[
            "inspect-audio",
            path,
            "--samples",
            &start.to_string(),
            &end,
            "--limited",
        ])?;
        assert_eq!(audio["audio"]["revision_id"], fixture.revision.as_str());
        let peak = audio["audio"]["samples"]
            .as_array()
            .ok_or("limited samples")?
            .iter()
            .flat_map(|pair| pair.as_array().into_iter().flatten())
            .filter_map(Value::as_f64)
            .fold(0.0_f64, |peak, value| peak.max(value.abs()));
        assert!(
            if *loud { peak > 0.5 } else { peak < 0.01 },
            "{} audio at {start}: peak {peak}, expected {}",
            fixture.name,
            if *loud { "loud" } else { "quiet" }
        );
    }
    for signal in &fixture.signals {
        // Limited inspection reads at most 256 samples per request.
        let mut left: Vec<f64> = Vec::new();
        let mut at = signal.start;
        while at < signal.start + signal.count {
            let end = (at + 256).min(signal.start + signal.count);
            let audio = success(&[
                "inspect-audio",
                path,
                "--samples",
                &at.to_string(),
                &end.to_string(),
                "--limited",
            ])?;
            left.extend(
                audio["audio"]["samples"]
                    .as_array()
                    .ok_or("limited samples")?
                    .iter()
                    .filter_map(|pair| pair.get(0).and_then(Value::as_f64)),
            );
            at = end;
        }
        assert_eq!(left.len() as i64, signal.count, "{}", fixture.name);
        let crossings = left
            .windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count() as f64
            * 48_000.0
            / signal.count as f64;
        let peak = left
            .iter()
            .fold(0.0_f64, |peak, value| peak.max(value.abs()));
        let rms = (left.iter().map(|value| value * value).sum::<f64>() / left.len() as f64).sqrt();
        let crest = peak / rms;
        if let Some((expected, tolerance)) = signal.crossings_per_second {
            assert!(
                (crossings / expected - 1.0).abs() <= tolerance,
                "{} at {}: {crossings} crossings/s, expected {expected}",
                fixture.name,
                signal.start
            );
        }
        if let Some((low, high)) = signal.crest {
            assert!(
                crest > low && crest < high,
                "{} at {}: crest {crest}, expected ({low}, {high})",
                fixture.name,
                signal.start
            );
        }
        if let Some((low, high)) = signal.peak {
            assert!(
                peak > low && peak < high,
                "{} at {}: peak {peak}, expected ({low}, {high})",
                fixture.name,
                signal.start
            );
        }
    }
    let past = fixture.frames.to_string();
    assert!(
        !recipes::cli(&["inspect-plan", path, "--frame", &past])?
            .status
            .success(),
        "{} has exactly {} frames: {plan}",
        fixture.name,
        fixture.frames
    );
    Ok(())
}

#[test]
fn every_recipe_builds_a_valid_package_whose_plan_matches_its_expectations() -> Result {
    let scratch = tempfile::tempdir()?;
    // DEADPAN_PREVIEW_EXPORT_KEEP=1 retains the packages for inspection.
    let root = if std::env::var_os("DEADPAN_PREVIEW_EXPORT_KEEP").is_some() {
        scratch.keep()
    } else {
        scratch.path().to_owned()
    };
    for fixture in recipes::all(&root)? {
        eprintln!(
            "{}: {} frames at {} {:?} {}",
            fixture.name,
            fixture.frames,
            fixture.revision,
            fixture.notes,
            fixture.package.display()
        );
        check(&fixture)?;
    }
    Ok(())
}

fn spawn(arguments: &[&str]) -> Result<Output> {
    let mut command = Command::new(recipes::cli_path());
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(deadpan_native_process::spawn(&mut command)?.wait_with_output()?)
}

/// Export the fixture's exact committed revision through the public headless
/// Render path and return the published movie.
fn render(fixture: &Fixture, output: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output)?;
    let name = format!("{}.mp4", fixture.name);
    let result = spawn(&[
        "render",
        fixture.package.to_str().ok_or("UTF-8")?,
        "--output",
        output.to_str().ok_or("UTF-8")?,
        "--name",
        &name,
        "--expected",
        &fixture.revision,
    ])?;
    let stdout = String::from_utf8(result.stdout)?;
    assert!(
        result.status.success(),
        "render {}: {}\n{}",
        fixture.name,
        String::from_utf8_lossy(&result.stderr),
        stdout.lines().last().unwrap_or_default()
    );
    let finished: Value = serde_json::from_str(stdout.lines().last().ok_or("render events")?)?;
    assert_eq!(finished["event"], "finished");
    assert_eq!(finished["status"]["outcome"], "published");
    assert_eq!(
        finished["status"]["captured_revision"],
        fixture.revision.as_str()
    );
    Ok(PathBuf::from(
        finished["status"]["receipt"]["movie"]
            .as_str()
            .ok_or("published movie")?,
    ))
}

/// Run `verify-export`; returns the report and whether the command passed.
fn verify(
    fixture: &Fixture,
    movie: &Path,
    revision: &str,
    extra: &[&str],
) -> Result<(Value, bool)> {
    let mut arguments = vec![
        "verify-export",
        fixture.package.to_str().ok_or("UTF-8")?,
        "--movie",
        movie.to_str().ok_or("UTF-8")?,
        "--revision",
        revision,
    ];
    arguments.extend_from_slice(extra);
    let output = spawn(&arguments)?;
    let report: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "verify-export {}: {error}: {}",
            fixture.name,
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    assert_eq!(report["passed"], output.status.success());
    if !output.status.success() {
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["error"]["code"], "ExportVerificationMismatch");
    }
    Ok((report, output.status.success()))
}

/// The exported pictures carry the recipe's independently derived provenance.
fn check_provenance(fixture: &Fixture, report: &Value) -> Result {
    let pictures = report["pictures"].as_array().ok_or("pictures")?;
    for (frame, expected) in &fixture.expectations {
        let picture = pictures
            .iter()
            .find(|picture| picture["output_frame"] == *frame)
            .ok_or_else(|| format!("{} frame {frame} was not compared", fixture.name))?;
        let provenance = &picture["provenance"];
        let actual = match provenance["kind"].as_str() {
            Some("original") => Expected::Original {
                source_ordinal: provenance["source_frame"].as_u64().ok_or("ordinal")?,
            },
            Some("background") => Expected::Background,
            Some("generated") => Expected::Generated {
                sampled_frame: provenance["source_frame"].as_u64().ok_or("sampled frame")?,
            },
            _ => return Err(format!("unknown provenance {provenance}").into()),
        };
        assert_eq!(actual, *expected, "{} frame {frame}", fixture.name);
    }
    Ok(())
}

fn summary_row(fixture: &Fixture, report: &Value) -> Value {
    let summary = &report["summary"];
    json!({
        "fixture": fixture.name,
        "frames": fixture.frames,
        "rows": fixture.section8_rows,
        "passed": report["passed"],
        "pictures": summary["pictures_checked"],
        "min_luma_psnr_db": summary["min_luma_psnr_db"],
        "min_chroma_psnr_db": summary["min_chroma_psnr_db"],
        "max_thumbnail_mad": summary["max_thumbnail_mad"],
        "audio_windows": summary["audio_windows_checked"],
        "signal_windows": summary["signal_windows"],
        "min_neighbor_margin_db": summary["min_neighbor_margin_db"],
        "blocks_compared": summary["blocks_compared"],
        "min_block_snr_db": summary["min_block_snr_db"],
        "max_block_level_db": summary["max_block_level_db"],
        "offsets": report["audio"].as_array().map(|windows| windows
            .iter()
            .map(|window| json!([window["offset_status"], window["measured_offset_samples"]]))
            .collect::<Vec<_>>()),
        "declared_audio_priming": report["movie"]["audio_edits"]["media_time"],
        "failures": report["failures"],
    })
}

fn keep_or(scratch: tempfile::TempDir) -> (PathBuf, Option<tempfile::TempDir>) {
    if std::env::var_os("DEADPAN_PREVIEW_EXPORT_KEEP").is_some() {
        (scratch.keep(), None)
    } else {
        (scratch.path().to_owned(), Some(scratch))
    }
}

/// Container aperture admission must reach the same retained source, project
/// basis, composed pictures and encoded output without changing either clock.
#[test]
fn clean_aperture_original_renders_at_its_visible_size_without_av_changes() -> Result {
    aperture_render(false)
}

#[test]
fn fractional_clean_aperture_renders_at_its_visible_size_without_av_changes() -> Result {
    aperture_render(true)
}

fn aperture_render(fractional: bool) -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!(
        "clean aperture Render evidence (fractional={fractional}): {}",
        root.display()
    );
    let fixture = if fractional {
        recipes::fractional_aperture_original(&root.join("fixture"))?
    } else {
        recipes::aperture_original(&root.join("fixture"))?
    };
    check(&fixture)?;
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    std::fs::write(
        root.join("verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    assert!(passed, "{}", report["failures"]);
    assert_eq!(report["raster"], json!([300, 160]));
    assert_eq!(report["range"], json!([0, 120]));
    assert_eq!(report["frame_rate"], json!([30000, 1001]));
    assert_eq!(report["movie"]["expected_audio_samples"], 192_192);
    assert_eq!(report["movie"]["audio"]["presented_end"], 192_192);
    assert_eq!(report["summary"]["pictures_checked"], 120);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    assert!(report["summary"]["signal_windows"].as_u64() > Some(0));
    check_provenance(&fixture, &report)?;
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn interlaced_originals_render_progressive_at_the_automatic_field_cadence() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("interlaced Render evidence: {}", root.display());
    for (name, frames, step, rate, samples) in [
        ("fields-tff", 24, 1, [50, 1], 23040),
        ("fields-bff", 24, 1, [60000, 1001], 19219),
        ("fields-single", 2, 1, [50, 1], 1920),
        ("fields-100", 12, 2, [50, 1], 11520),
        ("telecine-tff", 30, 1, [60000, 1001], 24024),
        ("telecine-bff", 30, 1, [60000, 1001], 24024),
        ("telecine-progressive", 30, 1, [60000, 1001], 24024),
        ("telecine-single", 3, 1, [60000, 1001], 2402),
    ] {
        let fixture = recipes::interlaced_original(&root.join(name), name, frames, step)?;
        let movie = render(&fixture, &root.join(name).join("exports"))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        std::fs::write(
            root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert!(passed, "{name}: {}", report["failures"]);
        assert_eq!(report["raster"], json!([96, 64]));
        assert_eq!(report["range"], json!([0, frames]));
        assert_eq!(report["frame_rate"], json!(rate));
        assert_eq!(report["movie"]["expected_audio_samples"], samples);
        assert_eq!(report["movie"]["audio"]["presented_end"], samples);
        assert_eq!(report["summary"]["pictures_checked"], frames);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
    }
    Ok(())
}

#[test]
fn vfr_recipe_plans_match_independent_source_clocks() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("VFR recipe plan evidence: {}", root.display());
    for fixture in recipes::vfr::matrix(&root)? {
        eprintln!(
            "{}: {} frames at {}",
            fixture.name, fixture.frames, fixture.revision
        );
        check_with_picture(&fixture, recipes::vfr::plan_picture)?;
    }
    Ok(())
}

#[test]
fn sdr_codec_originals_render_sdr_with_exact_picture_and_audio_timing() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("SDR codec Render evidence: {}", root.display());
    for name in [
        "hevc-sdr-8-limited",
        "hevc-sdr-8-full",
        "hevc-sdr-10-limited",
        "hevc-sdr-10-full",
        "h264-sdr-10-limited",
        "h264-sdr-10-full",
        "vp9-sdr-8-limited",
        "vp9-sdr-8-full",
        "vp9-sdr-10-limited",
        "vp9-sdr-10-full",
    ] {
        let fixture = recipes::sdr_codec_original(&root.join(name), name)?;
        let movie = render(&fixture, &root.join(name).join("exports"))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        std::fs::write(
            root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert!(passed, "{name}: {}", report["failures"]);
        assert_eq!(report["frame_rate"], json!([30000, 1001]));
        assert_eq!(report["range"], json!([0, 12]));
        assert_eq!(report["output_color"]["output"], "sdr_rec709");
        assert_eq!(report["output_color"]["hdr_sources"], false);
        assert_eq!(report["movie"]["color"]["container"], json!([1, 1, 1]));
        assert_eq!(report["movie"]["color"]["container_full_range"], false);
        assert_eq!(report["movie"]["color"]["problems"], json!([]));
        assert_eq!(report["summary"]["pictures_checked"], 12);
        assert_eq!(report["summary"]["index_observable_comparisons"], 22);
        assert_eq!(report["summary"]["index_unobservable_comparisons"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert_eq!(report["movie"]["audio"]["presented_end"], 19219);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
    }
    Ok(())
}

#[test]
fn webm_originals_render_the_measured_clock_and_sample_aspect() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("WebM Render evidence: {}", root.display());
    for (name, count) in [
        ("vp9-sdr-8-limited.webm", 12),
        ("vp9-sdr-8-full.webm", 12),
        ("vp9-sdr-10-limited.webm", 12),
        ("vp9-sdr-10-full.webm", 12),
        ("vp9-sdr-8-limited.mkv", 12),
        ("vp9-anamorphic.webm", 12),
        ("vp9-altref.webm", 60),
        ("vp9-existing-8.webm", 2),
        ("vp9-existing-10.webm", 2),
        ("vp9-vfr.webm", 12),
    ] {
        let fixture = recipes::webm_original(&root.join(name), name, count)?;
        let movie = render(&fixture, &root.join(name).join("exports"))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        std::fs::write(
            root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert!(passed, "{name}: {}", report["failures"]);
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert_eq!(report["output_color"]["output"], "sdr_rec709");
        assert_eq!(
            report["frame_rate"],
            if name == "vp9-vfr.webm" {
                json!([50, 1])
            } else {
                json!([30000, 1001])
            }
        );
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
    }
    Ok(())
}

#[test]
fn prores_originals_render_ten_bit_422_vfr_aspect_and_both_field_orders() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("ProRes Render evidence: {}", root.display());
    for name in [
        "proxy",
        "lt",
        "standard",
        "hq",
        "anamorphic",
        "vfr",
        "tff",
        "bff",
        "apple-hq",
    ] {
        let fixture = recipes::prores_original(&root.join(name), name)?;
        let movie = render(&fixture, &root.join(name).join("exports"))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        std::fs::write(
            root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert!(passed, "{name}: {}", report["failures"]);
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert_eq!(report["output_color"]["output"], "sdr_rec709");
        assert_eq!(report["output_color"]["hdr_sources"], false);
        assert_eq!(report["movie"]["audio"]["presented_end"], 19219);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
    }
    Ok(())
}

#[test]
fn av1_originals_render_sdr_hdr_grain_superres_and_variable_timing() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("AV1 Render evidence: {}", root.display());
    for name in [
        "av1-sdr-8-limited.mp4",
        "av1-sdr-8-full.mp4",
        "av1-sdr-10-limited.mp4",
        "av1-sdr-10-full.mp4",
        "av1-anamorphic.mp4",
        "av1-vfr.mp4",
        "av1-multigop.mp4",
        "av1-grain.mp4",
        "av1-superres.mp4",
        "av1-no-config-sequence.mp4",
        "av1-pq.mp4",
        "av1-hlg.mp4",
        "av1-pq-static-config.mp4",
        "av1-pq-static-packet.mp4",
        "av1-sdr-8-limited.webm",
        "av1-sdr-10-full.webm",
        "av1-anamorphic.webm",
        "av1-vfr.webm",
        "av1-grain.webm",
        "av1-pq.webm",
    ] {
        let fixture = recipes::av1_original(&root.join(name), name)?;
        let movie = render(&fixture, &root.join(name).join("exports"))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        std::fs::write(
            root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert!(passed, "{name}: {}", report["failures"]);
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert_eq!(
            report["output_color"]["hdr_sources"],
            name.contains("pq") || name.contains("hlg")
        );
        assert_eq!(report["frame_rate"], json!([30000, 1001]));
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
    }
    Ok(())
}

#[test]
fn opus_webm_originals_render_with_exact_source_samples_and_verified_encoded_audio() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("Opus Render evidence: {}", root.display());
    for name in [
        "opus-av",
        "opus-av-offset",
        "opus-av-audio-first",
        "opus-av-multi",
    ] {
        let fixture = recipes::opus_original(&root.join(name), name)?;
        let movie = render(&fixture, &root.join(name).join("exports"))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        std::fs::write(
            root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert!(passed, "{name}: {}", report["failures"]);
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert!(report["summary"]["signal_windows"].as_u64() > Some(0));
        assert_eq!(report["frame_rate"], json!([30000, 1001]));
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
    }
    Ok(())
}

#[test]
fn vp9_reference_pictures_render_without_adding_or_losing_timeline_frames() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("VP9 reference Render evidence: {}", root.display());
    for (name, frames) in [
        ("vp9-altref", 60),
        ("vp9-existing-8", 2),
        ("vp9-existing-10", 2),
    ] {
        let fixture = recipes::sdr_source_original(&root.join(name), name, frames)?;
        let movie = render(&fixture, &root.join(name).join("exports"))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        std::fs::write(
            root.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert!(passed, "{name}: {}", report["failures"]);
        assert_eq!(report["range"], json!([0, frames]));
        assert_eq!(report["summary"]["pictures_checked"], frames);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert_eq!(report["output_color"]["output"], "sdr_rec709");
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
    }
    Ok(())
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "real eleven-recipe matrix; run with --release"
)]
fn vfr_recipe_exports_match_independent_provenance_and_committed_preview() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("VFR recipe export evidence: {}", root.display());
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    for fixture in recipes::vfr::matrix(&root.join("fixtures"))? {
        check_with_picture(&fixture, recipes::vfr::plan_picture)?;
        let started = std::time::Instant::now();
        let movie = render(&fixture, &root.join("exports"))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        std::fs::write(
            root.join(format!("{}-verification.json", fixture.name)),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["frame_rate"], json!([30000, 1001]));
        check_provenance(&fixture, &report)?;
        let mut row = summary_row(&fixture, &report);
        row["seconds"] = json!(started.elapsed().as_secs_f64());
        eprintln!("{row}");
        if !passed {
            failures.push(fixture.name);
        }
        rows.push(row);
    }
    std::fs::write(root.join("results.json"), serde_json::to_vec_pretty(&rows)?)?;
    assert!(failures.is_empty(), "VFR export failures: {failures:?}");
    Ok(())
}

/// Default-gate end-to-end check: one real export of a Section 8 recipe
/// matches its committed preview, and a later same-duration reframe and gain
/// change of that revision is reported against the old movie.
#[test]
fn black_pause_export_matches_preview_and_later_edits_are_reported() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::black_pause(&root.join("fixture"))?;
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{}", report["failures"]);
    assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
    check_provenance(&fixture, &report)?;
    assert_eq!(report["movie"]["audio_edits"]["media_time"], 2048);
    assert_eq!(report["movie"]["audio"]["first_pts"], -2048);
    assert_eq!(report["movie"]["color"]["problems"], json!([]));
    assert!(report["summary"]["signal_windows"].as_u64() > Some(0));
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    for window in report["audio"].as_array().ok_or("audio")? {
        if window["kind"] == "signal" {
            assert_eq!(window["offset_status"], "verified_zero", "{window}");
            assert_eq!(window["measured_offset_samples"], 0, "{window}");
        }
    }
    eprintln!("{}", summary_row(&fixture, &report));

    // Edit 29 shows the Original click frame after the 12-frame pause.
    let changed = recipes::reframe_and_attenuate(&fixture, 29)?;
    let (stale, passed) = verify(
        &fixture,
        &movie,
        &changed,
        &["--frames", "10,20,29,35", "--samples", "40000:64000"],
    )?;
    assert!(!passed);
    let flags = |frame: u64| -> Vec<Value> {
        stale["pictures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|picture| picture["output_frame"] == frame)
            .unwrap()["flags"]
            .as_array()
            .unwrap()
            .clone()
    };
    assert!(flags(10).is_empty() && flags(20).is_empty(), "{stale}");
    for frame in [29, 35] {
        assert!(
            flags(frame).contains(&json!("gross_structural_mismatch")),
            "{stale}"
        );
    }
    let audio = &stale["audio"][0];
    assert_eq!(audio["kind"], "signal");
    assert_eq!(audio["measured_offset_samples"], 0, "{audio}");
    assert!(
        audio["flags"]
            .as_array()
            .unwrap()
            .contains(&json!("audio_level")),
        "{audio}"
    );

    // Encoder-fault negatives on altered copies of the verified movie.
    let faults = root.join("faults");
    std::fs::create_dir_all(&faults)?;
    let flags_of = |report: &Value| -> String { report["failures"].to_string() };
    // Audio 2,048 samples late: the edit no longer skips AAC priming and preroll. The
    // priming cross-check agrees with the altered edit (it is self-referential);
    // the content alignment must catch the shift.
    let late = faults.join("audio-late.mp4");
    std::fs::write(&late, patch_edit(&std::fs::read(&movie)?, *b"soun", 0)?)?;
    let (report, passed) = verify(&fixture, &late, &fixture.revision, &[])?;
    assert!(!passed);
    assert_eq!(
        report["movie"]["audio"]["first_pts"],
        0,
        "{}",
        flags_of(&report)
    );
    assert_eq!(
        report["summary"]["nonzero_offsets"],
        1,
        "{}",
        flags_of(&report)
    );
    assert_eq!(report["audio"][0]["offset_status"], "offset");
    assert_eq!(report["audio"][0]["measured_offset_samples"], 2048);
    // Pictures one frame early, as a dropped leading frame would appear: the
    // decoder discards the picture before the edit, every later picture lands
    // on the previous ordinal and the last ordinal is missing.
    let early = faults.join("video-early.mp4");
    let frame_ticks = 1001;
    std::fs::write(
        &early,
        patch_edit(&std::fs::read(&movie)?, *b"vide", frame_ticks)?,
    )?;
    let (report, passed) = verify(&fixture, &early, &fixture.revision, &[])?;
    assert!(!passed);
    let text = flags_of(&report);
    assert_eq!(
        report["movie"]["video_frames"],
        fixture.frames - 1,
        "{text}"
    );
    assert!(text.contains("missing at the end"), "{text}");
    assert!(text.contains("frame_index_mismatch"), "{text}");
    // The same movie against the pre-pause revision: a different range.
    let (report, passed) = verify(&fixture, &movie, "black-pause-r02", &[])?;
    assert!(!passed);
    let text = flags_of(&report);
    assert!(text.contains("beyond the committed 30 frames"), "{text}");
    assert!(text.contains("audio edit presents"), "{text}");
    Ok(())
}

/// Rewrite the media time of the first edit of the track whose handler is
/// `handler` ("vide" or "soun") in an MP4 byte image.
fn patch_edit(bytes: &[u8], handler: [u8; 4], media_time: i64) -> Result<Vec<u8>> {
    fn boxes(bytes: &[u8], start: usize, end: usize) -> Vec<(usize, usize, [u8; 4])> {
        let mut found = Vec::new();
        let mut at = start;
        while at + 8 <= end {
            let size = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = bytes[at + 4..at + 8].try_into().unwrap();
            if size < 8 || at + size > end {
                break;
            }
            found.push((at, at + size, kind));
            at += size;
        }
        found
    }
    let find = |start: usize, end: usize, kind: &[u8; 4]| {
        boxes(bytes, start, end)
            .into_iter()
            .find(|entry| &entry.2 == kind)
            .map(|(start, end, _)| (start, end))
    };
    let mut patched = bytes.to_vec();
    let (moov_start, moov_end) = find(0, bytes.len(), b"moov").ok_or("moov")?;
    for (trak_start, trak_end, kind) in boxes(bytes, moov_start + 8, moov_end) {
        if &kind != b"trak" {
            continue;
        }
        let (mdia_start, mdia_end) = find(trak_start + 8, trak_end, b"mdia").ok_or("mdia")?;
        let (hdlr_start, _) = find(mdia_start + 8, mdia_end, b"hdlr").ok_or("hdlr")?;
        // hdlr: header, version/flags, pre_defined, then handler type.
        if bytes[hdlr_start + 16..hdlr_start + 20] != handler {
            continue;
        }
        let (edts_start, edts_end) = find(trak_start + 8, trak_end, b"edts").ok_or("edts")?;
        let (elst_start, _) = find(edts_start + 8, edts_end, b"elst").ok_or("elst")?;
        let version = bytes[elst_start + 8];
        let entry = elst_start + 16;
        if version == 1 {
            patched[entry + 8..entry + 16].copy_from_slice(&media_time.to_be_bytes());
        } else {
            patched[entry + 4..entry + 8]
                .copy_from_slice(&i32::try_from(media_time)?.to_be_bytes());
        }
        return Ok(patched);
    }
    Err("track not found".into())
}

/// Every Section 8 recipe fixture exported through public Render and
/// verified against its committed preview. Debug renders take about four
/// seconds per output second, so this runs in optimized test builds:
/// `cargo test --release --locked -p deadpan-cli --test preview_export`.
/// DEADPAN_PREVIEW_EXPORT_RESULTS=<new.json> records the measured table.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn every_recipe_export_matches_its_committed_preview() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    let (fixtures, skipped) = recipes::all_with_skips(&root.join("fixtures"))?;
    for (name, reason) in &skipped {
        eprintln!("SKIPPED fixture {name}: {reason}");
        rows.push(json!({"fixture": name, "skipped": reason}));
    }
    for fixture in fixtures {
        let started = std::time::Instant::now();
        let movie = render(&fixture, &root.join("exports"))?;
        let rendered = started.elapsed();
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
        check_provenance(&fixture, &report)?;
        let mut row = summary_row(&fixture, &report);
        row["render_seconds"] = json!(rendered.as_secs_f64());
        row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
        eprintln!("{row}");
        if !passed {
            failed.push(fixture.name);
        }
        rows.push(row);
        if fixture.name == "generated-pause" {
            // Against the revision before acceptance (the Background Hold
            // the generated pictures replaced), exactly the Hold differs.
            let (stale, passed) = verify(
                &fixture,
                &movie,
                "generated-pause-r03",
                &["--frames", "14,15,20,26,27", "--no-audio"],
            )?;
            assert!(!passed, "{stale}");
            for picture in stale["pictures"].as_array().ok_or("pictures")? {
                let frame = picture["output_frame"].as_u64().ok_or("frame")?;
                let flagged = picture["flags"].as_array().is_some_and(|f| !f.is_empty());
                assert_eq!(
                    flagged,
                    (15..27).contains(&frame),
                    "generated-pause frame {frame} against Background: {picture}"
                );
            }
        }
        if fixture.name == "delayed-caption" {
            // The exported pictures carry the caption: against a revision
            // without it, only the captioned frames differ.
            let cleared = recipes::clear_captions(&fixture, 0)?;
            let (stale, passed) = verify(
                &fixture,
                &movie,
                &cleared,
                &["--frames", "2,8,20,30", "--no-audio"],
            )?;
            let flagged = |frame: u64| -> bool {
                stale["pictures"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|picture| picture["output_frame"] == frame)
                    .is_some_and(|picture| {
                        picture["flags"]
                            .as_array()
                            .is_some_and(|flags| !flags.is_empty())
                    })
            };
            assert!(!passed, "{stale}");
            assert!(
                flagged(8),
                "captioned frame 8 matches an uncaptioned reference: {stale}"
            );
            for frame in [2, 20, 30] {
                assert!(
                    !flagged(frame),
                    "frame {frame} changed without a caption edit: {stale}"
                );
            }
        }
    }
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
    }
    assert!(failed.is_empty(), "{failed:?}: {}", Value::Array(rows));
    Ok(())
}
