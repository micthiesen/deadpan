//! Durations of earlier completed jobs, for estimates before a run has
//! measured its own rate (specification §18.4, DP-18.6).
//!
//! The history is per user and per machine, never per project: it lives in
//! one small file under Application Support, outside authored state, project
//! history and every project database. It is a rebuildable observation, so
//! an unreadable, oversized or foreign-version file is ignored with a
//! diagnostic rather than failing anything.
//!
//! Each sample is the wall-clock running time of one successful,
//! uninterrupted run and its work size in the bucket's own unit (seconds of
//! audio, frames, pictures). A bucket names the job kind and the parameters
//! that change its speed (model, raster, codec), so only comparable runs are
//! pooled. An estimate needs [`MIN_SAMPLES`] samples in the same bucket and
//! uses their median seconds per unit, which one slow outlier cannot move.

use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const FILE_VERSION: u32 = 1;
/// Fewer comparable runs than this give no estimate.
pub const MIN_SAMPLES: usize = 3;
/// The newest samples kept per bucket; older ones age out.
const MAX_SAMPLES: usize = 16;
/// Buckets kept; the least recently recorded is dropped first.
const MAX_BUCKETS: usize = 64;
const MAX_BUCKET_LENGTH: usize = 160;
/// Far above what [`MAX_BUCKETS`] full buckets serialize to.
const MAX_BYTES: u64 = 256 * 1024;
/// Runs shorter than this are dominated by startup noise; longer than this
/// (a week) is not a plausible uninterrupted run.
const MIN_SECONDS: f64 = 0.5;
const MAX_SECONDS: f64 = 7.0 * 24.0 * 3600.0;

/// The work one run does: its comparable bucket and size.
#[derive(Clone, Debug, PartialEq)]
pub struct Work {
    bucket: String,
    units: f64,
}

impl Work {
    /// `parameters` names everything that changes the speed per unit, for
    /// example `"model=base.en"` or `"raster=3840x2160"`. `None` for a
    /// size that cannot be measured.
    pub fn new(kind: super::JobKind, parameters: &str, units: f64) -> Option<Self> {
        let bucket = format!("{}/{parameters}", kind.history_key());
        (units.is_finite() && units > 0.0 && bucket.len() <= MAX_BUCKET_LENGTH)
            .then_some(Self { bucket, units })
    }

    #[cfg(test)]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }
}

/// An estimate from earlier runs of comparable work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Earlier {
    /// Expected running time of the whole job.
    pub total: Duration,
    /// How many earlier runs it is based on.
    pub runs: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Sample {
    units: f64,
    seconds: f64,
}

impl Sample {
    fn valid(self) -> bool {
        self.units.is_finite()
            && self.units > 0.0
            && self.seconds.is_finite()
            && (MIN_SECONDS..=MAX_SECONDS).contains(&self.seconds)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Bucket {
    /// Recording order across buckets, for dropping the least recent.
    used: u64,
    samples: VecDeque<Sample>,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    version: u32,
    buckets: BTreeMap<String, Bucket>,
}

/// The per-user history. Without a path it is kept in memory only (tests,
/// smoke runs and CLI-free hosts never read the user's file).
#[derive(Debug, Default)]
pub struct JobHistory {
    path: Option<PathBuf>,
    buckets: BTreeMap<String, Bucket>,
    /// Why the file was ignored or could not be written, if it was.
    diagnostic: Option<String>,
    /// A newer Deadpan's file is never overwritten.
    read_only: bool,
}

impl JobHistory {
    /// `Application Support/Deadpan/job-history.json` for the native app.
    pub fn user() -> Self {
        match crate::keymap_file::application_support_directory() {
            Ok(directory) => Self::at(directory.join("Deadpan").join("job-history.json")),
            Err(error) => Self {
                diagnostic: Some(format!("Job history is unavailable: {error}")),
                ..Self::default()
            },
        }
    }

    /// The history kept in `path`. A missing file is an empty history; any
    /// other problem is ignored and reported once through [`Self::diagnostic`].
    pub fn at(path: PathBuf) -> Self {
        let mut history = Self {
            path: Some(path.clone()),
            ..Self::default()
        };
        match read(&path) {
            Ok(Some(buckets)) => history.buckets = buckets,
            Ok(None) => {}
            Err(Unreadable::Newer(version)) => {
                history.read_only = true;
                history.diagnostic = Some(format!(
                    "Job history at {} has version {version}; this Deadpan reads version {FILE_VERSION} and leaves it unchanged.",
                    path.display()
                ));
            }
            Err(Unreadable::Invalid(reason)) => {
                history.diagnostic = Some(format!(
                    "Job history at {} was ignored and will be replaced: {reason}",
                    path.display()
                ));
            }
        }
        if let Some(diagnostic) = &history.diagnostic {
            eprintln!("{diagnostic}");
        }
        history
    }

    #[cfg(test)]
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    /// The expected running time of `work` from at least [`MIN_SAMPLES`]
    /// earlier runs in its bucket: their median seconds per unit times its
    /// size. `None` without enough comparable runs.
    pub fn estimate(&self, work: &Work) -> Option<Earlier> {
        let samples = &self.buckets.get(&work.bucket)?.samples;
        if samples.len() < MIN_SAMPLES {
            return None;
        }
        let mut rates: Vec<f64> = samples
            .iter()
            .map(|sample| sample.seconds / sample.units)
            .collect();
        rates.sort_by(f64::total_cmp);
        let middle = rates.len() / 2;
        let median = if rates.len().is_multiple_of(2) {
            f64::midpoint(rates[middle - 1], rates[middle])
        } else {
            rates[middle]
        };
        let seconds = median * work.units;
        (seconds.is_finite() && seconds > 0.0 && seconds <= MAX_SECONDS).then(|| Earlier {
            total: Duration::from_secs_f64(seconds),
            runs: samples.len(),
        })
    }

    /// Record one successful, uninterrupted run and save the file.
    pub fn record(&mut self, work: &Work, running: Duration) {
        let sample = Sample {
            units: work.units,
            seconds: running.as_secs_f64(),
        };
        if !sample.valid() {
            return;
        }
        let used = self
            .buckets
            .values()
            .map(|bucket| bucket.used)
            .max()
            .unwrap_or(0)
            + 1;
        let bucket = self.buckets.entry(work.bucket.clone()).or_default();
        bucket.used = used;
        bucket.samples.push_back(sample);
        while bucket.samples.len() > MAX_SAMPLES {
            bucket.samples.pop_front();
        }
        while self.buckets.len() > MAX_BUCKETS {
            let oldest = self
                .buckets
                .iter()
                .min_by_key(|(_, bucket)| bucket.used)
                .map(|(key, _)| key.clone())
                .expect("nonempty");
            self.buckets.remove(&oldest);
        }
        self.save();
    }

    fn save(&mut self) {
        let Some(path) = self.path.clone().filter(|_| !self.read_only) else {
            return;
        };
        if let Err(error) = write(&path, &self.buckets) {
            let diagnostic = format!("Job history could not be saved: {error}");
            eprintln!("{diagnostic}");
            self.diagnostic = Some(diagnostic);
        }
    }

    #[cfg(test)]
    fn samples(&self, bucket: &str) -> usize {
        self.buckets
            .get(bucket)
            .map_or(0, |bucket| bucket.samples.len())
    }
}

enum Unreadable {
    Newer(u32),
    Invalid(String),
}

fn read(path: &Path) -> Result<Option<BTreeMap<String, Bucket>>, Unreadable> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Unreadable::Invalid(error.to_string())),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Unreadable::Invalid(error.to_string()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Unreadable::Invalid("it is larger than 256 KiB".into()));
    }
    let wire: Wire =
        serde_json::from_slice(&bytes).map_err(|error| Unreadable::Invalid(error.to_string()))?;
    if wire.version > FILE_VERSION {
        return Err(Unreadable::Newer(wire.version));
    }
    if wire.version != FILE_VERSION {
        return Err(Unreadable::Invalid(format!(
            "unsupported version {}",
            wire.version
        )));
    }
    if wire.buckets.len() > MAX_BUCKETS {
        return Err(Unreadable::Invalid(format!(
            "it has more than {MAX_BUCKETS} buckets"
        )));
    }
    // Keep every well-formed bucket; drop invalid samples individually.
    let buckets = wire
        .buckets
        .into_iter()
        .filter(|(key, _)| key.len() <= MAX_BUCKET_LENGTH)
        .filter_map(|(key, mut bucket)| {
            bucket.samples.retain(|sample| sample.valid());
            while bucket.samples.len() > MAX_SAMPLES {
                bucket.samples.pop_front();
            }
            (!bucket.samples.is_empty()).then_some((key, bucket))
        })
        .collect();
    Ok(Some(buckets))
}

/// Replace the file atomically: a crash leaves the old or the new history.
fn write(path: &Path, buckets: &BTreeMap<String, Bucket>) -> Result<(), String> {
    let directory = path.parent().ok_or("the history path has no directory")?;
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(&Wire {
        version: FILE_VERSION,
        buckets: buckets.clone(),
    })
    .map_err(|error| error.to_string())?;
    let temporary = directory.join(format!(".job-history.{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        error.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobKind;

    fn work(units: f64) -> Work {
        Work::new(JobKind::Transcription, "model=base.en", units).expect("work")
    }

    fn seconds(estimate: Option<Earlier>) -> Option<f64> {
        estimate.map(|earlier| (earlier.total.as_secs_f64() * 1000.0).round() / 1000.0)
    }

    #[test]
    fn needs_three_comparable_runs_and_uses_their_median_rate() {
        let mut history = JobHistory::default();
        history.record(&work(100.0), Duration::from_secs(10));
        history.record(&work(200.0), Duration::from_secs(20));
        assert_eq!(
            history.estimate(&work(50.0)),
            None,
            "two runs are not enough"
        );
        // A slow outlier (2 s per unit) does not move the median (0.1).
        history.record(&work(10.0), Duration::from_secs(20));
        let estimate = history.estimate(&work(50.0)).expect("three runs");
        assert_eq!(estimate.runs, 3);
        assert_eq!(seconds(Some(estimate)), Some(5.0));
        // An even count takes the midpoint of the middle two rates.
        history.record(&work(100.0), Duration::from_secs(30));
        assert_eq!(seconds(history.estimate(&work(10.0))), Some(2.0));
    }

    #[test]
    fn buckets_keep_kinds_and_parameters_apart() {
        let mut history = JobHistory::default();
        for _ in 0..3 {
            history.record(&work(10.0), Duration::from_secs(10));
        }
        let other_model = Work::new(JobKind::Transcription, "model=large", 10.0).expect("work");
        let other_kind = Work::new(JobKind::Shots, "model=base.en", 10.0).expect("work");
        assert!(history.estimate(&work(10.0)).is_some());
        assert_eq!(history.estimate(&other_model), None);
        assert_eq!(history.estimate(&other_kind), None);
        assert_ne!(other_model.bucket(), other_kind.bucket());
    }

    #[test]
    fn rejects_unmeasurable_work_and_implausible_runs() {
        assert_eq!(Work::new(JobKind::Shots, "", 0.0), None);
        assert_eq!(Work::new(JobKind::Shots, "", f64::NAN), None);
        assert_eq!(Work::new(JobKind::Shots, &"x".repeat(200), 1.0), None);
        let mut history = JobHistory::default();
        history.record(&work(1.0), Duration::from_millis(10));
        history.record(&work(1.0), Duration::from_secs(8 * 24 * 3600));
        assert_eq!(history.samples(work(1.0).bucket()), 0);
    }

    #[test]
    fn stays_bounded() {
        let mut history = JobHistory::default();
        for run in 0..(MAX_SAMPLES + 5) {
            history.record(&work(1.0), Duration::from_secs(1 + run as u64));
        }
        assert_eq!(history.samples(work(1.0).bucket()), MAX_SAMPLES);
        // The oldest samples aged out: the median reflects recent runs.
        assert_eq!(
            seconds(history.estimate(&work(1.0))),
            Some(f64::midpoint(13.0, 14.0))
        );
        for parameter in 0..(MAX_BUCKETS + 3) {
            let work = Work::new(JobKind::Shots, &format!("raster={parameter}"), 1.0).unwrap();
            history.record(&work, Duration::from_secs(1));
        }
        assert_eq!(history.buckets.len(), MAX_BUCKETS);
        // The least recently recorded buckets were dropped first.
        assert_eq!(history.samples(work(1.0).bucket()), 0);
        assert!(history.buckets.contains_key("shots/raster=66"));
    }

    #[test]
    fn persists_and_reloads() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("Support/Deadpan/job-history.json");
        let mut history = JobHistory::at(path.clone());
        assert_eq!(history.diagnostic(), None);
        for _ in 0..3 {
            history.record(&work(10.0), Duration::from_secs(4));
        }
        let reloaded = JobHistory::at(path.clone());
        assert_eq!(reloaded.diagnostic(), None);
        assert_eq!(seconds(reloaded.estimate(&work(10.0))), Some(4.0));
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size < MAX_BYTES);
    }

    #[test]
    fn ignores_a_corrupt_file_with_a_diagnostic_and_replaces_it() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("job-history.json");
        for corrupt in [
            b"{not json".to_vec(),
            br#"{"version":0,"buckets":{}}"#.to_vec(),
            vec![b' '; (MAX_BYTES + 1) as usize],
        ] {
            std::fs::write(&path, corrupt).unwrap();
            let mut history = JobHistory::at(path.clone());
            assert!(
                history
                    .diagnostic()
                    .is_some_and(|text| text.contains("ignored"))
            );
            assert_eq!(history.estimate(&work(1.0)), None);
            history.record(&work(1.0), Duration::from_secs(1));
            assert_eq!(JobHistory::at(path.clone()).diagnostic(), None, "replaced");
        }
        // Invalid samples are dropped one by one; the rest are kept.
        std::fs::write(
            &path,
            r#"{"version":1,"buckets":{"transcription/model=base.en":{"used":1,"samples":[
                {"units":10,"seconds":1},{"units":10,"seconds":1},{"units":-1,"seconds":1},
                {"units":10,"seconds":1}]}}}"#,
        )
        .unwrap();
        let history = JobHistory::at(path.clone());
        assert_eq!(history.diagnostic(), None);
        assert_eq!(history.estimate(&work(10.0)).map(|e| e.runs), Some(3));
    }

    #[test]
    fn leaves_a_newer_version_unchanged() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("job-history.json");
        let newer = br#"{"version":9,"buckets":{}}"#;
        std::fs::write(&path, newer).unwrap();
        let mut history = JobHistory::at(path.clone());
        assert!(
            history
                .diagnostic()
                .is_some_and(|text| text.contains("version 9"))
        );
        history.record(&work(1.0), Duration::from_secs(1));
        assert_eq!(std::fs::read(&path).unwrap(), newer);
    }
}
