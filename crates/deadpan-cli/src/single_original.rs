//! Headless creation of a one-Original project from a complete local file.
//!
//! This is the closed-project equivalent of native New: create an Awaiting
//! Source package, retain the complete original as managed bytes or an explicit
//! linked location, qualify its
//! picture and first audio track from a verified snapshot, then atomically
//! establish the full-source baseline through `initialize_prepared_source`.
//! Publish only after qualification and baseline creation succeed. A failure
//! removes the private staging package and leaves the requested path unused.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
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
    create_at_free_name(package, &|_| None, source, label, cancelled, retained)
        .map(|(_, created)| created)
}

/// Like [`create`], but a name taken before the build or at the final
/// no-replace rename moves to the candidate `next` returns for the taken
/// path, so a finished build is never discarded for a name collision.
/// Candidates must be `.deadpan` siblings of `package`. Returns the path that
/// holds the Ready project.
pub fn create_at_free_name(
    package: &Path,
    next: &dyn Fn(&Path) -> Option<PathBuf>,
    source: &Path,
    label: &str,
    cancelled: &AtomicBool,
    retained: impl FnOnce(&ProjectStore, &OriginalMediaRecord) -> Result<(), CliError>,
) -> Result<(PathBuf, CreatedOriginal), CliError> {
    create_owned_at_free_name(
        package,
        next,
        source,
        label,
        OriginalOwnership::Managed,
        cancelled,
        retained,
    )
}

fn create_owned_at_free_name(
    package: &Path,
    next: &dyn Fn(&Path) -> Option<PathBuf>,
    source: &Path,
    label: &str,
    ownership: OriginalOwnership,
    cancelled: &AtomicBool,
    retained: impl FnOnce(&ProjectStore, &OriginalMediaRecord) -> Result<(), CliError>,
) -> Result<(PathBuf, CreatedOriginal), CliError> {
    /// Bounds a pathological stream of racing creators.
    const MAX_CANDIDATES: usize = 1000;
    if !source.is_absolute() {
        return Err(CliError::Usage("The Original path must be absolute".into()));
    }
    let parent = match package.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_owned(),
        _ => PathBuf::from("."),
    };
    let admissible = |candidate: &Path| {
        candidate
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("deadpan")
            && candidate.parent().map_or(Path::new("."), |parent| {
                if parent.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    parent
                }
            }) == parent
    };
    if !admissible(package) {
        return Err(CliError::Usage(
            "The project path must end in .deadpan".into(),
        ));
    }
    let taken = |path: &Path| CliError::Usage(format!("{} already exists", path.display()));
    let mut target = package.to_owned();
    let mut candidates = 0;
    // A candidate that is not admissible is never used.
    let mut advance = |current: &Path| -> Option<PathBuf> {
        candidates += 1;
        (candidates <= MAX_CANDIDATES)
            .then(|| next(current))
            .flatten()
            .filter(|candidate| admissible(candidate))
    };
    while fs::symlink_metadata(&target).is_ok() {
        target = advance(&target).ok_or_else(|| taken(&target))?;
    }
    let staging = parent.join(format!(".{}.creating.deadpan", uuid::Uuid::new_v4()));
    let built = build(&staging, source, label, ownership, cancelled, retained);
    let published = built.and_then(|created| {
        loop {
            match rustix::fs::renameat_with(
                rustix::fs::CWD,
                &staging,
                rustix::fs::CWD,
                &target,
                rustix::fs::RenameFlags::NOREPLACE,
            ) {
                Ok(()) => break,
                Err(rustix::io::Errno::EXIST) => {
                    target = advance(&target).ok_or_else(|| taken(&target))?;
                }
                Err(error) => return Err(CliError::Io(error.into())),
            }
        }
        File::open(&parent)?.sync_all()?;
        Ok(created)
    });
    if published.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    published.map(|created| (target, created))
}

fn build(
    package: &Path,
    source: &Path,
    label: &str,
    ownership: OriginalOwnership,
    cancelled: &AtomicBool,
    retained: impl FnOnce(&ProjectStore, &OriginalMediaRecord) -> Result<(), CliError>,
) -> Result<CreatedOriginal, CliError> {
    let document = ProjectDocument::new_automatic(
        identity(ProjectId::new)?,
        identity(RevisionId::new)?,
        identity(NodeId::new)?,
    )?;
    let mut store = ProjectStore::create_single_source(package, &document)?;
    let retention =
        store.retain_original(source, ownership, preparation::original_limits(), cancelled)?;
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

/// `project create-original <project.deadpan> <absolute-source> [--linked]`
pub(crate) fn run(package: &Path, source: &Path, linked: bool) -> Result<(), CliError> {
    let label = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Original");
    let ownership = if linked {
        OriginalOwnership::linked_at(source)
    } else {
        OriginalOwnership::Managed
    };
    let (_, created) = create_owned_at_free_name(
        package,
        &|_| None,
        source,
        label,
        ownership,
        &AtomicBool::new(false),
        |_, _| Ok(()),
    )?;
    write_json(&serde_json::json!({ "protocol": 1, "created": created }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_creation_keeps_the_external_original_and_the_managed_default() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
            .canonicalize()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        for linked in [true, false] {
            let package = directory.path().join(format!("{linked}.deadpan"));
            run(&package, &source, linked).unwrap();
            let store = ProjectStore::open(&package, deadpan_store::AccessMode::ReadOnly).unwrap();
            let records = store.original_records(None, 2).unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].managed(), !linked);
            assert_eq!(
                records[0].linked().map(|link| link.path()),
                linked.then_some(source.as_path())
            );
            assert_eq!(store.snapshot().unwrap().duration().unwrap().frames(), 120);
            assert!(matches!(
                store.single_source_state().unwrap(),
                Some(SingleSourceState::Ready { .. })
            ));
            store.validate_full().unwrap();
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
        assert!(source.is_file());
    }

    #[test]
    fn a_name_taken_during_the_build_moves_to_the_next_candidate() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
            .canonicalize()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("Clip.deadpan");
        let second = directory.path().join("Clip 2.deadpan");
        let third = directory.path().join("Clip 3.deadpan");
        // "Clip 2" is taken before the build; "Clip" is taken while it runs.
        fs::create_dir(&second).unwrap();
        let next = |taken: &Path| {
            [&first, &second, &third]
                .into_iter()
                .skip_while(|candidate| candidate.as_path() != taken)
                .nth(1)
                .cloned()
        };
        let (published, created) = create_at_free_name(
            &first,
            &next,
            &source,
            "Clip",
            &AtomicBool::new(false),
            |_, _| Ok(fs::create_dir(&first)?),
        )
        .unwrap();
        assert_eq!(published, third);
        assert!(matches!(
            created.single_source,
            SingleSourceState::Ready { .. }
        ));
        assert_eq!(fs::read_dir(&first).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&second).unwrap().count(), 0);
        // No staging package is left behind.
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);

        // Without candidates the collision is refused and nothing remains.
        let taken = directory.path().join("Taken.deadpan");
        let error = create(&taken, &source, "Taken", &AtomicBool::new(false), |_, _| {
            Ok(fs::create_dir(&taken)?)
        })
        .unwrap_err();
        assert!(matches!(error, CliError::Usage(message) if message.ends_with("already exists")));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 4);
    }

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
