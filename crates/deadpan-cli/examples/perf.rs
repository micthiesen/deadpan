//! Release benchmark stages for docs/qualification performance reports.
//! `cargo xtask perf` builds this in release mode, copies the fixtures and runs
//! each stage under `/usr/bin/time -l`; run a stage directly only on a package
//! copy, because `edit` commits to the package it is given.
//!
//! Usage:
//!   perf seek PACKAGE [--cold N] [--warm N] [--step N] [--seed N]
//!             [--proxy-cache NEW_DIR --worker MEDIA_WORKER]
//!   perf proxy-build PACKAGE --proxy-cache DIR --worker MEDIA_WORKER
//!   perf playback PACKAGE [--seconds N] [--start-frame N]
//!   perf edit PACKAGE [--cycles N] [--seed N] [--kinds split,pause,wrap]
//!   perf make-large NEW_PACKAGE [--beats N]
//!   perf scale [--sizes 1000,10000,...] [--source PACKAGE --fragments N]
//!
//! Each stage prints one JSON object on stdout. Wall-clock intervals use the
//! monotonic clock; GPU completion waits on the submitted Metal work. These are
//! headless measurements of the shared media/store/plan paths, not of the
//! native window's input routing or physical display scanout.

use std::error::Error;
use std::time::Instant;

use serde_json::{Value, json};

#[path = "perf/edit.rs"]
mod edit;
#[path = "perf/gpu.rs"]
mod gpu;
#[path = "../../xtask/src/percentile.rs"]
mod percentile;
#[path = "perf/playback.rs"]
mod playback;
#[path = "perf/proxy.rs"]
mod proxy;
#[path = "perf/scale.rs"]
mod scale;
#[path = "perf/seek.rs"]
mod seek;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (stage, rest) = arguments
        .split_first()
        .ok_or("usage: perf seek|playback|edit|make-large|scale ...")?;
    let options = Options::parse(rest)?;
    let started = Instant::now();
    let mut report = match stage.as_str() {
        "seek" => seek::run(&options)?,
        "playback" => playback::run(&options)?,
        "edit" => edit::run(&options)?,
        "make-large" => edit::make_large(&options)?,
        "scale" => scale::run(&options)?,
        "proxy-build" => proxy::build_stage(&options)?,
        other => return Err(format!("unknown perf stage {other}").into()),
    };
    report["stage"] = json!(stage);
    report["stage_wall_ms"] = json!(ms(started));
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Positional package plus `--name value` pairs.
pub struct Options {
    pub package: Option<std::path::PathBuf>,
    values: std::collections::BTreeMap<String, String>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self> {
        let mut package = None;
        let mut values = std::collections::BTreeMap::new();
        let mut iter = arguments.iter();
        while let Some(argument) = iter.next() {
            if let Some(name) = argument.strip_prefix("--") {
                let value = iter.next().ok_or(format!("--{name} needs a value"))?;
                values.insert(name.to_owned(), value.clone());
            } else if package.is_none() {
                package = Some(std::path::PathBuf::from(argument));
            } else {
                return Err(format!("unexpected argument {argument}").into());
            }
        }
        Ok(Self { package, values })
    }

    pub fn package(&self) -> Result<&std::path::Path> {
        self.package
            .as_deref()
            .ok_or_else(|| "this stage needs a package path".into())
    }

    pub fn number(&self, name: &str, default: u64) -> Result<u64> {
        self.values
            .get(name)
            .map_or(Ok(default), |value| value.parse().map_err(Into::into))
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
}

pub fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

/// Nearest-rank distribution (shared with `cargo xtask perf`). Raw samples
/// stay in the report where outliers matter.
pub fn summary(samples: &[f64]) -> Value {
    match percentile::distribution(samples) {
        None => json!({"n": 0}),
        Some(d) => json!({
            "n": d.n,
            "min": round(d.min),
            "p50": round(d.p50),
            "p95": round(d.p95),
            "max": round(d.max),
            "mean": round(d.mean),
        }),
    }
}

pub fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Small deterministic generator, so a rerun visits the same positions.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }
    pub fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound.max(1)
    }
}
