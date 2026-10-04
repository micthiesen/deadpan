//! Headless creation of a one-Original project from a complete local file.
//!
//! This is the closed-project equivalent of native New: create an Awaiting
//! Source package, retain the complete original as managed bytes, qualify its
//! picture and first audio track from a verified snapshot, then atomically
//! establish the full-source baseline through `initialize_prepared_source`.
//! A failure after package creation leaves the explicit, recoverable Awaiting
//! Source state and any retained bytes; it never claims a ready Original.

use std::fs::{self, File};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_core::{AssetId, NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_store::original_media::{OriginalMediaRecord, OriginalOwnership};
use deadpan_store::single_source::{SingleSourceInitialization, SingleSourceState};
use deadpan_store::{ProjectStore, StoreError};
use serde::Serialize;

use crate::live_project::preparation;
use crate::{CliError, write_json};

/// Longest Original label, in characters, derived from untrusted names.
pub const MAX_LABEL_CHARS: usize = 120;

#[derive(Debug, Serialize)]
pub struct CreatedOriginal {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub asset_id: AssetId,
    pub node: NodeId,
    pub content: String,
    pub byte_length: u64,
    pub duration_frames: i64,
    pub presentation_basis: deadpan_core::PresentationBasis,
    pub single_source: SingleSourceState,
}

/// Bounded single-line display label: control characters become spaces,
/// whitespace collapses and the result never is empty.
pub fn display_label(untrusted: &str) -> String {
    let mut label = String::new();
    for word in untrusted
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
    {
        if !label.is_empty() {
            label.push(' ');
        }
        label.push_str(word);
    }
    let label: String = label.chars().take(MAX_LABEL_CHARS).collect();
    if label.trim().is_empty() {
        "Original".into()
    } else {
        label.trim_end().to_owned()
    }
}

fn identity<T>(
    make: impl FnOnce(String) -> Result<T, deadpan_core::DocumentError>,
) -> Result<T, CliError> {
    Ok(make(uuid::Uuid::new_v4().to_string())?)
}

/// Create `package` and establish `source` as its complete Original.
/// `retained` runs once the original bytes are durably retained and before
/// qualification, for operational metadata keyed by the retained content.
///
/// The package is built at a hidden sibling path and moved into place only
/// once it is Ready, with a rename that never replaces an existing entry. Any
/// failure removes the partial package, so the requested path either holds a
/// complete project or nothing and a retry can reuse it.
pub fn create(
    package: &Path,
    source: &Path,
    label: &str,
    cancelled: &AtomicBool,
    retained: impl FnOnce(&ProjectStore, &OriginalMediaRecord) -> Result<(), CliError>,
) -> Result<CreatedOriginal, CliError> {
    if !source.is_absolute() {
        return Err(CliError::Usage("The Original path must be absolute".into()));
    }
    if package.extension().and_then(|extension| extension.to_str()) != Some("deadpan") {
        return Err(CliError::Usage(
            "The project path must end in .deadpan".into(),
        ));
    }
    if fs::symlink_metadata(package).is_ok() {
        return Err(CliError::Usage(format!(
            "{} already exists",
            package.display()
        )));
    }
    let parent = match package.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let staging = parent.join(format!(".{}.creating.deadpan", uuid::Uuid::new_v4()));
    let built = build(&staging, source, label, cancelled, retained);
    let published = built.and_then(|created| {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            &staging,
            rustix::fs::CWD,
            package,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|error| CliError::Io(error.into()))?;
        File::open(parent)?.sync_all()?;
        Ok(created)
    });
    if published.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    published
}

fn build(
    package: &Path,
    source: &Path,
    label: &str,
    cancelled: &AtomicBool,
    retained: impl FnOnce(&ProjectStore, &OriginalMediaRecord) -> Result<(), CliError>,
) -> Result<CreatedOriginal, CliError> {
    let document = ProjectDocument::new_automatic(
        identity(ProjectId::new)?,
        identity(RevisionId::new)?,
        identity(NodeId::new)?,
    )?;
    let mut store = ProjectStore::create_single_source(package, &document)?;
    let retention = store.retain_original(
        source,
        OriginalOwnership::Managed,
        preparation::original_limits(),
        cancelled,
    )?;
    retained(&store, &retention.record)?;
    let asset = identity(AssetId::new)?;
    let handle = store.original_import_handle()?;
    let prepared =
        preparation::qualify_primary(&handle, &retention.record, asset.clone(), cancelled)?;
    let initialization = SingleSourceInitialization {
        expected_revision: document.revision_id().clone(),
        new_revision: identity(RevisionId::new)?,
        new_asset_id: asset,
        node: identity(NodeId::new)?,
        label: display_label(label),
    };
    let outcome = store.initialize_prepared_source(&initialization, &prepared, cancelled)?;
    let snapshot = store.snapshot()?;
    let single_source = store.single_source_state()?.ok_or_else(|| {
        StoreError::SingleSource("initialized project lost its single-Original profile".into())
    })?;
    if !matches!(single_source, SingleSourceState::Ready { .. }) {
        return Err(StoreError::SingleSource("initialization did not reach Ready".into()).into());
    }
    Ok(CreatedOriginal {
        project_id: snapshot.project_id().clone(),
        revision_id: snapshot.revision_id().clone(),
        asset_id: outcome.asset_id,
        node: initialization.node,
        content: retention.record.object().content().to_string(),
        byte_length: retention.record.object().byte_length(),
        duration_frames: snapshot.duration()?.frames(),
        presentation_basis: snapshot.presentation_basis().clone(),
        single_source,
    })
}

/// `project create-original <project.deadpan> <absolute-source>`
pub(crate) fn run(package: &Path, source: &Path) -> Result<(), CliError> {
    let label = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Original");
    let created = create(package, source, label, &AtomicBool::new(false), |_, _| {
        Ok(())
    })?;
    write_json(&serde_json::json!({ "protocol": 1, "created": created }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_bounded_single_line_text() {
        assert_eq!(display_label("  A\n\tclip\u{7}name  "), "A clip name");
        assert_eq!(display_label("\n\r"), "Original");
        assert_eq!(
            display_label(&"é".repeat(500)).chars().count(),
            MAX_LABEL_CHARS
        );
    }
}
