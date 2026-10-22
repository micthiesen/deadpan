#![cfg(target_os = "macos")]

#[path = "support/physical_mac.rs"]
mod physical_mac;
#[path = "preview_export/recipes.rs"]
mod recipes;

use physical_mac::skip_without_physical_mac;
use recipes::{Expected, Fixture, Result, success};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn node_sets_export_preserves_discontiguous_union_shared_mark_and_complete_audio() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("node sets Render evidence: {}", root.display());
    let fixture = recipes::node_sets::build(&root.join("fixture"))?;
    check(&fixture)?;
    if skip_without_physical_mac("discontiguous node union encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = render(&fixture, &root.join("exports"))?;
    let rendered = started.elapsed();
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 32);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("full canonical audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["project_samples"], json!([0, 51251]));
    for window in audio {
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["uncovered_samples"], 0);
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    assert!(passed, "{report}");
    let mut row = summary_row(&fixture, &report);
    row["render_seconds"] = json!(rendered.as_secs_f64());
    row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
    row["movie"] = json!(movie);
    row["package"] = json!(fixture.package);
    row["revision"] = json!(fixture.revision);
    row["notes"] = json!(fixture.notes);
    std::fs::write(
        root.join("node-sets-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(
        root.join("node-sets-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    Ok(())
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn range_sets_export_preserves_same_source_windows_and_complete_audio() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("range sets Render evidence: {}", root.display());
    let fixture = recipes::range_sets::build(&root.join("fixture"))?;
    check(&fixture)?;
    if skip_without_physical_mac("discontiguous range union encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = render(&fixture, &root.join("exports"))?;
    let rendered = started.elapsed();
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 30);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("full canonical audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["project_samples"], json!([0, 48048]));
    for window in audio {
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["uncovered_samples"], 0);
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    assert!(passed, "{report}");
    let mut row = summary_row(&fixture, &report);
    row["render_seconds"] = json!(rendered.as_secs_f64());
    row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
    row["movie"] = json!(movie);
    row["package"] = json!(fixture.package);
    row["revision"] = json!(fixture.revision);
    row["notes"] = json!(fixture.notes);
    std::fs::write(
        root.join("range-sets-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(
        root.join("range-sets-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    Ok(())
}

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
fn stereo_balance_keeps_channels_through_limited_preview_and_render() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::stereo_balance(&root.join("stereo-balance"))?;
    for (frame, ratio, peak) in [(4, 0.0, 0.316), (16, 2.0, 0.158)] {
        let start = (frame * 16_016 + 5) / 10;
        let audio = success(&[
            "inspect-audio",
            fixture.package.to_str().ok_or("UTF-8")?,
            "--samples",
            &start.to_string(),
            &(start + 256).to_string(),
            "--limited",
        ])?;
        let pairs = audio["audio"]["samples"].as_array().ok_or("samples")?;
        let mut measured = 0.0_f64;
        for pair in pairs {
            let left = pair[0].as_f64().ok_or("left")?;
            let right = pair[1].as_f64().ok_or("right")?;
            assert_eq!(right, left * ratio, "frame {frame}: {pair}");
            measured = measured.max(left.abs());
        }
        assert!((measured - peak).abs() < 0.002, "{measured}");
    }
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn stored_equalizer_changes_preview_and_matches_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::equalizer(&root.join("equalizer"))?;
    check(&fixture)?;
    for (frame, peak) in [
        (4, 10.0_f64.powf(-4.0 / 20.0)),
        (16, 10.0_f64.powf(-16.0 / 20.0)),
    ] {
        let start = (frame * 16_016 + 5) / 10;
        let audio = success(&[
            "inspect-audio",
            fixture.package.to_str().ok_or("UTF-8")?,
            "--samples",
            &start.to_string(),
            &(start + 256).to_string(),
            "--limited",
        ])?;
        let pairs = audio["audio"]["samples"].as_array().ok_or("samples")?;
        let measured = pairs
            .iter()
            .map(|pair| pair[0].as_f64().unwrap().abs())
            .fold(0.0_f64, f64::max);
        assert!(
            (measured - peak).abs() < 0.001,
            "EQ tone amplitude {measured}, expected {peak}"
        );
    }
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn group_saturation_matches_summed_voice_oracle_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::group_saturation(&root.join("group-saturation"))?;
    let start = 16016 + 512;
    let audio = success(&[
        "inspect-audio",
        fixture.package.to_str().ok_or("UTF-8")?,
        "--samples",
        &start.to_string(),
        &(start + 256).to_string(),
        "--limited",
    ])?;
    let pairs = audio["audio"]["samples"].as_array().ok_or("samples")?;
    assert_eq!(pairs.len(), 256);
    let amplitude = |db: f64| 10.0_f64.powf(db / 20.0);
    let mut maximum_error = 0.0_f64;
    for (index, pair) in pairs.iter().enumerate() {
        let at = start + index as i64;
        let source = at - 16016;
        // Independently generated pcm-stereo-48000.wav fixture, exact i16 PCM.
        let left = ((source * 97) % 16384) - 8192;
        let right = match source % 257 {
            0 => -32768,
            1 => 32767,
            _ => (source % 97) * 3 - 48 * 3,
        };
        let tone = ((std::f64::consts::TAU * 1000.0 * at as f64 / 48000.0).sin() * amplitude(-10.0))
            as f32;
        for (channel, pcm) in [left, right].into_iter().enumerate() {
            let sound = pcm as f64 / 32768.0;
            let group = ((f64::from(tone) + sound) * amplitude(12.0)).tanh() * amplitude(-12.0);
            let expected =
                ((group + sound * amplitude(-12.0)) * amplitude(6.0)).tanh() * amplitude(-6.0);
            let actual = pair[channel].as_f64().ok_or("channel sample")?;
            maximum_error = maximum_error.max((actual - expected).abs());
        }
    }
    assert!(
        maximum_error < 0.000001,
        "shared bus oracle error: {maximum_error}"
    );
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "package":fixture.package,"movie":movie,"report":report,
                "oracle_samples":256,"maximum_oracle_error":maximum_error,
            }))?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn shared_equalizer_preserves_mixed_preview_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::shared_equalizer(&root.join("shared-equalizer"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn post_shared_automation_preserves_preview_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::post_shared_automation(&root.join("post-shared-automation"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes,
            }))?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn selected_tail_send_preserves_preview_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::selected_tail_send(&root.join("selected-tail-send"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes,
            }))?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn continuous_event_return_preserves_preview_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::continuous_event_return(&root.join("continuous-event-return"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes,
            }))?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn partial_copy_preserves_authored_mix_and_uses_current_mastering_context() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let report = recipes::partial_sounds::partial_copy_mastering_context(&root.join("mixed"))?;
    if let Some(path) = std::env::var_os("DEADPAN_COPY_MASTERING_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    eprintln!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[test]
fn nested_equalized_sound_prepares_cold_and_split_reads() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let report = recipes::nested_sounds::prepare(&root.join("nested-eq"))?;
    if let Some(path) = std::env::var_os("DEADPAN_NESTED_EQ_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    eprintln!("{report}");
    Ok(())
}

#[test]
fn partial_repeat_attached_sounds_preserve_preview_and_encoded_fidelity() -> Result {
    partial_repeat_export(false)
}

#[test]
fn partial_existing_repeat_attached_sounds_preserve_preview_and_encoded_fidelity() -> Result {
    partial_repeat_export(true)
}

#[test]
fn occurrence_pause_preserves_attached_pcm_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::partial_sounds::occurrence_pauses(&root.join("occurrence-pauses"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
    assert!(passed, "{report}");
    assert_eq!(report["summary"]["pictures_checked"], 15);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_OCCURRENCE_PAUSE_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn mixed_repeat_pause_preview_and_encoded_output_keep_authored_source_clocks() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "default-play",
            recipes::mixed_pause::default_play as fn(&Path) -> Result<Fixture>,
        ),
        (
            "play-default",
            recipes::mixed_pause::play_default as fn(&Path) -> Result<Fixture>,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(
            json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
        );
    }
    if let Some(path) = std::env::var_os("DEADPAN_MIXED_PAUSE_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn scoped_hold_duration_preview_and_encoded_output_keep_authored_source_clocks() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "grow",
            recipes::scoped_duration::grow as fn(&Path) -> Result<Fixture>,
        ),
        (
            "shrink",
            recipes::scoped_duration::shrink as fn(&Path) -> Result<Fixture>,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert_eq!(report["audio"][0]["flags"], json!([]));
        assert_eq!(report["audio"][0]["blocks"]["low_snr_blocks"], 0);
        assert_eq!(report["audio"][0]["offset_status"], "verified_zero");
        assert_eq!(report["audio"][0]["measured_offset_samples"], 0);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(
            json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
        );
    }
    if let Some(path) = std::env::var_os("DEADPAN_SCOPED_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn ordinary_hold_duration_preview_and_encoded_output_keep_shared_source_clocks() -> Result {
    verify_ordinary_hold_duration(false)
}

#[test]
fn shared_hold_duration_semantics_preview_and_encoded_output_keep_shared_source_clocks() -> Result {
    verify_ordinary_hold_duration(true)
}

#[test]
fn mixed_hold_duration_semantics_preview_and_encoded_output_keep_selected_source_clocks() -> Result
{
    verify_selected_or_mixed_hold_duration(false)
}

#[test]
fn selected_hold_duration_semantics_preview_and_encoded_output_keep_each_source_clock() -> Result {
    verify_selected_or_mixed_hold_duration(true)
}

#[test]
fn shared_fixed_retime_hold_duration_preview_and_encoded_output_keep_fixed_windows() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, grow, affine) in [
        ("unit-grow", true, false),
        ("unit-shrink", false, false),
        ("affine-grow", true, true),
        ("affine-shrink", false, true),
    ] {
        let fixture = recipes::shared_fixed_retime_duration::build(&root.join(name), grow, affine)?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        for audio in report["audio"].as_array().ok_or("audio report")? {
            assert_eq!(audio["flags"], json!([]));
            assert_eq!(audio["uncovered_samples"], 0);
            assert_eq!(audio["blocks"]["low_snr_blocks"], 0);
            assert_eq!(audio["offset_status"], "verified_zero");
            assert_eq!(audio["measured_offset_samples"], 0);
        }
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(json!({"package":fixture.package,"movie":movie,
            "report":report,"notes":fixture.notes}));
    }
    if let Some(path) = std::env::var_os("DEADPAN_SHARED_FIXED_RETIME_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn this_play_fixed_retime_hold_duration_preview_and_encoded_output_keep_fixed_windows() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "unit-grow",
            recipes::this_play_fixed_retime_duration::unit_grow as fn(&Path) -> Result<Fixture>,
        ),
        (
            "unit-shrink",
            recipes::this_play_fixed_retime_duration::unit_shrink,
        ),
        (
            "affine-grow",
            recipes::this_play_fixed_retime_duration::affine_grow,
        ),
        (
            "affine-shrink",
            recipes::this_play_fixed_retime_duration::affine_shrink,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        for audio in report["audio"].as_array().ok_or("audio report")? {
            assert_eq!(audio["flags"], json!([]));
            assert_eq!(audio["uncovered_samples"], 0);
            assert_eq!(audio["blocks"]["low_snr_blocks"], 0);
            assert_eq!(audio["offset_status"], "verified_zero");
            assert_eq!(audio["measured_offset_samples"], 0);
        }
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(json!({"package":fixture.package,"movie":movie,
            "report":report,"notes":fixture.notes}));
    }
    if let Some(path) = std::env::var_os("DEADPAN_THIS_PLAY_FIXED_RETIME_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn nested_fixed_retime_hold_duration_preview_and_encoded_output_keep_fixed_windows() -> Result {
    let scratch = tempfile::tempdir()?;
    // Keep every source package and emitted file whenever a result record is requested.
    let (root, _guard) =
        if std::env::var_os("DEADPAN_NESTED_FIXED_RETIME_DURATION_RESULTS").is_some() {
            (scratch.keep(), None)
        } else {
            keep_or(scratch)
        };
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "unit-grow",
            recipes::nested_fixed_retime_duration::unit_grow as fn(&Path) -> Result<Fixture>,
        ),
        (
            "unit-shrink",
            recipes::nested_fixed_retime_duration::unit_shrink,
        ),
        (
            "affine-grow",
            recipes::nested_fixed_retime_duration::affine_grow,
        ),
        (
            "affine-shrink",
            recipes::nested_fixed_retime_duration::affine_shrink,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        for audio in report["audio"].as_array().ok_or("audio report")? {
            assert_eq!(audio["flags"], json!([]));
            assert_eq!(audio["uncovered_samples"], 0);
            assert_eq!(audio["blocks"]["low_snr_blocks"], 0);
            assert_eq!(audio["offset_status"], "verified_zero");
            assert_eq!(audio["measured_offset_samples"], 0);
        }
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(json!({"package":fixture.package,"movie":movie,
            "report":report,"notes":fixture.notes}));
    }
    if let Some(path) = std::env::var_os("DEADPAN_NESTED_FIXED_RETIME_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn mixed_fixed_retime_hold_duration_preview_and_encoded_output_keep_fixed_windows() -> Result {
    let scratch = tempfile::tempdir()?;
    let (root, _guard) =
        if std::env::var_os("DEADPAN_MIXED_FIXED_RETIME_DURATION_RESULTS").is_some() {
            (scratch.keep(), None)
        } else {
            keep_or(scratch)
        };
    let mut results = Vec::new();
    for outer_default in [true, false] {
        for affine in [false, true] {
            for grow in [true, false] {
                let name = format!(
                    "{}-{}-{}",
                    if outer_default {
                        "default-play"
                    } else {
                        "play-default"
                    },
                    if affine { "affine" } else { "unit" },
                    if grow { "grow" } else { "shrink" }
                );
                let fixture = recipes::mixed_fixed_retime_duration::build(
                    &root.join(&name),
                    outer_default,
                    grow,
                    affine,
                )?;
                check(&fixture)?;
                if skip_without_physical_mac("encoded Render") {
                    continue;
                }
                let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
                let (report, passed) =
                    verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
                assert!(passed, "{name}: {report}");
                assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
                assert_eq!(report["summary"]["pictures_failed"], 0);
                assert_eq!(report["summary"]["nonzero_offsets"], 0);
                for audio in report["audio"].as_array().ok_or("audio report")? {
                    assert_eq!(audio["flags"], json!([]));
                    assert_eq!(audio["uncovered_samples"], 0);
                    assert_eq!(audio["blocks"]["low_snr_blocks"], 0);
                    assert_eq!(audio["offset_status"], "verified_zero");
                    assert_eq!(audio["measured_offset_samples"], 0);
                }
                check_provenance(&fixture, &report)?;
                eprintln!("{}", summary_row(&fixture, &report));
                results.push(json!({"package":fixture.package,"movie":movie,
                    "report":report,"notes":fixture.notes}));
            }
        }
    }
    if let Some(path) = std::env::var_os("DEADPAN_MIXED_FIXED_RETIME_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn selected_fixed_retime_hold_duration_preview_and_encoded_output_keep_fixed_windows() -> Result {
    use recipes::selected_fixed_retime_duration::Selection;
    let scratch = tempfile::tempdir()?;
    let (root, _guard) =
        if std::env::var_os("DEADPAN_SELECTED_FIXED_RETIME_DURATION_RESULTS").is_some() {
            (scratch.keep(), None)
        } else {
            keep_or(scratch)
        };
    let mut results = Vec::new();
    for selection in [
        Selection::InnerDefault,
        Selection::InnerPlay,
        Selection::OuterDefault,
        Selection::OuterPlay,
    ] {
        for affine in [false, true] {
            for grow in [true, false] {
                let name = format!(
                    "{}-{}-{}",
                    selection.name(),
                    if affine { "affine" } else { "unit" },
                    if grow { "grow" } else { "shrink" }
                );
                let fixture = recipes::selected_fixed_retime_duration::build(
                    &root.join(&name),
                    selection,
                    grow,
                    affine,
                )?;
                check(&fixture)?;
                if skip_without_physical_mac("encoded Render") {
                    continue;
                }
                let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
                let (report, passed) =
                    verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
                assert!(passed, "{name}: {report}");
                assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
                assert_eq!(report["summary"]["pictures_failed"], 0);
                assert_eq!(report["summary"]["nonzero_offsets"], 0);
                for audio in report["audio"].as_array().ok_or("audio report")? {
                    assert_eq!(audio["flags"], json!([]));
                    assert_eq!(audio["uncovered_samples"], 0);
                    assert_eq!(audio["blocks"]["low_snr_blocks"], 0);
                    assert_eq!(audio["offset_status"], "verified_zero");
                    assert_eq!(audio["measured_offset_samples"], 0);
                }
                check_provenance(&fixture, &report)?;
                eprintln!("{}", summary_row(&fixture, &report));
                results.push(json!({"selection":selection.name(),"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}));
            }
        }
    }
    if let Some(path) = std::env::var_os("DEADPAN_SELECTED_FIXED_RETIME_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn owned_gap_hold_duration_semantics_preview_and_encoded_output_keep_live_and_dormant_clocks()
-> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "play-grow",
            recipes::owned_gap_duration::play_grow as fn(&Path) -> Result<Fixture>,
        ),
        ("play-shrink", recipes::owned_gap_duration::play_shrink),
        ("shared-grow", recipes::owned_gap_duration::shared_grow),
        ("shared-shrink", recipes::owned_gap_duration::shared_shrink),
        ("dormant-play", recipes::owned_gap_duration::dormant_play),
        (
            "dormant-shared",
            recipes::owned_gap_duration::dormant_shared,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        for audio in report["audio"].as_array().ok_or("audio report")? {
            assert_eq!(audio["flags"], json!([]));
            assert_eq!(audio["uncovered_samples"], 0);
            assert_eq!(audio["blocks"]["low_snr_blocks"], 0);
            assert_eq!(audio["offset_status"], "verified_zero");
            assert_eq!(audio["measured_offset_samples"], 0);
        }
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(json!({"package":fixture.package,"movie":movie,
            "report":report,"notes":fixture.notes}));
    }
    if let Some(path) = std::env::var_os("DEADPAN_OWNED_GAP_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

fn verify_selected_or_mixed_hold_duration(selected: bool) -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    let cases = if selected {
        [
            (
                "selected-owned-grow",
                recipes::ordinary_duration::selected_owned_grow as fn(&Path) -> Result<Fixture>,
            ),
            (
                "selected-owned-shrink",
                recipes::ordinary_duration::selected_owned_shrink,
            ),
            (
                "selected-shared-grow",
                recipes::ordinary_duration::selected_shared_grow,
            ),
            (
                "selected-shared-shrink",
                recipes::ordinary_duration::selected_shared_shrink,
            ),
        ]
    } else {
        [
            (
                "outer-grow",
                recipes::ordinary_duration::mixed_outer_grow as fn(&Path) -> Result<Fixture>,
            ),
            (
                "outer-shrink",
                recipes::ordinary_duration::mixed_outer_shrink as fn(&Path) -> Result<Fixture>,
            ),
            (
                "inner-grow",
                recipes::ordinary_duration::mixed_inner_grow as fn(&Path) -> Result<Fixture>,
            ),
            (
                "inner-shrink",
                recipes::ordinary_duration::mixed_inner_shrink as fn(&Path) -> Result<Fixture>,
            ),
        ]
    };
    for (name, factory) in cases {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        for audio in report["audio"].as_array().ok_or("audio report")? {
            assert_eq!(audio["flags"], json!([]));
            assert_eq!(audio["uncovered_samples"], 0);
            assert_eq!(audio["blocks"]["low_snr_blocks"], 0);
            assert_eq!(audio["offset_status"], "verified_zero");
            assert_eq!(audio["measured_offset_samples"], 0);
        }
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(
            json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
        );
    }
    let variable = if selected {
        "DEADPAN_SELECTED_DURATION_RESULTS"
    } else {
        "DEADPAN_MIXED_DURATION_RESULTS"
    };
    if let Some(path) = std::env::var_os(variable) {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

fn verify_ordinary_hold_duration(semantic: bool) -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "grow",
            if semantic {
                recipes::ordinary_duration::semantic_grow
            } else {
                recipes::ordinary_duration::grow
            } as fn(&Path) -> Result<Fixture>,
        ),
        (
            "shrink",
            if semantic {
                recipes::ordinary_duration::semantic_shrink
            } else {
                recipes::ordinary_duration::shrink
            } as fn(&Path) -> Result<Fixture>,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert_eq!(report["audio"][0]["flags"], json!([]));
        assert_eq!(report["audio"][0]["blocks"]["low_snr_blocks"], 0);
        assert_eq!(report["audio"][0]["offset_status"], "verified_zero");
        assert_eq!(report["audio"][0]["measured_offset_samples"], 0);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(
            json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
        );
    }
    let results_key = if semantic {
        "DEADPAN_SHARED_DURATION_RESULTS"
    } else {
        "DEADPAN_ORDINARY_DURATION_RESULTS"
    };
    if let Some(path) = std::env::var_os(results_key) {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn fixed_retime_hold_duration_preview_and_encoded_output_keep_interior_source_clocks() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "grow",
            recipes::fixed_retime_duration::grow as fn(&Path) -> Result<Fixture>,
        ),
        (
            "shrink",
            recipes::fixed_retime_duration::shrink as fn(&Path) -> Result<Fixture>,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert_eq!(report["audio"][0]["flags"], json!([]));
        assert_eq!(report["audio"][0]["blocks"]["low_snr_blocks"], 0);
        assert_eq!(report["audio"][0]["offset_status"], "verified_zero");
        assert_eq!(report["audio"][0]["measured_offset_samples"], 0);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(
            json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
        );
    }
    if let Some(path) = std::env::var_os("DEADPAN_FIXED_RETIME_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn processing_hold_duration_preview_and_encoded_output_keep_input_clocks() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "preserve-grow",
            recipes::processing_duration::preserve_grow as fn(&Path) -> Result<Fixture>,
        ),
        (
            "preserve-shrink",
            recipes::processing_duration::preserve_shrink as fn(&Path) -> Result<Fixture>,
        ),
        (
            "shift-grow",
            recipes::processing_duration::shift_grow as fn(&Path) -> Result<Fixture>,
        ),
        (
            "shift-shrink",
            recipes::processing_duration::shift_shrink as fn(&Path) -> Result<Fixture>,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert!(
            report["summary"]["signal_windows"]
                .as_u64()
                .is_some_and(|count| count > 0)
        );
        assert_eq!(report["audio"][0]["flags"], json!([]));
        assert_eq!(report["audio"][0]["blocks"]["low_snr_blocks"], 0);
        assert_eq!(report["audio"][0]["offset_status"], "verified_zero");
        assert_eq!(report["audio"][0]["measured_offset_samples"], 0);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(json!({"name":name,"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}));
    }
    if let Some(path) = std::env::var_os("DEADPAN_PROCESSING_HOLD_DURATION_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn clocked_speed_update_preview_and_encoded_output_keep_exact_pause_clocks() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut results = Vec::new();
    for (name, factory) in [
        (
            "shrink",
            recipes::clocked_speed_update::shrink as fn(&Path) -> Result<Fixture>,
        ),
        (
            "grow",
            recipes::clocked_speed_update::grow as fn(&Path) -> Result<Fixture>,
        ),
    ] {
        let fixture = factory(&root.join(name))?;
        check(&fixture)?;
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
        let movie = render(&fixture, &root.join(format!("exports-{name}")))?;
        let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
        assert!(passed, "{name}: {report}");
        assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
        assert_eq!(report["summary"]["pictures_failed"], 0);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        assert!(
            report["summary"]["signal_windows"]
                .as_u64()
                .is_some_and(|count| count > 0)
        );
        assert_eq!(report["audio"][0]["flags"], json!([]));
        assert_eq!(report["audio"][0]["blocks"]["low_snr_blocks"], 0);
        assert_eq!(report["audio"][0]["offset_status"], "verified_zero");
        assert_eq!(report["audio"][0]["measured_offset_samples"], 0);
        check_provenance(&fixture, &report)?;
        eprintln!("{}", summary_row(&fixture, &report));
        results.push(json!({"name":name,"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}));
    }
    if let Some(path) = std::env::var_os("DEADPAN_CLOCKED_SPEED_UPDATE_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&results)?)?;
    }
    Ok(())
}

#[test]
fn inner_repeat_speed_update_preview_and_encoded_output_keep_physical_carriers() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::inner_repeat_speed::shrink(&root.join("inner-repeat-speed"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports-inner-repeat-speed"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
    assert!(passed, "{report}");
    assert_eq!(report["summary"]["pictures_checked"], 22);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    assert!(
        report["summary"]["signal_windows"]
            .as_u64()
            .is_some_and(|count| count > 0)
    );
    assert_eq!(report["audio"][0]["flags"], json!([]));
    assert_eq!(report["audio"][0]["blocks"]["low_snr_blocks"], 0);
    assert_eq!(report["audio"][0]["offset_status"], "verified_zero");
    assert_eq!(report["audio"][0]["measured_offset_samples"], 0);
    check_provenance(&fixture, &report)?;
    eprintln!("{}", summary_row(&fixture, &report));
    if let Some(path) = std::env::var_os("DEADPAN_INNER_REPEAT_SPEED_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "name": fixture.name,
                "package": fixture.package,
                "movie": movie,
                "report": report,
                "notes": fixture.notes,
            }))?,
        )?;
    }
    Ok(())
}

#[test]
fn scoped_audio_chain_preview_and_complete_encoded_output_isolate_one_nested_play() -> Result {
    use recipes::section8::scoped_audio::{FRAMES, SAMPLES};
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::section8::scoped_audio::build(&root.join("scoped-audio"))?;
    assert_eq!(fixture.frames, FRAMES);
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports-scoped-audio"))?;
    // Cover every presented sample, including both endpoints and all five seams.
    // Six bounded contiguous windows stay far below the CLI's 64-window limit.
    let windows: Vec<_> = (0..SAMPLES)
        .step_by(19_220)
        .map(|start| (start, (start + 19_220).min(SAMPLES)))
        .collect();
    assert_eq!(windows.len(), 6);
    assert_eq!(windows[0].0, 0);
    assert_eq!(windows.last().ok_or("last audio window")?.1, SAMPLES);
    for pair in windows.windows(2) {
        assert_eq!(pair[0].1, pair[1].0);
    }
    let selection = windows
        .iter()
        .map(|(start, end)| format!("{start}:{end}"))
        .collect::<Vec<_>>()
        .join(",");
    let (report, passed) = verify(
        &fixture,
        &movie,
        &fixture.revision,
        &["--every", "1", "--samples", &selection],
    )?;
    assert!(passed, "{report}");
    assert_eq!(report["movie"]["video_frames"], FRAMES);
    assert_eq!(report["movie"]["expected_video_frames"], FRAMES);
    assert_eq!(report["movie"]["expected_audio_samples"], SAMPLES);
    assert_eq!(report["movie"]["audio"]["presented_end"], SAMPLES);
    assert_eq!(
        report["movie"]["audio_edits"]["presented_media_ticks"],
        SAMPLES
    );
    assert_eq!(report["movie"]["picture_anomalies"], json!([]));
    assert_eq!(report["movie"]["audio"]["gaps"], json!([]));
    assert_eq!(report["summary"]["pictures_checked"], FRAMES);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["audio_windows_checked"], windows.len());
    assert_eq!(report["summary"]["signal_windows"], windows.len());
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    assert_eq!(
        report["pictures"].as_array().ok_or("pictures")?.len(),
        FRAMES as usize
    );
    check_provenance(&fixture, &report)?;
    let audio = report["audio"].as_array().ok_or("complete audio windows")?;
    assert_eq!(audio.len(), windows.len());
    for (window, (start, end)) in audio.iter().zip(&windows) {
        assert_eq!(window["window"], json!([start, end]));
        assert_eq!(window["project_samples"], json!([start, end]));
        assert_eq!(window["passed"], true);
        assert_eq!(window["kind"], "signal");
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    if let Some(path) = std::env::var_os("DEADPAN_SCOPED_AUDIO_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes,
            }))?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn fixed_retime_output_owned_sound_keeps_retained_phase_through_hold_edits_and_complete_export()
-> Result {
    use recipes::fixed_retime_output_sound::{FRAMES, SAMPLES};
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::fixed_retime_output_sound::build(&root.join("output-owned-sound"))?;
    assert_eq!(fixture.frames, FRAMES);
    check_with_picture(&fixture, recipes::fixed_retime_output_sound::plan_picture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports-output-owned"))?;
    // Both complete contiguous halves contain substantial independent event
    // signal. Silence outside it is also verified; no default sampled gaps.
    let windows = [(0, SAMPLES / 2), (SAMPLES / 2, SAMPLES)];
    let selection = windows
        .iter()
        .map(|(a, b)| format!("{a}:{b}"))
        .collect::<Vec<_>>()
        .join(",");
    let (report, passed) = verify(
        &fixture,
        &movie,
        &fixture.revision,
        &["--every", "1", "--samples", &selection],
    )?;
    assert!(passed, "{report}");
    assert_eq!(report["movie"]["video_frames"], FRAMES);
    assert_eq!(report["movie"]["expected_video_frames"], FRAMES);
    assert_eq!(report["movie"]["expected_audio_samples"], SAMPLES);
    assert_eq!(report["movie"]["audio"]["presented_end"], SAMPLES);
    assert_eq!(
        report["movie"]["audio_edits"]["presented_media_ticks"],
        SAMPLES
    );
    assert_eq!(report["movie"]["picture_anomalies"], json!([]));
    assert_eq!(report["movie"]["audio"]["gaps"], json!([]));
    assert_eq!(report["summary"]["pictures_checked"], FRAMES);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["audio_windows_checked"], windows.len());
    assert_eq!(report["summary"]["signal_windows"], windows.len());
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    assert_eq!(
        report["pictures"].as_array().ok_or("pictures")?.len(),
        FRAMES as usize
    );
    check_provenance(&fixture, &report)?;
    let audio = report["audio"].as_array().ok_or("audio windows")?;
    assert_eq!(audio.len(), windows.len());
    for (window, (start, end)) in audio.iter().zip(windows) {
        assert_eq!(window["window"], json!([start, end]));
        assert_eq!(window["project_samples"], json!([start, end]));
        assert_eq!(window["passed"], true);
        assert_eq!(window["kind"], "signal");
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    if let Some(path) = std::env::var_os("DEADPAN_FIXED_RETIME_OUTPUT_SOUND_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn fractional_processed_owner_support_keeps_nonzero_shared_master_and_complete_export() -> Result {
    use recipes::fractional_owner_support::{FRAMES, SAMPLES};
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let directory = root.join("fractional-owner-support");
    let fixture = recipes::fractional_owner_support::build(&directory)?;
    assert_eq!(fixture.frames, FRAMES);
    assert_eq!(fixture.expectations.len(), FRAMES as usize);
    check_with_picture(&fixture, recipes::fixed_retime_output_sound::plan_picture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports-fractional-owner"))?;
    // The two contiguous halves include distinct nonperiodic Original and
    // independent-event content. Every emitted sample, including the exact
    // exhausted physical point, is compared without event-based alignment.
    let windows = [(0, SAMPLES / 2), (SAMPLES / 2, SAMPLES)];
    let selection = windows
        .iter()
        .map(|(a, b)| format!("{a}:{b}"))
        .collect::<Vec<_>>()
        .join(",");
    let (report, passed) = verify(
        &fixture,
        &movie,
        &fixture.revision,
        &["--every", "1", "--samples", &selection],
    )?;
    assert!(passed, "{report}");
    assert_eq!(report["movie"]["video_frames"], FRAMES);
    assert_eq!(report["movie"]["expected_video_frames"], FRAMES);
    assert_eq!(report["movie"]["expected_audio_samples"], SAMPLES);
    assert_eq!(report["movie"]["audio"]["presented_end"], SAMPLES);
    assert_eq!(
        report["movie"]["audio_edits"]["presented_media_ticks"],
        SAMPLES
    );
    assert_eq!(report["movie"]["picture_anomalies"], json!([]));
    assert_eq!(report["movie"]["audio"]["gaps"], json!([]));
    assert_eq!(report["summary"]["pictures_checked"], FRAMES);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["audio_windows_checked"], windows.len());
    assert_eq!(report["summary"]["signal_windows"], windows.len());
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let pictures = report["pictures"].as_array().ok_or("pictures")?;
    assert_eq!(pictures.len(), FRAMES as usize);
    check_provenance(&fixture, &report)?;
    for (picture, (frame, expected)) in pictures.iter().zip(&fixture.expectations) {
        assert_eq!(picture["output_frame"], *frame);
        assert_eq!(picture["project_frame"], *frame);
        assert_eq!(picture["passed"], true);
        assert_eq!(picture["flags"], json!([]));
        if let Expected::Original { source_ordinal } = expected {
            assert_eq!(picture["provenance"]["source_pts"], source_ordinal * 1001);
        }
    }
    let audio = report["audio"].as_array().ok_or("audio windows")?;
    assert_eq!(audio.len(), windows.len());
    for (window, (start, end)) in audio.iter().zip(windows) {
        assert_eq!(window["window"], json!([start, end]));
        assert_eq!(window["project_samples"], json!([start, end]));
        assert_eq!(window["passed"], true);
        assert_eq!(window["kind"], "signal");
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    if let Some(path) = std::env::var_os("DEADPAN_FRACTIONAL_OWNER_SUPPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes,
                "oracle":directory.join("whole-master-oracle.json"),
                "producer":directory.join("producer/producer.json")
            }))?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

fn partial_repeat_export(existing: bool) -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let factory = if existing {
        recipes::partial_sounds::partial_existing_repeat_attached_sounds
    } else {
        recipes::partial_sounds::partial_repeat_attached_sounds
    };
    let fixture = factory(&root.join("partial-repeat-sounds"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    assert_eq!(report["audio"][0]["flags"], json!([]));
    assert_eq!(report["audio"][0]["blocks"]["low_snr_blocks"], 0);
    assert_eq!(report["audio"][0]["offset_status"], "verified_zero");
    assert_eq!(report["audio"][0]["measured_offset_samples"], 0);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(
        report["summary"]["pictures_checked"],
        if existing { 17 } else { 13 }
    );
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PARTIAL_REPEAT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn split_attached_sounds_preserve_preview_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::partial_sounds::split_attached_sounds(&root.join("split-sounds"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_SPLIT_SOUND_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn partial_attached_sound_copies_preserve_preview_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture =
        recipes::partial_sounds::partial_attached_sound_copies(&root.join("partial-sounds"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn treated_event_preserves_preview_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::treated_event(&root.join("treated-event"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes}),
            )?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn continuous_owner_return_preserves_preview_and_verified_export() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::continuous_owner_return(&root.join("continuous-owner-return"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &[])?;
    assert!(passed, "{report}");
    check_provenance(&fixture, &report)?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "package":fixture.package,"movie":movie,"report":report,"notes":fixture.notes,
            }))?,
        )?;
    }
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}

#[test]
fn every_recipe_builds_a_valid_package_whose_plan_matches_its_expectations_part_1() -> Result {
    check_recipe_partition(0)
}

#[test]
fn every_recipe_builds_a_valid_package_whose_plan_matches_its_expectations_part_2() -> Result {
    check_recipe_partition(1)
}

#[test]
fn every_recipe_builds_a_valid_package_whose_plan_matches_its_expectations_part_3() -> Result {
    check_recipe_partition(2)
}

fn check_recipe_partition(partition: usize) -> Result {
    let scratch = tempfile::tempdir()?;
    // DEADPAN_PREVIEW_EXPORT_KEEP=1 retains the packages for inspection.
    let root = if std::env::var_os("DEADPAN_PREVIEW_EXPORT_KEEP").is_some() {
        scratch.keep()
    } else {
        scratch.path().to_owned()
    };
    // All three residues cover the shared inventory exactly once. New builders
    // automatically join one test without changing a hand-maintained name list.
    for fixture in recipes::partition(&root, partition, 3)? {
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
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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
fn source_formats_preserve_independent_clocks_through_combined_structural_edits_part_1() -> Result {
    check_source_format_partition(0)
}

#[test]
fn source_formats_preserve_independent_clocks_through_combined_structural_edits_part_2() -> Result {
    check_source_format_partition(1)
}

#[test]
fn source_formats_preserve_independent_clocks_through_combined_structural_edits_part_3() -> Result {
    check_source_format_partition(2)
}

#[test]
fn source_formats_preserve_independent_clocks_through_combined_structural_edits_part_4() -> Result {
    check_source_format_partition(3)
}

fn check_source_format_partition(partition: usize) -> Result {
    use deadpan_core::PitchPolicy;
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("Source-format operation evidence: {}", root.display());
    // The four residues cover every current and future case exactly once;
    // each case still exercises both pitch policies and every original check.
    for case in recipes::formats::CASES.iter().skip(partition).step_by(4) {
        for (policy, pitch) in [
            ("preserve", PitchPolicy::Preserve),
            ("tape", PitchPolicy::FollowSpeed),
        ] {
            let directory = root.join(case.name).join(policy);
            let fixture = recipes::formats::build(&directory, *case, pitch)?;
            check_with_picture(&fixture, |picture| case.plan_picture(picture))?;
            if skip_without_physical_mac("encoded Render") {
                continue;
            }
            let movie = render(&fixture, &directory.join("exports"))?;
            let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
            std::fs::write(
                directory.join("verification.json"),
                serde_json::to_vec_pretty(&report)?,
            )?;
            assert!(passed, "{} {policy}: {}", case.name, report["failures"]);
            assert_eq!(report["frame_rate"], json!(case.rate));
            assert_eq!(report["range"], json!([0, 26]));
            assert_eq!(report["summary"]["pictures_checked"], 26);
            assert_eq!(report["summary"]["nonzero_offsets"], 0);
            check_provenance(&fixture, &report)?;
            eprintln!("{policy}: {}", summary_row(&fixture, &report));
        }
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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

/// Source interpretation changes exported pixels while retaining exact picture
/// and audio clocks. Historical revisions remain verifiable after Undo.
#[test]
fn source_interpretation_exports_are_revision_bound_and_undo_restores_picture_semantics() -> Result
{
    use deadpan_core::{
        Command as Edit, CommandRequest, RevisionId, SourceChromaLocation, SourceColorRange,
        SourceInterpretation,
    };
    use deadpan_store::{AccessMode, ProjectStore};
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("source interpretation export evidence: {}", root.display());
    let automatic = recipes::black_pause(&root.join("fixture"))?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let old_movie = render(&automatic, &root.join("automatic"))?;
    let mut corrected = automatic.clone();
    corrected.revision = "interpreted".into();
    {
        let mut store = ProjectStore::open(&automatic.package, AccessMode::ReadWrite)?;
        let document = store.snapshot()?;
        let Some(deadpan_store::single_source::SingleSourceState::Ready { asset, .. }) =
            store.single_source_state()?
        else {
            return Err("missing Original".into());
        };
        store.commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(&corrected.revision)?,
            command: Edit::SetSourceInterpretation {
                asset,
                interpretation: Some(SourceInterpretation {
                    range: Some(SourceColorRange::Full),
                    chroma_location: Some(SourceChromaLocation::Center),
                }),
            },
        })?;
        let after = store.snapshot()?;
        assert_eq!(after.nodes(), document.nodes());
        assert_eq!(after.assets(), document.assets());
    }
    let new_movie = render(&corrected, &root.join("corrected"))?;
    for (movie, revision) in [
        (&old_movie, &automatic.revision),
        (&new_movie, &corrected.revision),
    ] {
        let (report, passed) = verify(&corrected, movie, revision, &[])?;
        std::fs::write(
            root.join(format!("{revision}-verification.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        assert!(passed, "{}", report["failures"]);
        assert_eq!(report["summary"]["pictures_checked"], corrected.frames);
        assert_eq!(report["summary"]["nonzero_offsets"], 0);
        eprintln!(
            "source interpretation {revision}: {}",
            summary_row(&corrected, &report)
        );
    }
    let (wrong, passed) = verify(&corrected, &old_movie, &corrected.revision, &[])?;
    std::fs::write(
        root.join("wrong-revision-verification.json"),
        serde_json::to_vec_pretty(&wrong)?,
    )?;
    assert!(
        !passed,
        "an automatic-range movie cannot verify against corrected pixels: {wrong}"
    );
    {
        let mut store = ProjectStore::open(&automatic.package, AccessMode::ReadWrite)?;
        store.undo(
            &RevisionId::new(&corrected.revision)?,
            RevisionId::new("interpretation-undone")?,
        )?;
        assert!(store.snapshot()?.source_interpretations().is_empty());
    }
    let (restored, passed) = verify(&automatic, &old_movie, "interpretation-undone", &[])?;
    std::fs::write(
        root.join("restored-verification.json"),
        serde_json::to_vec_pretty(&restored)?,
    )?;
    assert!(passed, "{}", restored["failures"]);
    let (captured, passed) = verify(&corrected, &new_movie, &corrected.revision, &[])?;
    std::fs::write(
        root.join("historical-corrected-verification.json"),
        serde_json::to_vec_pretty(&captured)?,
    )?;
    assert!(
        passed,
        "immutable corrected revision after Undo: {}",
        captured["failures"]
    );
    Ok(())
}

/// Default-gate end-to-end check: one real export of a Section 8 recipe
/// matches its committed preview, and a later same-duration reframe and gain
/// change of that revision is reported against the old movie.
#[test]
fn black_pause_export_matches_preview_and_later_edits_are_reported() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let fixture = recipes::black_pause(&root.join("fixture"))?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
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

/// Staged mark selectors must follow preceding edits through a real export.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn named_mark_range_export_reresolves_after_hold_growth_cut_and_repeat() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("named mark range Render evidence: {}", root.display());
    let fixture = recipes::named_mark_range::build(&root.join("fixture"))?;
    check(&fixture)?;
    if skip_without_physical_mac("named mark range encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = render(&fixture, &root.join("exports"))?;
    let rendered = started.elapsed();
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 33);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("audio report")?;
    assert!(!audio.is_empty(), "full canonical audio comparison");
    for window in audio {
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["uncovered_samples"], 0);
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    assert!(passed, "{report}");
    let mut row = summary_row(&fixture, &report);
    row["render_seconds"] = json!(rendered.as_secs_f64());
    row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
    row["movie"] = json!(movie);
    row["package"] = json!(fixture.package);
    row["revision"] = json!(fixture.revision);
    row["notes"] = json!(fixture.notes);
    std::fs::write(
        root.join("named-mark-range-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(
        root.join("named-mark-range-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    Ok(())
}

/// Whole node identities resolve again after earlier structural edits.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn explicit_node_export_reresolves_after_hold_growth_whole_cut_and_repeat() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("explicit node Render evidence: {}", root.display());
    let fixture = recipes::explicit_node::build(&root.join("fixture"))?;
    check(&fixture)?;
    if skip_without_physical_mac("explicit node encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = render(&fixture, &root.join("exports"))?;
    let rendered = started.elapsed();
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 37);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("audio report")?;
    assert_eq!(audio.len(), 1, "one full canonical audio comparison");
    assert_eq!(audio[0]["project_samples"], json!([0, 59259]));
    for window in audio {
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["uncovered_samples"], 0);
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    assert!(passed, "{report}");
    let mut row = summary_row(&fixture, &report);
    row["render_seconds"] = json!(rendered.as_secs_f64());
    row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
    row["movie"] = json!(movie);
    row["package"] = json!(fixture.package);
    row["revision"] = json!(fixture.revision);
    row["notes"] = json!(fixture.notes);
    std::fs::write(
        root.join("explicit-node-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(
        root.join("explicit-node-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    Ok(())
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
        if skip_without_physical_mac("encoded Render") {
            continue;
        }
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
        // The exported pictures carry the authored framing: against a later
        // revision that changes only one beat's framing, exactly that beat's
        // frames differ (`:zoom off` really returned the second half to the
        // full picture; the Hold really shows the magnified detail).
        let reframed = match fixture.name {
            "abrupt-return" => Some((
                recipes::section8::reframe(&fixture, 15, Some(recipes::section8::zoomed()?))?,
                15..30,
            )),
            "freeze-detail" => Some((recipes::section8::reframe(&fixture, 15, None)?, 15..27)),
            _ => None,
        };
        if let Some((revision, changed)) = reframed {
            let (stale, passed) = verify(
                &fixture,
                &movie,
                &revision,
                &["--frames", "0,14,15,20,26,27,29", "--no-audio"],
            )?;
            assert!(!passed, "{stale}");
            let mut flagged_frames = Vec::new();
            for picture in stale["pictures"].as_array().ok_or("pictures")? {
                let frame = picture["output_frame"].as_u64().ok_or("frame")?;
                let flagged = picture["flags"].as_array().is_some_and(|f| !f.is_empty());
                assert_eq!(
                    flagged,
                    changed.contains(&frame),
                    "{} frame {frame} against reframed {revision}: {picture}",
                    fixture.name
                );
                if flagged {
                    flagged_frames.push(json!([frame, picture["flags"]]));
                }
            }
            eprintln!(
                "{} against reframed {revision}: flagged {}",
                fixture.name,
                Value::Array(flagged_frames)
            );
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

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "four-minute real export; run with --release"
)]
fn disk_prepared_preserve_export_matches_its_committed_preview() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("disk Preserve export evidence: {}", root.display());
    let fixture = recipes::disk_preserve(&root.join("fixtures"))?;
    check(&fixture)?;
    if skip_without_physical_mac("encoded Render") {
        return Ok(());
    }
    let movie = render(&fixture, &root.join("exports"))?;
    let (report, passed) = verify(&fixture, &movie, &fixture.revision, &["--every", "1"])?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "fixture": fixture.name, "package": fixture.package, "movie": movie,
                "revision": fixture.revision, "report": report,
            }))?,
        )?;
    }
    check_provenance(&fixture, &report)?;
    assert!(passed, "{report}");
    assert_eq!(report["summary"]["pictures_checked"], 7200);
    eprintln!("{}", summary_row(&fixture, &report));
    Ok(())
}
