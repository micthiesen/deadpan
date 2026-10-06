use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_store::original_media::{
    LinkedOriginal, OriginalContentId, OriginalMediaError, OriginalMediaLimits, OriginalOwnership,
    moved_location,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

use crate::live_project::preparation::{self, PreparationCommand};
use crate::{CliError, write_json};

fn content(value: &str) -> Result<OriginalContentId, CliError> {
    OriginalContentId::new(value.to_owned())
        .map_err(|e| CliError::Store(StoreError::OriginalMedia(OriginalMediaError::Identity(e))))
}

pub(super) fn run(action: &str, arguments: &[&str]) -> Result<(), CliError> {
    let limits = OriginalMediaLimits::default();
    let cancelled = AtomicBool::new(false);
    match (action, arguments) {
        ("retain-original", [project, source])
        | ("retain-original", [project, source, "--linked"]) => {
            let ownership = if arguments.len() == 3 {
                OriginalOwnership::linked_at(Path::new(source))
            } else {
                OriginalOwnership::Managed
            };
            let mut store = match ProjectStore::open(Path::new(project), AccessMode::ReadWrite) {
                Ok(store) => store,
                Err(StoreError::AlreadyOpen) => {
                    return preparation::run(
                        Path::new(project),
                        PreparationCommand::Retain {
                            path: Path::new(source).to_owned(),
                            ownership: ownership.into(),
                        },
                    );
                }
                Err(error) => return Err(error.into()),
            };
            let retained =
                store.retain_original(Path::new(source), ownership, limits, &cancelled)?;
            write_json(
                &serde_json::json!({ "protocol": 1, "retained_original": retained, "authored_asset_registered": false }),
            )
        }
        ("originals", [project]) | ("originals", [project, "--after", _]) => {
            let after = arguments.get(2).map(|value| content(value)).transpose()?;
            let store = ProjectStore::open(Path::new(project), AccessMode::ReadOnly)?;
            let records = store.original_records(after.as_ref(), 100)?;
            let cursor = records
                .last()
                .map(|r| r.object().content().digest().to_owned());
            write_json(
                &serde_json::json!({ "protocol": 1, "originals": records, "next_after": cursor }),
            )
        }
        ("original-provenance", [project, digest]) => {
            let key = content(digest)?;
            let store = ProjectStore::open(Path::new(project), AccessMode::ReadOnly)?;
            write_json(&serde_json::json!({
                "protocol": 1, "content": key, "provenance": store.original_provenance(&key)?
            }))
        }
        ("verify-original", [project, digest]) => {
            let key = content(digest)?;
            let store = ProjectStore::open(Path::new(project), AccessMode::ReadOnly)?;
            let snapshot = store.snapshot_original(&key, limits, &cancelled)?;
            write_json(
                &serde_json::json!({ "protocol": 1, "verified_original": snapshot.record(), "verification": "complete_bytes_and_identity" }),
            )
        }
        ("relink-original", [project, digest, source, "--expected-version", version]) => {
            let key = content(digest)?;
            let version = version.parse::<u64>().map_err(|_| {
                CliError::Usage("Location version must be a positive integer".into())
            })?;
            if version == 0 {
                return Err(CliError::Usage(
                    "Location version must be a positive integer".into(),
                ));
            }
            let location = LinkedOriginal::bookmarked(Path::new(source).to_owned())
                .map_err(StoreError::from)?;
            let mut store = match ProjectStore::open(Path::new(project), AccessMode::ReadWrite) {
                Ok(store) => store,
                Err(StoreError::AlreadyOpen) => {
                    return preparation::run(
                        Path::new(project),
                        PreparationCommand::Relink {
                            content: key,
                            expected_version: version,
                            location,
                        },
                    );
                }
                Err(error) => return Err(error.into()),
            };
            let record = store.relink_original(&key, version, location, limits, &cancelled)?;
            write_json(&serde_json::json!({ "protocol": 1, "relinked_original": record }))
        }
        ("relink-moved", [project]) => {
            // Closed projects only: the app relinks moved files itself when
            // it opens a project.
            let mut store = ProjectStore::open(Path::new(project), AccessMode::ReadWrite)?;
            let mut relinked = Vec::new();
            let mut refused = Vec::new();
            let mut after = None;
            loop {
                let records = store.original_records(after.as_ref(), 100)?;
                let Some(last) = records.last() else { break };
                after = Some(last.object().content().clone());
                for record in records {
                    let Some(candidate) = moved_location(&record) else {
                        continue;
                    };
                    let path = candidate.path().to_owned();
                    let outcome = store
                        .original_import_handle()?
                        .prepare_relink(&record, record.version(), candidate, limits, &cancelled)
                        .and_then(|prepared| store.relink_prepared_original(&prepared, &cancelled));
                    match outcome {
                        Ok(record) => relinked.push(record),
                        Err(error) => refused.push(serde_json::json!({
                            "content": record.object().content(),
                            "candidate": path,
                            "error": { "code": error.code(), "message": error.to_string() },
                        })),
                    }
                }
            }
            write_json(&serde_json::json!({
                "protocol": 1, "relinked_originals": relinked, "refused_candidates": refused,
            }))
        }
        _ => Err(CliError::Usage(
            "Invalid original-media command; see --help".into(),
        )),
    }
}
