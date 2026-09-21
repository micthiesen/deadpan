use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_store::original_media::{
    LinkedOriginal, OriginalContentId, OriginalMediaError, OriginalMediaLimits, OriginalOwnership,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

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
                OriginalOwnership::Linked { bookmark: None }
            } else {
                OriginalOwnership::Managed
            };
            let mut store = ProjectStore::open(Path::new(project), AccessMode::ReadWrite)?;
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
            let location = LinkedOriginal::new(Path::new(source).to_owned(), None)
                .map_err(StoreError::from)?;
            let mut store = ProjectStore::open(Path::new(project), AccessMode::ReadWrite)?;
            let record = store.relink_original(&key, version, location, limits, &cancelled)?;
            write_json(&serde_json::json!({ "protocol": 1, "relinked_original": record }))
        }
        _ => Err(CliError::Usage(
            "Invalid original-media command; see --help".into(),
        )),
    }
}
