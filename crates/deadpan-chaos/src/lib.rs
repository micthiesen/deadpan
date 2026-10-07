//! Stable-Rust adversarial input runner for Deadpan's Gate G suite.
//!
//! The pinned stable toolchain cannot run cargo-fuzz/libFuzzer, so this crate
//! supplies a small deterministic replacement: seed corpora, stacked byte and
//! JSON mutations, verdict-class novelty in place of edge coverage, panic
//! capture, per-case time and allocation bounds, delta minimization and
//! reproducible artifacts. Native decoders run one case per child process so
//! aborts, signals and sanitizer reports are observed rather than fatal.
//!
//! Modes, selected by environment:
//! - Regression (default): seeded and deterministic, a fixed iteration count,
//!   suitable for `cargo test`.
//! - Campaign: `DEADPAN_CHAOS_SECONDS=N` runs each target for N seconds from a
//!   fresh seed (or `DEADPAN_CHAOS_SEED`). `cargo xtask chaos` drives this.
//! - Replay: `DEADPAN_CHAOS_REPLAY=<file>` runs exactly one saved input.
//!
//! See docs/ADVERSARIAL.md.

mod alloc;
pub mod mutate;
mod process;

pub use alloc::{
    CountingAllocator, allocation_baseline, allocation_peak_since, installed as allocator_installed,
};
pub use mutate::{Rng, fnv1a, json_bombs, mutate_bytes, mutate_json};
pub use process::{ChildRunner, child_input, finish_child};

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A target's classification of one input. Rejections carry a stable class
/// (typically the typed error code) used as a novelty signal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    Rejected(String),
}

/// `Err` reports an invariant violation found by the target itself (for
/// example an accepted document that does not round-trip).
pub type Outcome = Result<Verdict, String>;

/// How seeds are mutated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Bytes,
    /// JSON-aware node mutation half of the time, raw bytes otherwise; also
    /// adds depth/size bombs to the seed corpus.
    Json,
    /// One selector byte followed by 4-byte big-endian length-prefixed JSON
    /// frames. Two thirds of mutations rewrite one frame's JSON body and
    /// recompute its length, so the semantic validators are reached.
    Frames,
}

/// Per-target configuration. Limits are failures, not hints.
#[derive(Clone, Debug)]
pub struct Target {
    pub name: &'static str,
    pub shape: Shape,
    /// Deterministic regression executions (overridable by
    /// `DEADPAN_CHAOS_ITERATIONS`).
    pub iterations: u64,
    pub max_input_bytes: usize,
    /// Wall time per case, confirmed by one re-run before reporting.
    pub max_case_time: Duration,
    /// Peak bytes allocated by the target thread per case, when the counting
    /// allocator is installed.
    pub max_case_alloc: u64,
    /// Executions spent minimizing each distinct failure.
    pub minimize_budget: u32,
}

impl Target {
    pub fn bytes(name: &'static str) -> Self {
        Self {
            name,
            shape: Shape::Bytes,
            iterations: 400,
            max_input_bytes: 1 << 20,
            max_case_time: Duration::from_secs(5),
            max_case_alloc: 256 << 20,
            minimize_budget: 1_500,
        }
    }

    pub fn frames(name: &'static str) -> Self {
        Self {
            shape: Shape::Frames,
            max_input_bytes: 512 * 1024,
            iterations: 1500,
            ..Self::bytes(name)
        }
    }

    pub fn json(name: &'static str) -> Self {
        Self {
            shape: Shape::Json,
            max_input_bytes: 8 << 20,
            ..Self::bytes(name)
        }
    }

    pub fn iterations(mut self, iterations: u64) -> Self {
        self.iterations = iterations;
        self
    }

    pub fn max_input_bytes(mut self, bytes: usize) -> Self {
        self.max_input_bytes = bytes;
        self
    }

    pub fn max_case_time(mut self, time: Duration) -> Self {
        self.max_case_time = time;
        self
    }

    pub fn minimize_budget(mut self, executions: u32) -> Self {
        self.minimize_budget = executions;
        self
    }

    pub fn max_case_alloc(mut self, bytes: u64) -> Self {
        self.max_case_alloc = bytes;
        self
    }
}

#[derive(Clone, Debug)]
enum Mode {
    Regression(u64),
    Campaign(Duration),
    Replay(PathBuf),
}

/// One distinct failure after minimization.
#[derive(Clone, Debug)]
pub struct Failure {
    pub class: String,
    pub detail: String,
    pub artifact: Option<PathBuf>,
    pub input_len: usize,
}

/// Summary of one target run. Printed as one `deadpan-chaos {json}` line and
/// appended to `$DEADPAN_CHAOS_OUT/report.jsonl` when that is set.
#[derive(Clone, Debug)]
pub struct Report {
    pub target: &'static str,
    pub mode: &'static str,
    pub seed: u64,
    pub executions: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub classes: BTreeMap<String, u64>,
    pub corpus: usize,
    pub max_case: Duration,
    pub max_alloc: Option<u64>,
    pub elapsed: Duration,
    pub failures: Vec<Failure>,
}

impl Report {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "target": self.target,
            "mode": self.mode,
            "seed": format!("{:#018x}", self.seed),
            "executions": self.executions,
            "accepted": self.accepted,
            "rejected": self.rejected,
            "distinct_classes": self.classes.len(),
            // The 48 most frequent classes; the rest are counted, not listed.
            "classes": self.top_classes(48),
            "corpus": self.corpus,
            "max_case_ms": self.max_case.as_secs_f64() * 1e3,
            "max_alloc_bytes": self.max_alloc,
            "elapsed_s": self.elapsed.as_secs_f64(),
            "failures": self.failures.iter().map(|failure| serde_json::json!({
                "class": failure.class,
                "detail": failure.detail,
                "artifact": failure.artifact.as_ref().map(|path| path.display().to_string()),
                "input_len": failure.input_len,
            })).collect::<Vec<_>>(),
        })
    }

    fn top_classes(&self, limit: usize) -> serde_json::Map<String, serde_json::Value> {
        let mut ranked: Vec<(&String, &u64)> = self.classes.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        ranked
            .into_iter()
            .take(limit)
            .map(|(class, count)| (class.clone(), serde_json::Value::from(*count)))
            .collect()
    }

    /// Panics with every distinct failure and its reproduction command.
    pub fn assert_clean(&self) {
        if self.failures.is_empty() {
            return;
        }
        let mut message = format!(
            "{} distinct adversarial failure(s) in {} (seed {:#018x}):\n",
            self.failures.len(),
            self.target,
            self.seed
        );
        for failure in &self.failures {
            let _ = writeln!(
                message,
                "- {} ({} bytes): {}\n  replay: DEADPAN_CHAOS_REPLAY={}",
                failure.class,
                failure.input_len,
                failure.detail,
                failure
                    .artifact
                    .as_ref()
                    .map_or_else(|| "<unsaved>".into(), |path| path.display().to_string())
            );
        }
        panic!("{message}");
    }
}

/// The committed coverage-guided corpus for `name` (`fuzz/corpus/<name>`),
/// in a stable order; empty when the directory is absent.
pub fn committed_corpus(name: &str) -> Vec<Vec<u8>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/corpus")
        .join(name);
    seeds_from_dir(&directory)
}

/// Writes each seed to `<directory>/<fnv1a>`; content-addressed, so repeated
/// exports are idempotent. Export is a developer action: a failure panics.
fn export_seeds(directory: &Path, corpus: &[Vec<u8>]) {
    std::fs::create_dir_all(directory).expect("create seed export directory");
    for seed in corpus {
        let name = format!("{:016x}", fnv1a(seed));
        std::fs::write(directory.join(name), seed).expect("write exported seed");
    }
}

fn environment_mode(target: &Target) -> Mode {
    if let Some(path) = std::env::var_os("DEADPAN_CHAOS_REPLAY") {
        return Mode::Replay(path.into());
    }
    if let Some(seconds) = std::env::var("DEADPAN_CHAOS_SECONDS")
        .ok()
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
    {
        return Mode::Campaign(Duration::from_secs_f64(seconds));
    }
    let iterations = std::env::var("DEADPAN_CHAOS_ITERATIONS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(target.iterations);
    Mode::Regression(iterations)
}

fn environment_seed(target: &Target, mode: &Mode) -> u64 {
    if let Some(seed) = std::env::var("DEADPAN_CHAOS_SEED").ok().and_then(|text| {
        let text = text.trim_start_matches("0x");
        u64::from_str_radix(text, 16).ok()
    }) {
        return seed ^ fnv1a(target.name.as_bytes());
    }
    match mode {
        Mode::Campaign(_) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |time| time.as_nanos() as u64);
            now ^ fnv1a(target.name.as_bytes())
        }
        _ => fnv1a(target.name.as_bytes()),
    }
}

/// Where artifacts go: `$DEADPAN_CHAOS_OUT`, else the system temp directory.
pub fn output_directory() -> PathBuf {
    std::env::var_os("DEADPAN_CHAOS_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("deadpan-chaos"))
}

/// Reads every regular file in `directory` (sorted) as a seed.
pub fn seeds_from_dir(directory: &Path) -> Vec<Vec<u8>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| std::fs::read(path).ok())
        .collect()
}

thread_local! {
    static CAPTURING: RefCell<Option<String>> = const { RefCell::new(None) };
}
static HOOK: Once = Once::new();

/// Installs a panic hook that records panics raised inside a fuzz case on
/// that thread instead of printing them, and defers to the previous hook for
/// every other panic (ordinary test failures keep their output).
fn install_hook() {
    HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let captured = CAPTURING.with(|slot| {
                let mut slot = slot.borrow_mut();
                match slot.as_mut() {
                    Some(text) => {
                        let payload = info
                            .payload()
                            .downcast_ref::<&str>()
                            .map(|text| (*text).to_owned())
                            .or_else(|| info.payload().downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "non-string panic payload".into());
                        let location = info
                            .location()
                            .map(|location| format!("{}:{}", location.file(), location.line()))
                            .unwrap_or_default();
                        *text = format!("{location}: {payload}");
                        true
                    }
                    None => false,
                }
            });
            if !captured {
                previous(info);
            }
        }));
    });
}

struct Case {
    result: Result<Verdict, (String, String)>,
    elapsed: Duration,
    alloc: Option<u64>,
}

fn normalize(text: &str) -> String {
    // serde_json positions say where, not what; drop them from the class.
    let text = text.split(" at line ").next().unwrap_or(text);
    let mut class = String::new();
    for c in text.chars().take(160) {
        let c = if c.is_ascii_digit() { '#' } else { c };
        if !(c == '#' && class.ends_with('#')) {
            class.push(c);
        }
    }
    if let Some(index) = class.find('\n') {
        class.truncate(index);
    }
    class
}

fn execute<F: Fn(&[u8]) -> Outcome>(target: &Target, run: &F, input: &[u8]) -> Case {
    CAPTURING.with(|slot| *slot.borrow_mut() = Some(String::new()));
    let baseline = alloc::begin();
    let started = Instant::now();
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| run(input)));
    let elapsed = started.elapsed();
    let peak = alloc::peak_since(baseline);
    let captured = CAPTURING
        .with(|slot| slot.borrow_mut().take())
        .unwrap_or_default();
    let alloc = alloc::installed().then_some(peak);
    let result = match outcome {
        Ok(Ok(verdict)) => {
            if let Some(peak) = alloc.filter(|peak| *peak > target.max_case_alloc) {
                Err((
                    "allocation bound exceeded".into(),
                    format!("peak {peak} bytes > {} bytes", target.max_case_alloc),
                ))
            } else {
                Ok(verdict)
            }
        }
        Ok(Err(violation)) => Err((format!("invariant: {}", normalize(&violation)), violation)),
        Err(_) => Err((format!("panic: {}", normalize(&captured)), captured)),
    };
    Case {
        result,
        elapsed,
        alloc,
    }
}

/// Classifies one case, re-running a slow case once to filter scheduler noise.
fn judge<F: Fn(&[u8]) -> Outcome>(target: &Target, run: &F, input: &[u8]) -> Case {
    let case = execute(target, run, input);
    if case.result.is_ok() && case.elapsed > target.max_case_time {
        let again = execute(target, run, input);
        if again.result.is_ok() && again.elapsed > target.max_case_time {
            return Case {
                result: Err((
                    "time bound exceeded".into(),
                    format!(
                        "{:?} and {:?} > {:?}",
                        case.elapsed, again.elapsed, target.max_case_time
                    ),
                )),
                ..again
            };
        }
        return again;
    }
    case
}

/// Delta-debugging style reduction that keeps the failure class.
fn minimize<F: Fn(&[u8]) -> Outcome>(
    target: &Target,
    run: &F,
    input: &[u8],
    class: &str,
) -> Vec<u8> {
    let mut best = input.to_vec();
    let mut budget = target.minimize_budget;
    let same = |candidate: &[u8]| -> bool {
        matches!(&execute(target, run, candidate).result, Err((found, _)) if found == class)
    };
    if class == "time bound exceeded" {
        return best; // Re-running slow cases thousands of times is not useful.
    }
    let mut chunk = best.len().div_ceil(2).max(1);
    while chunk >= 1 && budget > 0 {
        let mut start = 0;
        let mut reduced = false;
        while start < best.len() && budget > 0 {
            let end = (start + chunk).min(best.len());
            let mut candidate = best.clone();
            candidate.drain(start..end);
            budget -= 1;
            if same(&candidate) {
                best = candidate;
                reduced = true;
            } else {
                start = end;
            }
        }
        if !reduced {
            if chunk == 1 {
                break;
            }
            chunk = chunk.div_ceil(2);
        }
    }
    best
}

fn save_artifact(target: &Target, class: &str, input: &[u8]) -> Option<PathBuf> {
    let directory = output_directory().join("crashes").join(target.name);
    std::fs::create_dir_all(&directory).ok()?;
    let path = directory.join(format!("{:016x}.bin", fnv1a(input)));
    std::fs::write(&path, input).ok()?;
    let _ = std::fs::write(path.with_extension("txt"), class);
    Some(path)
}

fn emit(report: &Report) {
    let line = report.to_json();
    eprintln!("deadpan-chaos {line}");
    if let Some(directory) = std::env::var_os("DEADPAN_CHAOS_OUT") {
        let directory = PathBuf::from(directory);
        if std::fs::create_dir_all(&directory).is_ok() {
            use std::io::Write as _;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(directory.join("report.jsonl"))
            {
                // One write per record keeps parallel targets' lines whole.
                let _ = file.write_all(format!("{line}\n").as_bytes());
            }
        }
    }
}

/// Runs `run` over the seeds and their mutations under `target`'s limits.
/// The caller asserts with [`Report::assert_clean`].
pub fn fuzz<F: Fn(&[u8]) -> Outcome>(target: Target, seeds: Vec<Vec<u8>>, run: F) -> Report {
    install_hook();
    let mode = environment_mode(&target);
    let seed = environment_seed(&target, &mode);
    let mut rng = Rng::new(seed);
    let started = Instant::now();
    let mut report = Report {
        target: target.name,
        mode: match mode {
            Mode::Regression(_) => "regression",
            Mode::Campaign(_) => "campaign",
            Mode::Replay(_) => "replay",
        },
        seed,
        executions: 0,
        accepted: 0,
        rejected: 0,
        classes: BTreeMap::new(),
        corpus: 0,
        max_case: Duration::ZERO,
        max_alloc: None,
        elapsed: Duration::ZERO,
        failures: Vec::new(),
    };
    let mut corpus: Vec<Vec<u8>> = seeds
        .into_iter()
        .map(|mut seed| {
            seed.truncate(target.max_input_bytes);
            seed
        })
        .collect();
    if target.shape == Shape::Json {
        corpus.extend(
            json_bombs()
                .into_iter()
                .filter(|bomb| bomb.len() <= target.max_input_bytes),
        );
    }
    if corpus.is_empty() {
        corpus.push(Vec::new());
    }
    if let Some(directory) = std::env::var_os("DEADPAN_CHAOS_EXPORT_SEEDS") {
        // Seed export for the coverage-guided libFuzzer targets (`cargo xtask
        // fuzz`): write the initial corpus and run nothing.
        export_seeds(&PathBuf::from(directory).join(target.name), &corpus);
        report.elapsed = started.elapsed();
        return report;
    }
    // Replay the committed, minimized libFuzzer corpus of the same name, so
    // the stable regression suite re-executes every input the coverage-guided
    // campaigns kept, including fixed crash reproducers.
    corpus.extend(committed_corpus(target.name).into_iter().map(|mut input| {
        input.truncate(target.max_input_bytes);
        input
    }));
    let parsed: Vec<Option<serde_json::Value>> = corpus
        .iter()
        .map(|bytes| match target.shape {
            Shape::Json => serde_json::from_slice(bytes).ok(),
            Shape::Bytes | Shape::Frames => None,
        })
        .collect();
    let mut parsed = parsed;
    let mut seen_failures: BTreeMap<String, ()> = BTreeMap::new();

    let mut record = |report: &mut Report,
                      corpus: &mut Vec<Vec<u8>>,
                      parsed: &mut Vec<Option<serde_json::Value>>,
                      input: Vec<u8>,
                      case: Case| {
        report.executions += 1;
        report.max_case = report.max_case.max(case.elapsed);
        if let Some(peak) = case.alloc {
            report.max_alloc = Some(report.max_alloc.unwrap_or(0).max(peak));
        }
        match case.result {
            Ok(verdict) => {
                let key = match &verdict {
                    Verdict::Accepted => {
                        report.accepted += 1;
                        "accepted".to_owned()
                    }
                    Verdict::Rejected(class) => {
                        report.rejected += 1;
                        normalize(class)
                    }
                };
                let count = report.classes.entry(key).or_insert(0);
                *count += 1;
                // Novelty: keep inputs that reach a new verdict class, plus a
                // small sample of accepted ones that keep mutation productive.
                let keep =
                    *count == 1 || (verdict == Verdict::Accepted && (*count).is_multiple_of(64));
                if keep && corpus.len() < 2048 {
                    if target.shape == Shape::Json {
                        parsed.push(serde_json::from_slice(&input).ok());
                    } else {
                        parsed.push(None);
                    }
                    corpus.push(input);
                }
            }
            Err((class, detail)) => {
                if seen_failures.insert(class.clone(), ()).is_none() && report.failures.len() < 8 {
                    let minimized = minimize(&target, &run, &input, &class);
                    let artifact = save_artifact(&target, &class, &minimized);
                    report.failures.push(Failure {
                        class,
                        detail: detail.chars().take(600).collect(),
                        artifact,
                        input_len: minimized.len(),
                    });
                }
            }
        }
    };

    match mode {
        Mode::Replay(path) => {
            let input = std::fs::read(&path).unwrap_or_default();
            let case = judge(&target, &run, &input);
            record(&mut report, &mut corpus, &mut parsed, input, case);
        }
        Mode::Regression(_) | Mode::Campaign(_) => {
            // Seeds themselves first, then every seed's structural truncations.
            let initial: Vec<Vec<u8>> = corpus.clone();
            for input in &initial {
                let case = judge(&target, &run, input);
                record(&mut report, &mut corpus, &mut parsed, input.clone(), case);
            }
            for input in &initial {
                for fraction in [1_usize, 2, 3, 5, 7] {
                    let cut = input.len() * fraction / 8;
                    let prefix = input[..cut].to_vec();
                    let case = judge(&target, &run, &prefix);
                    record(&mut report, &mut corpus, &mut parsed, prefix, case);
                }
            }
            let mut iteration = 0_u64;
            loop {
                match mode {
                    Mode::Regression(limit) if iteration >= limit => break,
                    Mode::Campaign(budget) if started.elapsed() >= budget => break,
                    _ => {}
                }
                iteration += 1;
                let index = rng.below(corpus.len());
                let donor = rng.below(corpus.len());
                let reframed = if target.shape == Shape::Frames && !rng.chance(1, 3) {
                    let donor_bytes = corpus[donor].clone();
                    mutate::mutate_frames(
                        &mut rng,
                        &corpus[index],
                        &donor_bytes,
                        target.max_input_bytes,
                    )
                } else {
                    None
                };
                let input = match (&parsed[index], target.shape) {
                    _ if reframed.is_some() => reframed.unwrap_or_default(),
                    (Some(value), Shape::Json) if rng.chance(1, 2) => mutate_json(
                        &mut rng,
                        value,
                        parsed[donor].as_ref(),
                        target.max_input_bytes,
                    ),
                    _ => {
                        let mut input = corpus[index].clone();
                        let other = corpus[donor].clone();
                        mutate_bytes(&mut rng, &mut input, &other, target.max_input_bytes);
                        input
                    }
                };
                let case = judge(&target, &run, &input);
                record(&mut report, &mut corpus, &mut parsed, input, case);
            }
        }
    }
    report.corpus = corpus.len();
    report.elapsed = started.elapsed();
    emit(&report);
    report
}

/// Maps any error's `Display` to a rejection class, keeping only the leading
/// structural words so numeric details do not flood the novelty table.
pub fn reject(error: impl std::fmt::Display) -> Outcome {
    let text = error.to_string();
    if text.trim().is_empty() {
        return Err("rejection carried an empty diagnostic".into());
    }
    Ok(Verdict::Rejected(text.chars().take(96).collect()))
}

/// A 4-byte big-endian length-prefixed frame, the shared worker framing.
pub fn frame(body: &[u8]) -> Vec<u8> {
    let mut bytes = u32::try_from(body.len())
        .unwrap_or(u32::MAX)
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(body);
    bytes
}

/// Splits a fuzz input into a selector byte and a stream, so one target can
/// drive several readers (host side, worker side, ...).
pub fn select(input: &[u8]) -> (u8, &[u8]) {
    input
        .split_first()
        .map_or((0, &[][..]), |(selector, rest)| (*selector, rest))
}

/// Reads frames from `stream` until a clean end, an error or `max_frames`.
/// `read` returns `Ok(Some(()))` per admitted frame, `Ok(None)` at a clean
/// end. A reader that neither consumes input nor ends is an invariant failure.
pub fn read_stream(
    mut stream: &[u8],
    max_frames: usize,
    mut read: impl FnMut(&mut &[u8]) -> Result<Option<()>, String>,
) -> Outcome {
    let mut admitted = 0;
    for _ in 0..max_frames {
        let before = stream.len();
        match read(&mut stream) {
            Ok(Some(())) => {
                if stream.len() == before {
                    return Err("reader admitted a frame without consuming input".into());
                }
                admitted += 1;
            }
            Ok(None) => {
                return Ok(if admitted > 0 {
                    Verdict::Accepted
                } else {
                    Verdict::Rejected("empty stream".into())
                });
            }
            Err(error) => return reject(error),
        }
    }
    Ok(Verdict::Accepted)
}

/// Shared stable/nightly framed-protocol exercise. Selectors are identical in
/// both runners: 0 reads and round-trips requests, 1 reads responses, and 2
/// reads one initial request then classifies its responses against that exact
/// request. Callbacks keep this development crate independent of worker jobs.
pub fn protocol_stream<Request, Response, Classifier>(
    input: &[u8],
    mut read_request: impl FnMut(&mut &[u8]) -> Result<Option<Request>, String>,
    mut write_request: impl FnMut(&mut Vec<u8>, &Request) -> Result<(), String>,
    mut read_response: impl FnMut(&mut &[u8]) -> Result<Option<Response>, String>,
    mut prepare: impl FnMut(&Request) -> Result<Classifier, String>,
    mut classify: impl FnMut(&Classifier, &Response) -> Result<(), String>,
) -> Outcome {
    let (selector, stream) = select(input);
    match selector % 3 {
        0 => {
            let mut violation = None;
            let outcome = read_stream(stream, 64, |reader| {
                let Some(request) = read_request(reader)? else {
                    return Ok(None);
                };
                let mut encoded = Vec::new();
                let problem = match write_request(&mut encoded, &request) {
                    Err(error) => Some(format!("admitted request did not re-encode: {error}")),
                    Ok(()) => {
                        let mut again = encoded.as_slice();
                        match read_request(&mut again) {
                            Ok(Some(_)) if again.is_empty() => None,
                            _ => Some("admitted request did not round-trip completely".into()),
                        }
                    }
                };
                if let Some(problem) = problem {
                    violation = Some(problem.clone());
                    return Err(problem);
                }
                Ok(Some(()))
            });
            violation.map_or(outcome, Err)
        }
        1 => read_stream(stream, 64, |reader| Ok(read_response(reader)?.map(|_| ()))),
        _ => {
            let mut reader = stream;
            let request = match read_request(&mut reader) {
                Ok(Some(request)) => request,
                Ok(None) => return Ok(Verdict::Rejected("missing initial request".into())),
                Err(error) => return reject(error),
            };
            if reader.len() == stream.len() {
                return Err("reader admitted an initial request without consuming input".into());
            }
            let classifier = match prepare(&request) {
                Ok(classifier) => classifier,
                Err(error) => return reject(error),
            };
            read_stream(reader, 64, |reader| match read_response(reader)? {
                Some(response) => {
                    classify(&classifier, &response)?;
                    Ok(Some(()))
                }
                None => Ok(None),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_selector_two_classifies_the_response_against_its_own_request() {
        fn read(input: &mut &[u8]) -> Result<Option<u8>, String> {
            let Some((first, rest)) = input.split_first() else {
                return Ok(None);
            };
            *input = rest;
            Ok(Some(*first))
        }
        let mut classified = Vec::new();
        let outcome = protocol_stream(
            &[2, 37, 37, 12],
            read,
            |output, request| {
                output.push(*request);
                Ok(())
            },
            read,
            |request| Ok(*request),
            |request, response| {
                classified.push((*request, *response));
                if request == response {
                    Ok(())
                } else {
                    Err("different attempt".into())
                }
            },
        )
        .unwrap();
        assert_eq!(classified, [(37, 37), (37, 12)]);
        assert!(
            matches!(outcome, Verdict::Rejected(reason) if reason.contains("different attempt"))
        );
    }

    #[test]
    fn protocol_request_roundtrip_errors_are_invariant_failures() {
        fn read(input: &mut &[u8]) -> Result<Option<u8>, String> {
            let Some((first, rest)) = input.split_first() else {
                return Ok(None);
            };
            *input = rest;
            Ok(Some(*first))
        }
        let outcome = protocol_stream(
            &[0, 1],
            read,
            |_, _| Err("broken writer".into()),
            read,
            |request| Ok(*request),
            |_: &u8, _: &u8| Ok(()),
        );
        assert!(outcome.unwrap_err().contains("did not re-encode"));
    }

    #[test]
    fn panics_are_captured_minimized_and_saved() {
        let report = fuzz(
            Target::bytes("self-test-panic").iterations(2_000),
            vec![b"hello world".to_vec()],
            |input| {
                if input.windows(2).any(|pair| pair == [0xff, 0xff]) {
                    panic!("boom");
                }
                Ok(Verdict::Rejected(format!("len{}", input.len().min(3))))
            },
        );
        assert!(report.executions > 2_000);
        assert_eq!(report.failures.len(), 1, "{report:?}");
        assert_eq!(report.failures[0].input_len, 2, "minimized to the trigger");
        assert!(report.failures[0].class.starts_with("panic: "));
    }

    #[test]
    fn regression_mode_is_deterministic() {
        let run = |input: &[u8]| {
            Ok(Verdict::Rejected(format!(
                "{}",
                input.first().copied().unwrap_or(0) % 7
            )))
        };
        let a = fuzz(
            Target::bytes("self-test-determinism").iterations(300),
            vec![b"abc".to_vec()],
            run,
        );
        let b = fuzz(
            Target::bytes("self-test-determinism").iterations(300),
            vec![b"abc".to_vec()],
            run,
        );
        assert_eq!(a.classes, b.classes);
        assert_eq!(a.corpus, b.corpus);
    }

    #[test]
    fn invariant_violations_are_failures() {
        let report = fuzz(
            Target::json("self-test-invariant").iterations(50),
            vec![b"{}".to_vec()],
            |input| {
                if input.is_empty() {
                    Err("empty accepted".into())
                } else {
                    Ok(Verdict::Accepted)
                }
            },
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.class.starts_with("invariant: "))
        );
    }

    #[test]
    fn child_aborts_hangs_and_verdicts_are_observed() {
        if let Some(input) = child_input() {
            match input.first() {
                Some(b'a') => std::process::abort(),
                Some(b'h') => std::thread::sleep(Duration::from_secs(60)),
                Some(b'p') => panic!("child panic"),
                _ => {}
            }
            finish_child(Ok(Verdict::Rejected("fine".into())));
        }
        let mut runner = ChildRunner::new("tests::child_aborts_hangs_and_verdicts_are_observed");
        runner.timeout = Duration::from_secs(3);
        assert_eq!(runner.run(b"ok"), Ok(Verdict::Rejected("fine".into())));
        let abort = runner.run(b"a").unwrap_err();
        assert!(abort.contains("crashed"), "{abort}");
        let hang = runner.run(b"h").unwrap_err();
        assert!(hang.contains("timeout"), "{hang}");
        let panic = runner.run(b"p").unwrap_err();
        assert!(panic.contains("without a verdict"), "{panic}");
    }
}
