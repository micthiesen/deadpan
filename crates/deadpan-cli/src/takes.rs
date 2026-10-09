//! Revision-bound named snapshots, shared with the native project's writer.

use std::path::Path;

use serde::Deserialize;

use crate::live_project::{ShortOperation, dispatch_short};
use crate::{CliError, read_request, write_json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    protocol: u32,
    request: deadpan_store::takes::Request,
}

pub fn run_list(arguments: &[&str]) -> Result<(), CliError> {
    let [path] = arguments else {
        return Err(CliError::Usage(
            "usage: project takes <project.deadpan>".into(),
        ));
    };
    write_json(&dispatch_short(
        Path::new(path),
        None,
        ShortOperation::TakeCatalog,
    )?)
}

pub fn run(arguments: &[&str]) -> Result<(), CliError> {
    let (path, file, dry_run) = match arguments {
        [path, "--json", file] => (*path, *file, false),
        [path, "--json", file, "--dry-run"] => (*path, *file, true),
        _ => {
            return Err(CliError::Usage(
                "usage: project take <project.deadpan> --json <request.json> [--dry-run]".into(),
            ));
        }
    };
    let envelope: Envelope = serde_json::from_str(&read_request(Path::new(file))?)?;
    if envelope.protocol != 1 {
        return Err(CliError::Protocol(envelope.protocol));
    }
    write_json(&dispatch_short(
        Path::new(path),
        Some(envelope.request.project_id.clone()),
        ShortOperation::Take {
            request: Box::new(envelope.request),
            dry_run,
        },
    )?)
}
