//! Developer entrypoint. Reports scaffold capabilities without probing private data.

use std::io::{self, Write};
use std::process::ExitCode;

use deadpan_core::{FrameRate, MIX_SAMPLE_RATE, ProjectFrame};
use serde::Serialize;

#[derive(Serialize)]
struct DoctorReport {
    schema_version: u32,
    application: &'static str,
    version: &'static str,
    os: &'static str,
    architecture: &'static str,
    stage: &'static str,
    internal_audio_sample_rate: u32,
    timing_probe: TimingProbe,
    unimplemented: &'static [&'static str],
}

#[derive(Serialize)]
struct TimingProbe {
    frame_rate_numerator: u32,
    frame_rate_denominator: u32,
    frame: i64,
    sample_boundary: i64,
}

fn doctor() -> Result<DoctorReport, deadpan_core::TimeError> {
    let rate = FrameRate::new(30_000, 1_001)?;
    Ok(DoctorReport {
        schema_version: 1,
        application: "deadpan",
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        stage: "development-foundation",
        internal_audio_sample_rate: MIX_SAMPLE_RATE,
        timing_probe: TimingProbe {
            frame_rate_numerator: rate.numerator(),
            frame_rate_denominator: rate.denominator(),
            frame: 1,
            sample_boundary: rate.audio_boundary(ProjectFrame(1))?.0,
        },
        unimplemented: &[
            "project-storage",
            "media-decode",
            "audio-output",
            "editing-commands",
            "media-preview",
            "analysis",
            "ai-generation",
            "youtube-import",
            "export",
            "distribution",
        ],
    })
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [command] if command == "doctor" => {
            let report = doctor()?;
            let mut output = io::stdout().lock();
            serde_json::to_writer_pretty(&mut output, &report)?;
            writeln!(output)?;
        }
        [] => println!("Usage: deadpan-cli doctor\n\nEmit foundation diagnostics as JSON."),
        [command] if command == "--help" || command == "-h" => {
            println!("Usage: deadpan-cli doctor\n\nEmit foundation diagnostics as JSON.");
        }
        _ => return Err("Unknown command. Usage: deadpan-cli doctor".into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("deadpan: {error}");
            ExitCode::FAILURE
        }
    }
}
