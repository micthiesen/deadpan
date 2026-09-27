//! Explicit baseline comparison. Differences are review evidence, never taste gates.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use image::{DynamicImage, ImageDecoder, ImageFormat, Limits, Rgba, RgbaImage};
use serde_json::json;

use super::report::{Finding, Report, ScenarioReport, Severity, StepReport};

const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 16_000_000;
const MAX_IMAGE_BYTES: u64 = 128 * 1024 * 1024;
// Complete runs retain semantic frames and accessibility trees for every
// captured state. Their reports can exceed 16 MiB without oversized images.
const MAX_REPORT_BYTES: u64 = 128 * 1024 * 1024;
const CHANNEL_THRESHOLD: u8 = 8;
type Checkpoint = (String, String);
type Checkpoints = BTreeMap<Checkpoint, (usize, usize)>;

/// Compares named checkpoints with a previous report. Baselines are read-only;
/// copied baseline and diff images are exclusively created under `output`.
pub(crate) fn compare(report: &mut Report, output: &Path, baseline: &Path) -> Result<(), String> {
    let output = output.canonicalize().map_err(|error| error.to_string())?;
    let baseline = baseline.canonicalize().map_err(|error| error.to_string())?;
    if output.starts_with(&baseline) || baseline.starts_with(&output) {
        return Err("Baseline and output directories must be separate and non-nested".into());
    }
    let previous: Report = serde_json::from_slice(&read_bounded(
        &baseline.join("report.json"),
        MAX_REPORT_BYTES,
    )?)
    .map_err(|error| format!("Read baseline report: {error}"))?;
    let current = checkpoints(report)?;
    let expected = checkpoints(&previous)?;
    let artifacts = output.join("comparison");
    if !current.is_empty() {
        fs::create_dir(&artifacts)
            .map_err(|error| format!("Create comparison directory: {error}"))?;
    }
    for (index, ((scenario_name, checkpoint), &(scenario_index, step_index))) in
        current.iter().enumerate()
    {
        let scenario = &mut report.scenarios[scenario_index];
        let Some(&(expected_scenario, expected_step)) =
            expected.get(&(scenario_name.clone(), checkpoint.clone()))
        else {
            warn(scenario, format!("No baseline checkpoint: {checkpoint}"));
            continue;
        };
        let actual_step = scenario.steps[step_index].clone();
        let expected_step = &previous.scenarios[expected_scenario].steps[expected_step];
        let (Some(actual_path), Some(expected_path)) =
            (&actual_step.screenshot, &expected_step.screenshot)
        else {
            warn(
                scenario,
                format!("Checkpoint {checkpoint} has no screenshot in one or both reports"),
            );
            continue;
        };
        let (actual, _) = read_image(&contained_png(&output, actual_path)?)?;
        let (expected_image, expected_bytes) =
            read_image(&contained_png(&baseline, expected_path)?)?;
        let (diff, changed, compared) = difference(&actual, &expected_image)?;
        let baseline_name = format!("comparison/{index:04}-baseline.png");
        let diff_name = format!("comparison/{index:04}-diff.png");
        write_new(&output.join(&baseline_name), &expected_bytes)?;
        let mut diff_file = File::create_new(output.join(&diff_name))
            .map_err(|error| format!("Create diff image: {error}"))?;
        diff.write_to(&mut diff_file, ImageFormat::Png)
            .map_err(|error| format!("Write diff image: {error}"))?;
        if changed > 0 || actual.dimensions() != expected_image.dimensions() {
            warn(
                scenario,
                format!(
                    "Checkpoint {checkpoint}: {changed}/{compared} pixels differ by more than {CHANNEL_THRESHOLD} in an RGBA channel or lie outside one image; actual {}×{}, baseline {}×{}. Review the images.",
                    actual.width(),
                    actual.height(),
                    expected_image.width(),
                    expected_image.height()
                ),
            );
        }
        for (kind, screenshot) in [("baseline", baseline_name), ("diff", diff_name)] {
            scenario.steps.push(StepReport {
                input: format!("{checkpoint} {kind}: {changed}/{compared} changed pixels"),
                frame: actual_step.frame,
                virtual_time_ms: actual_step.virtual_time_ms,
                elapsed_wall_ms: actual_step.elapsed_wall_ms,
                semantic: json!({"comparison": {
                    "checkpoint": checkpoint,
                    "kind": kind,
                    "channel_threshold": CHANNEL_THRESHOLD,
                    "changed_pixels": changed,
                    "compared_pixels": compared,
                    "actual_dimensions": [actual.width(), actual.height()],
                    "baseline_dimensions": [expected_image.width(), expected_image.height()],
                    "diff_legend": "magenta: changed overlap; amber: absent in one image; dark: within tolerance",
                    "source_capture_frame": actual_step.frame
                }}),
                screenshot: Some(screenshot),
            });
        }
    }
    for (scenario_name, checkpoint) in expected.keys().filter(|key| !current.contains_key(*key)) {
        if let Some(scenario) = report
            .scenarios
            .iter_mut()
            .find(|scenario| &scenario.name == scenario_name)
        {
            warn(
                scenario,
                format!("Current run omitted baseline checkpoint: {checkpoint}"),
            );
        } else {
            let mut scenario = ScenarioReport::new(scenario_name.clone());
            warn(
                &mut scenario,
                format!("Current run omitted baseline scenario and checkpoint: {checkpoint}"),
            );
            report.scenarios.push(scenario);
        }
    }
    Ok(())
}

fn warn(scenario: &mut ScenarioReport, message: String) {
    scenario.findings.push(Finding {
        severity: Severity::Warning,
        message,
    });
}

fn checkpoints(report: &Report) -> Result<Checkpoints, String> {
    let mut result = BTreeMap::new();
    for (scenario_index, scenario) in report.scenarios.iter().enumerate() {
        for (step_index, step) in scenario.steps.iter().enumerate() {
            let Some(checkpoint) = step
                .semantic
                .get("checkpoint")
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            let key = (scenario.name.clone(), checkpoint.to_owned());
            if result.insert(key, (scenario_index, step_index)).is_some() {
                return Err(format!(
                    "Duplicate checkpoint {checkpoint:?} in scenario {:?}",
                    scenario.name
                ));
            }
        }
    }
    Ok(result)
}

fn contained_png(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || !relative
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_. /".contains(&byte))
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        || path.extension().and_then(|extension| extension.to_str()) != Some("png")
    {
        return Err(format!(
            "Screenshot is not a plain relative PNG path: {relative:?}"
        ));
    }
    let canonical = root
        .join(path)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !canonical.starts_with(root) {
        return Err("Screenshot escapes its report directory".into());
    }
    Ok(canonical)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!(
            "Report input is not a regular file within its {limit}-byte limit"
        ));
    }
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if u64::try_from(bytes.len()).map_err(|error| error.to_string())? > limit {
        return Err("Report input grew beyond its byte limit".into());
    }
    Ok(bytes)
}

fn read_image(path: &Path) -> Result<(RgbaImage, Vec<u8>), String> {
    let bytes = read_bounded(path, MAX_IMAGE_BYTES)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_BYTES);
    let decoder = image::codecs::png::PngDecoder::with_limits(Cursor::new(&bytes), limits)
        .map_err(|error| format!("Read screenshot PNG: {error}"))?;
    let (width, height) = decoder.dimensions();
    dimensions_allowed(width, height)?;
    let image = DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("Decode screenshot PNG: {error}"))?
        .into_rgba8();
    Ok((image, bytes))
}

fn dimensions_allowed(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        Err("Comparison images must be at most 4096 per side and 16 million pixels".into())
    } else {
        Ok(())
    }
}

fn difference(actual: &RgbaImage, expected: &RgbaImage) -> Result<(RgbaImage, u64, u64), String> {
    let (width, height) = (
        actual.width().max(expected.width()),
        actual.height().max(expected.height()),
    );
    dimensions_allowed(width, height)?;
    let mut diff = RgbaImage::new(width, height);
    let mut changed = 0;
    let mut compared = 0;
    for (x, y, pixel) in diff.enumerate_pixels_mut() {
        let a = actual.get_pixel_checked(x, y);
        let b = expected.get_pixel_checked(x, y);
        *pixel = match (a, b) {
            (None, None) => Rgba([0, 0, 0, 255]),
            (Some(a), Some(b)) => {
                compared += 1;
                if a.0
                    .iter()
                    .zip(b.0)
                    .any(|(&a, b)| a.abs_diff(b) > CHANNEL_THRESHOLD)
                {
                    changed += 1;
                    Rgba([255, 0, 160, 255])
                } else {
                    Rgba([a[0] / 5, a[1] / 5, a[2] / 5, 255])
                }
            }
            _ => {
                compared += 1;
                changed += 1;
                Rgba([255, 190, 0, 255])
            }
        };
    }
    Ok((diff, changed, compared))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create_new(path).map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_harness::report::RunMode;

    #[test]
    fn identical_changed_and_differently_sized_images_are_counted() {
        let expected = RgbaImage::from_pixel(2, 2, Rgba([20, 40, 60, 255]));
        assert_eq!(difference(&expected, &expected).unwrap().1, 0);
        let mut actual = expected.clone();
        actual.put_pixel(0, 0, Rgba([28, 40, 60, 255]));
        assert_eq!(difference(&actual, &expected).unwrap().1, 0);
        actual.put_pixel(0, 0, Rgba([29, 40, 60, 255]));
        let (diff, changed, compared) = difference(&actual, &expected).unwrap();
        assert_eq!((changed, compared), (1, 4));
        assert_eq!(*diff.get_pixel(0, 0), Rgba([255, 0, 160, 255]));
        let smaller = RgbaImage::from_pixel(1, 2, Rgba([20, 40, 60, 255]));
        assert_eq!(difference(&smaller, &expected).unwrap().1, 2);
        assert!(dimensions_allowed(4097, 1).is_err());
        assert!(dimensions_allowed(4096, 4096).is_err());
    }

    #[test]
    fn traversal_and_duplicate_checkpoints_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        for path in [
            "../x.png",
            "/x.png",
            "https://x.png",
            "x%2fy.png",
            "x\\y.png",
        ] {
            assert!(contained_png(directory.path(), path).is_err());
        }
        let mut report = fixture_report();
        let duplicate = report.scenarios[0].steps[0].clone();
        report.scenarios[0].steps.push(duplicate);
        assert!(
            checkpoints(&report)
                .unwrap_err()
                .contains("Duplicate checkpoint")
        );
    }

    #[test]
    fn comparison_adds_review_evidence_without_mutating_baseline_or_failing_checks() {
        let baseline = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let original = RgbaImage::from_pixel(2, 2, Rgba([20, 40, 60, 255]));
        original.save(baseline.path().join("capture.png")).unwrap();
        let report_bytes = serde_json::to_vec(&fixture_report()).unwrap();
        fs::write(baseline.path().join("report.json"), &report_bytes).unwrap();
        let baseline_bytes = fs::read(baseline.path().join("capture.png")).unwrap();
        let mut changed = original;
        changed.put_pixel(1, 0, Rgba([100, 40, 60, 255]));
        changed.save(output.path().join("capture.png")).unwrap();
        let mut report = fixture_report();
        compare(&mut report, output.path(), baseline.path()).unwrap();
        assert!(!report.is_failure());
        assert_eq!(report.scenarios[0].steps.len(), 3);
        assert!(
            report.scenarios[0].findings[0]
                .message
                .contains("1/4 pixels")
        );
        assert_eq!(
            report.scenarios[0].steps[2].semantic["comparison"]["changed_pixels"],
            1
        );
        assert!(output.path().join("comparison/0000-diff.png").is_file());
        assert_eq!(
            fs::read(baseline.path().join("capture.png")).unwrap(),
            baseline_bytes
        );
        assert_eq!(
            fs::read(baseline.path().join("report.json")).unwrap(),
            report_bytes
        );
    }

    #[test]
    fn complete_reports_above_sixteen_mib_remain_valid_baselines() {
        let baseline = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let mut current = Report::new(RunMode::Visual, json!({}));
        let path = baseline.path().join("report.json");
        let mut file = File::create_new(&path).unwrap();
        serde_json::to_writer(&mut file, &current).unwrap();
        // Stream valid JSON whitespace rather than allocate a huge metadata
        // value or keep a large fixture in the repository.
        let padding = [b' '; 8192];
        for _ in 0..=(16 * 1024 * 1024 / padding.len()) {
            file.write_all(&padding).unwrap();
        }
        drop(file);
        assert!(fs::metadata(&path).unwrap().len() > 16 * 1024 * 1024);
        compare(&mut current, output.path(), baseline.path()).unwrap();
        assert!(current.scenarios.is_empty());
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
    }

    #[test]
    fn reports_above_the_new_limit_are_rejected_before_reading() {
        let file = tempfile::NamedTempFile::new().unwrap();
        // A sparse file exercises the metadata guard without allocating or
        // writing a 128 MiB test payload.
        file.as_file().set_len(MAX_REPORT_BYTES + 1).unwrap();
        let error = read_bounded(file.path(), MAX_REPORT_BYTES).unwrap_err();
        assert!(error.contains("byte limit"));
    }

    fn fixture_report() -> Report {
        let mut report = Report::new(RunMode::Visual, json!({}));
        let mut scenario = ScenarioReport::new("workspace");
        scenario.steps.push(StepReport {
            semantic: json!({"checkpoint": "initial"}),
            screenshot: Some("capture.png".into()),
            ..StepReport::default()
        });
        report.scenarios.push(scenario);
        report
    }
}
