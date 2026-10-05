//! `verify-export PROJECT --movie MOVIE [...]` argument handling.

use std::{
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use deadpan_core::RevisionId;

use super::{AudioSelection, FrameSelection, Thresholds, VerifyError, VerifyRequest, verify};

pub const USAGE: &str = "verify-export <project.deadpan> --movie <movie.mp4> [--revision <id>] [--frames <N,N,...> | --every <N>] [--samples <START:END,...> | --no-audio] [--report <new.json>]";
/// Generous single-invocation bound; references and decode are cooperative.
const DEADLINE: Duration = Duration::from_secs(2 * 60 * 60);

pub fn parse(arguments: &[&str]) -> Result<(VerifyRequest, Option<PathBuf>), VerifyError> {
    let usage = || VerifyError::Request(format!("usage: {USAGE}"));
    let (package, mut rest) = arguments.split_first().ok_or_else(usage)?;
    let mut movie = None;
    let mut revision = None;
    let mut frames = FrameSelection::Automatic;
    let mut audio = AudioSelection::Automatic;
    let mut report = None;
    let mut frames_set = false;
    let mut audio_set = false;
    while let Some((flag, tail)) = rest.split_first() {
        let (value, tail) = match *flag {
            "--no-audio" => (None, tail),
            _ => {
                let (value, tail) = tail.split_first().ok_or_else(usage)?;
                (Some(*value), tail)
            }
        };
        rest = tail;
        match (*flag, value) {
            ("--movie", Some(value)) if movie.is_none() => movie = Some(PathBuf::from(value)),
            ("--revision", Some(value)) if revision.is_none() => {
                revision = Some(
                    RevisionId::new(value)
                        .map_err(|error| VerifyError::Request(error.to_string()))?,
                );
            }
            ("--frames", Some(value)) if !frames_set => {
                frames = FrameSelection::List(
                    value
                        .split(',')
                        .map(|item| item.trim().parse::<u64>())
                        .collect::<Result<_, _>>()
                        .map_err(|_| VerifyError::Request("invalid --frames list".into()))?,
                );
                frames_set = true;
            }
            ("--every", Some(value)) if !frames_set => {
                frames = FrameSelection::Every(
                    value
                        .parse()
                        .map_err(|_| VerifyError::Request("invalid --every".into()))?,
                );
                frames_set = true;
            }
            ("--samples", Some(value)) if !audio_set => {
                audio = AudioSelection::Windows(
                    value.split(',').map(window).collect::<Result<_, _>>()?,
                );
                audio_set = true;
            }
            ("--no-audio", None) if !audio_set => {
                audio = AudioSelection::None;
                audio_set = true;
            }
            ("--report", Some(value)) if report.is_none() => report = Some(PathBuf::from(value)),
            _ => return Err(usage()),
        }
    }
    Ok((
        VerifyRequest {
            package: PathBuf::from(package),
            movie: movie.ok_or_else(usage)?,
            revision,
            frames,
            audio,
            thresholds: Thresholds::default(),
        },
        report,
    ))
}

fn window(value: &str) -> Result<Range<i64>, VerifyError> {
    let invalid = || VerifyError::Request(format!("invalid sample window {value:?}"));
    let (start, end) = value.split_once(':').ok_or_else(invalid)?;
    Ok(start.trim().parse().map_err(|_| invalid())?..end.trim().parse().map_err(|_| invalid())?)
}

/// Writes the complete report to stdout (and optionally a new file), then
/// fails with `ExportVerificationMismatch` when any check failed.
pub fn run(arguments: &[&str]) -> Result<(), crate::CliError> {
    let (request, report_path) = parse(arguments)?;
    // SIGINT/SIGTERM cancel cooperatively between bounded decode/render steps.
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut handlers = Vec::new();
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        handlers.push(signal_hook::flag::register(signal, Arc::clone(&cancelled))?);
    }
    let result = verify(&request, &cancelled, Instant::now() + DEADLINE);
    for handler in handlers {
        signal_hook::low_level::unregister(handler);
    }
    let report = result?;
    if let Some(path) = report_path {
        write_new(&path, &serde_json::to_vec_pretty(&report)?)?;
    }
    crate::write_json(&report)?;
    if report.passed {
        Ok(())
    } else {
        Err(VerifyError::Mismatch(report.failures.len()).into())
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
