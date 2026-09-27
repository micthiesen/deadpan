//! Portable evidence from production UI replay. Rendering never changes pass/fail.

use std::fs;
use std::io;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunMode {
    #[default]
    Visual,
    Performance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Report {
    pub mode: RunMode,
    pub metadata: Value,
    pub scenarios: Vec<ScenarioReport>,
}

impl Report {
    pub fn new(mode: RunMode, metadata: Value) -> Self {
        Self {
            mode,
            metadata,
            scenarios: Vec::new(),
        }
    }

    pub fn is_failure(&self) -> bool {
        self.scenarios.iter().any(ScenarioReport::is_failure)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct ScenarioReport {
    pub name: String,
    pub steps: Vec<StepReport>,
    pub checks: Vec<Check>,
    pub timings: Vec<TimingMetric>,
    pub findings: Vec<Finding>,
    pub skipped: Vec<String>,
}

impl ScenarioReport {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn is_failure(&self) -> bool {
        self.checks.iter().any(|check| !check.passed)
            || self
                .findings
                .iter()
                .any(|finding| finding.severity == Severity::Failure)
            || self.timings.iter().any(|metric| {
                metric
                    .samples
                    .iter()
                    .any(|sample| sample.outcome != SampleOutcome::Completed)
            })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct StepReport {
    pub input: String,
    pub frame: u64,
    pub virtual_time_ms: f64,
    pub elapsed_wall_ms: f64,
    pub semantic: Value,
    /// A screenshot already written beneath the report directory.
    pub screenshot: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Check {
    pub name: String,
    pub passed: bool,
    pub expected: Value,
    pub actual: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct TimingMetric {
    pub name: String,
    pub samples: Vec<TimingSample>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SampleOutcome {
    #[default]
    Completed,
    Failed,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub(crate) struct TimingSample {
    pub elapsed_ms: f64,
    pub outcome: SampleOutcome,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Severity {
    #[default]
    Warning,
    Failure,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Finding {
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Serialize)]
struct LatencySummary {
    n: usize,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}

#[derive(Debug, Serialize)]
struct TimingSummary {
    samples: usize,
    completed: Option<LatencySummary>,
    failed: usize,
    timed_out: usize,
}

impl TimingMetric {
    fn summary(&self) -> TimingSummary {
        let completed: Vec<_> = self
            .samples
            .iter()
            .filter(|sample| sample.outcome == SampleOutcome::Completed)
            .map(|sample| sample.elapsed_ms)
            .collect();
        TimingSummary {
            samples: self.samples.len(),
            completed: quantiles(&completed),
            failed: self
                .samples
                .iter()
                .filter(|sample| sample.outcome == SampleOutcome::Failed)
                .count(),
            timed_out: self
                .samples
                .iter()
                .filter(|sample| sample.outcome == SampleOutcome::TimedOut)
                .count(),
        }
    }
}

/// Nearest-rank percentiles. Failed and timed-out attempts are reported separately.
fn quantiles(values: &[f64]) -> Option<LatencySummary> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let percentile = |percent: usize| {
        let rank = n / 100 * percent + (n % 100 * percent).div_ceil(100);
        sorted[rank.saturating_sub(1)]
    };
    Some(LatencySummary {
        n,
        p50_ms: percentile(50),
        p95_ms: percentile(95),
        p99_ms: percentile(99),
        max_ms: sorted[n - 1],
    })
}

/// Writes evidence even when a scenario failed. Existing screenshot artifacts
/// are neither moved nor removed. The caller owns the output directory.
pub(crate) fn write_report(report: &Report, directory: &Path) -> io::Result<()> {
    validate(report, directory)?;
    let mut json = serde_json::to_value(report).map_err(io::Error::other)?;
    json["schema_version"] = Value::from(1);
    json["failed"] = Value::from(report.is_failure());
    for (index, scenario) in report.scenarios.iter().enumerate() {
        json["scenarios"][index]["failed"] = Value::from(scenario.is_failure());
        for (metric_index, metric) in scenario.timings.iter().enumerate() {
            json["scenarios"][index]["timings"][metric_index]["summary"] =
                serde_json::to_value(metric.summary()).map_err(io::Error::other)?;
        }
    }
    let bytes = serde_json::to_vec_pretty(&json).map_err(io::Error::other)?;
    fs::write(directory.join("report.json"), bytes)?;
    fs::write(directory.join("report.html"), render_html(report))
}

fn validate(report: &Report, directory: &Path) -> io::Result<()> {
    let root = directory.canonicalize()?;
    for scenario in &report.scenarios {
        for step in &scenario.steps {
            finite_time(step.virtual_time_ms)?;
            finite_time(step.elapsed_wall_ms)?;
            if let Some(image) = &step.screenshot {
                validate_image_path(image)?;
                let image = root.join(image).canonicalize()?;
                if !image.starts_with(&root) || !image.is_file() {
                    return Err(invalid(
                        "Screenshot must be a file inside the report directory",
                    ));
                }
            }
        }
        for metric in &scenario.timings {
            for sample in &metric.samples {
                finite_time(sample.elapsed_ms)?;
            }
        }
    }
    Ok(())
}

fn finite_time(value: f64) -> io::Result<()> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(invalid(
            "Recorded milliseconds must be finite and nonnegative",
        ))
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn validate_image_path(image: &str) -> io::Result<()> {
    let path = Path::new(image);
    let ordinary_characters = image
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_. /".contains(&byte));
    let ordinary_components = path
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    let image_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "png" | "jpg" | "jpeg" | "webp"));
    if !image.is_empty() && ordinary_characters && ordinary_components && image_extension {
        Ok(())
    } else {
        Err(invalid("Screenshot must have a plain relative image path"))
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn json_text(value: &Value) -> String {
    escape(&serde_json::to_string_pretty(value).expect("JSON values serialize"))
}

fn render_html(report: &Report) -> String {
    let mut html = String::from(HTML_START);
    let status = if report.is_failure() { "FAIL" } else { "PASS" };
    let mode = match report.mode {
        RunMode::Visual => {
            "Visual capture. Readback can affect timings; use a separate performance run for latency qualification."
        }
        RunMode::Performance => {
            "Performance run. Timing uses recorded wall-clock samples; virtual time describes replay scheduling."
        }
    };
    html.push_str(&format!(
        "<header><h1>Deadpan interaction report <span class=\"{}\">{status}</span></h1><p>{mode}</p><p><a href=\"report.json\">Full JSON evidence</a></p><details><summary>Run metadata</summary><pre>{}</pre></details></header><main>",
        if report.is_failure() { "fail" } else { "pass" },
        json_text(&report.metadata)
    ));
    if report.scenarios.is_empty() {
        html.push_str("<p>No scenarios were recorded.</p>");
    }
    for scenario in &report.scenarios {
        html.push_str(&format!(
            "<section><h2>{} <span class=\"{}\">{}</span></h2>",
            escape(&scenario.name),
            if scenario.is_failure() {
                "fail"
            } else {
                "pass"
            },
            if scenario.is_failure() {
                "FAIL"
            } else {
                "PASS"
            }
        ));
        for finding in &scenario.findings {
            let (class, label) = match finding.severity {
                Severity::Warning => ("warning", "Warning"),
                Severity::Failure => ("fail", "Failure"),
            };
            html.push_str(&format!(
                "<p class=\"{class}\"><strong>{label}:</strong> {}</p>",
                escape(&finding.message)
            ));
        }
        if !scenario.skipped.is_empty() {
            html.push_str("<h3>Skipped or unimplemented boundaries</h3><ul>");
            for skipped in &scenario.skipped {
                html.push_str(&format!("<li>{}</li>", escape(skipped)));
            }
            html.push_str("</ul>");
        }
        if !scenario.checks.is_empty() {
            html.push_str("<h3>Checks</h3><ul class=\"checks\">");
            for check in &scenario.checks {
                html.push_str(&format!(
                    "<li><strong class=\"{}\">{}</strong> {}<details><summary>Expected and actual</summary><h4>Expected</h4><pre>{}</pre><h4>Actual</h4><pre>{}</pre></details></li>",
                    if check.passed { "pass" } else { "fail" },
                    if check.passed { "PASS" } else { "FAIL" },
                    escape(&check.name),
                    json_text(&check.expected),
                    json_text(&check.actual)
                ));
            }
            html.push_str("</ul>");
        }
        render_timings(&mut html, &scenario.timings);
        render_frames(&mut html, &scenario.steps);
        html.push_str("<details><summary>Every recorded step and semantic snapshot</summary><ol class=\"steps\">");
        for step in &scenario.steps {
            html.push_str(&format!(
                "<li><strong>{}</strong><p>Frame {} · virtual {:.3} ms · elapsed wall {:.3} ms</p><pre>{}</pre></li>",
                escape(&step.input), step.frame, step.virtual_time_ms, step.elapsed_wall_ms,
                json_text(&step.semantic)
            ));
        }
        html.push_str("</ol></details></section>");
    }
    html.push_str(HTML_END);
    html
}

fn render_timings(html: &mut String, metrics: &[TimingMetric]) {
    if metrics.is_empty() {
        html.push_str("<p>No timing samples were recorded.</p>");
        return;
    }
    html.push_str("<h3>Timing samples</h3><p>Nearest-rank percentiles in milliseconds, using completed attempts. All failed and timed-out attempts remain in the JSON evidence.</p><div class=\"table-scroll\"><table><thead><tr><th>Stage</th><th>n completed</th><th>p50</th><th>p95</th><th>p99</th><th>max</th><th>failed</th><th>timed out</th><th>total</th></tr></thead><tbody>");
    for metric in metrics {
        let summary = metric.summary();
        html.push_str(&format!("<tr><th>{}</th>", escape(&metric.name)));
        if let Some(latency) = summary.completed {
            html.push_str(&format!(
                "<td>{}</td><td>{:.3}</td><td>{:.3}</td><td>{:.3}</td><td>{:.3}</td>",
                latency.n, latency.p50_ms, latency.p95_ms, latency.p99_ms, latency.max_ms
            ));
        } else {
            html.push_str("<td>0</td><td colspan=\"4\">No completed samples</td>");
        }
        html.push_str(&format!(
            "<td>{}</td><td>{}</td><td>{}</td></tr>",
            summary.failed, summary.timed_out, summary.samples
        ));
    }
    html.push_str("</tbody></table></div>");
}

fn render_frames(html: &mut String, steps: &[StepReport]) {
    let shots: Vec<_> = steps
        .iter()
        .filter(|step| step.screenshot.is_some())
        .collect();
    let Some(first) = shots.first() else {
        return;
    };
    html.push_str(&format!(
        "<div class=\"sequence\"><h3>Captured frames</h3><p>Playback advances through captures, not real-time video.</p><div class=\"player-controls\"><button class=\"play\" type=\"button\">Play captures</button><label>Capture <input class=\"seek\" type=\"range\" min=\"0\" max=\"{}\" value=\"0\"></label><span class=\"position\"></span></div><figure><img class=\"preview\" src=\"{}\" alt=\"{}\"><figcaption></figcaption></figure><div class=\"contact-sheet\">",
        shots.len() - 1,
        escape(first.screenshot.as_deref().expect("filtered screenshot")),
        escape(&first.input)
    ));
    for step in shots {
        let image = escape(step.screenshot.as_deref().expect("filtered screenshot"));
        let caption = escape(&format!(
            "Frame {} · {:.3} ms virtual · {}",
            step.frame, step.virtual_time_ms, step.input
        ));
        html.push_str(&format!(
            "<button class=\"shot\" type=\"button\" data-src=\"{image}\" data-caption=\"{caption}\"><img src=\"{image}\" alt=\"{caption}\" loading=\"lazy\"><span>{caption}</span></button>"
        ));
    }
    html.push_str("</div></div>");
}

const HTML_START: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Deadpan interaction report</title><style>
:root{color-scheme:dark;font:15px/1.5 system-ui,sans-serif;background:#17191d;color:#e9ebf4}
body{margin:0 auto;padding:28px;max-width:1480px}h1{font-size:26px}h2{font-size:21px}h3{font-size:17px}
header,section{border:1px solid #373d49;border-radius:8px;padding:20px;margin-bottom:24px;background:#202329}
header p,section p{color:#aab0bf}a{color:#c4b5fd}.pass{color:#a7f3d0}.fail{color:#ff9494}.warning{color:#f6d365}
h1 span,h2 span{font-size:14px;margin-left:12px}summary,button,input{cursor:pointer}details{margin:12px 0}
pre{overflow:auto;max-height:420px;padding:12px;background:#17191d;white-space:pre-wrap;overflow-wrap:anywhere}
table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}th,td{text-align:left;padding:8px;border-bottom:1px solid #373d49;white-space:nowrap}.table-scroll{overflow:auto}
button{font:inherit;color:inherit;background:#292d35;border:1px solid #545d70;border-radius:4px;padding:6px 12px}button:focus-visible{outline:2px solid #c4b5fd;outline-offset:3px}
.player-controls{display:flex;gap:16px;align-items:center;flex-wrap:wrap}figure{margin:16px 0}.preview{display:block;max-width:100%;max-height:75vh;margin:auto}figcaption{padding:8px;color:#aab0bf;overflow-wrap:anywhere}
.contact-sheet{display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:12px;margin:16px 0}.shot{padding:8px;text-align:left;min-width:0}.shot img{width:100%;height:150px;object-fit:contain;background:#17191d}.shot span{display:block;font-size:12px;overflow-wrap:anywhere}.shot[aria-current=true]{border-color:#c4b5fd;background:#393750}.checks li,.steps li{margin:12px 0}
</style></head><body>"##;

const HTML_END: &str = r##"</main><script>
document.querySelectorAll('.sequence').forEach(section => {
  const shots = Array.from(section.querySelectorAll('.shot'));
  const preview = section.querySelector('.preview');
  const caption = section.querySelector('figcaption');
  const seek = section.querySelector('.seek');
  const play = section.querySelector('.play');
  const position = section.querySelector('.position');
  let current = 0;
  let timer;
  const stop = () => { clearInterval(timer); timer = undefined; play.textContent = 'Play captures'; };
  const show = index => {
    current = index;
    preview.src = shots[index].dataset.src;
    preview.alt = shots[index].dataset.caption;
    caption.textContent = shots[index].dataset.caption;
    seek.value = String(index);
    position.textContent = `${index + 1} / ${shots.length}`;
    shots.forEach((shot, i) => shot.setAttribute('aria-current', String(i === index)));
  };
  shots.forEach((shot, index) => shot.addEventListener('click', () => { stop(); show(index); }));
  seek.addEventListener('input', () => { stop(); show(Number(seek.value)); });
  play.disabled = shots.length < 2;
  play.addEventListener('click', () => {
    if (timer !== undefined) { stop(); return; }
    if (current === shots.length - 1) show(0);
    play.textContent = 'Pause captures';
    timer = setInterval(() => {
      if (current + 1 === shots.length) { stop(); return; }
      show(current + 1);
    }, 250);
  });
  document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); });
  show(0);
});
</script></body></html>"##;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nearest_rank_quantiles_cover_empty_small_and_unsorted_samples() {
        assert!(quantiles(&[]).is_none());
        let one = quantiles(&[4.0]).unwrap();
        assert_eq!(
            (one.n, one.p50_ms, one.p99_ms, one.max_ms),
            (1, 4.0, 4.0, 4.0)
        );
        let pair = quantiles(&[9.0, 2.0]).unwrap();
        assert_eq!((pair.p50_ms, pair.p95_ms, pair.p99_ms), (2.0, 9.0, 9.0));
        let values: Vec<_> = (1..=100).rev().map(f64::from).collect();
        let hundred = quantiles(&values).unwrap();
        assert_eq!(
            (hundred.p50_ms, hundred.p95_ms, hundred.p99_ms),
            (50.0, 95.0, 99.0)
        );
    }

    #[test]
    fn labels_are_escaped_and_paths_cannot_be_urls_or_traversal() {
        let mut report = Report::new(
            RunMode::Visual,
            json!({"host": "</pre><script>bad()</script>"}),
        );
        let mut scenario = ScenarioReport::new("<img src=x onerror='bad()'>");
        scenario.findings.push(Finding {
            severity: Severity::Warning,
            message: "<script>bad()</script>".into(),
        });
        report.scenarios.push(scenario);
        let html = render_html(&report);
        assert!(!html.contains("<script>bad()"));
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;img src=x onerror=&#39;bad()&#39;&gt;"));
        for path in [
            "/tmp/a.png",
            "../a.png",
            "a/../../b.png",
            "./a.png",
            "https://x/a.png",
            "a\\b.png",
            "a%2fb.png",
            "a.png?x",
            "a.svg",
            "a\".png",
        ] {
            assert!(validate_image_path(path).is_err(), "admitted {path}");
        }
        assert!(validate_image_path("frames/scene-001.png").is_ok());
    }

    #[test]
    fn failed_scenarios_and_timeouts_keep_all_evidence() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("frame.png"), b"retained artifact").unwrap();
        let mut report = Report::new(RunMode::Performance, json!({"fixture": "test"}));
        let mut scenario = ScenarioReport::new("replay failure");
        scenario.checks.push(Check {
            name: "selection survived".into(),
            passed: false,
            expected: json!("beat-a"),
            actual: json!("beat-b"),
        });
        scenario.steps.push(StepReport {
            input: "click beat".into(),
            screenshot: Some("frame.png".into()),
            ..StepReport::default()
        });
        scenario.timings.push(TimingMetric {
            name: "picture".into(),
            samples: vec![
                TimingSample {
                    elapsed_ms: 7.0,
                    outcome: SampleOutcome::Completed,
                },
                TimingSample {
                    elapsed_ms: 80.0,
                    outcome: SampleOutcome::Failed,
                },
                TimingSample {
                    elapsed_ms: 100.0,
                    outcome: SampleOutcome::TimedOut,
                },
            ],
        });
        report.scenarios.push(scenario);
        assert!(report.is_failure());
        write_report(&report, directory.path()).unwrap();
        let json: Value =
            serde_json::from_slice(&fs::read(directory.path().join("report.json")).unwrap())
                .unwrap();
        assert_eq!(json["failed"], true);
        let metric = &json["scenarios"][0]["timings"][0];
        assert_eq!(metric["samples"].as_array().unwrap().len(), 3);
        assert_eq!(metric["summary"]["completed"]["n"], 1);
        assert_eq!(metric["summary"]["failed"], 1);
        assert_eq!(metric["summary"]["timed_out"], 1);
        assert_eq!(
            fs::read(directory.path().join("frame.png")).unwrap(),
            b"retained artifact"
        );
        let html = fs::read_to_string(directory.path().join("report.html")).unwrap();
        assert!(html.contains("class=\"fail\">FAIL"));
        assert!(html.contains("contact-sheet"));
        assert!(html.contains("data-src=\"frame.png\""));
    }

    #[cfg(unix)]
    #[test]
    fn screenshot_symlinks_cannot_escape_the_report_directory() {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("outside.png"), b"outside").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("outside.png"),
            directory.path().join("link.png"),
        )
        .unwrap();
        let mut report = Report::new(RunMode::Visual, Value::Null);
        let mut scenario = ScenarioReport::new("unsafe screenshot");
        scenario.steps.push(StepReport {
            screenshot: Some("link.png".into()),
            ..StepReport::default()
        });
        report.scenarios.push(scenario);
        assert_eq!(
            write_report(&report, directory.path()).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
