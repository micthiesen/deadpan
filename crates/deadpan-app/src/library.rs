//! Native project placement. Source locations and the process directory never
//! choose where a newly created project is stored.

use std::path::{Path, PathBuf};

use deadpan_core::ProjectDocument;
use deadpan_store::{ProjectStore, StoreError};

#[derive(Clone, Debug)]
pub struct ProjectLibrary {
    root: PathBuf,
}

impl ProjectLibrary {
    pub fn documents() -> Result<Self, String> {
        Self::from_documents(documents_directory()?)
    }

    /// Explicit injection keeps lifecycle tests independent of the user's files.
    pub fn from_documents(documents: PathBuf) -> Result<Self, String> {
        if !documents.is_absolute() {
            return Err("The Documents directory must be an absolute path".into());
        }
        Ok(Self {
            root: documents.join("Deadpan"),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The store atomically creates each package. Retry only a collision at that
    /// exclusive creation boundary, never an error inside an allocated package.
    pub fn create(
        &self,
        source: &Path,
        document: &ProjectDocument,
    ) -> Result<(PathBuf, ProjectStore), String> {
        std::fs::create_dir_all(&self.root).map_err(|error| {
            format!(
                "Cannot create project library {}: {error}",
                self.root.display()
            )
        })?;
        let stem = project_stem(source);
        for ordinal in 1..=10_000_u32 {
            let name = if ordinal == 1 {
                format!("{stem}.deadpan")
            } else {
                format!("{stem} {ordinal}.deadpan")
            };
            let path = self.root.join(name);
            match ProjectStore::create_single_source(&path, document) {
                Ok(store) => return Ok((path, store)),
                Err(StoreError::PackageAlreadyExists(_)) => continue,
                Err(error) => {
                    return Err(format!("Cannot create project {}: {error}", path.display()));
                }
            }
        }
        Err(format!("No unused project name is available for {stem}"))
    }

    /// The first unused `<name>.deadpan` (or `<name> N.deadpan`) path in the
    /// library for untrusted display text such as a remote title. The text is
    /// reduced to one bounded file-name component; it never forms a path. A
    /// creator must still refuse an existing package at its own exclusive
    /// creation boundary, since this check cannot reserve the name.
    pub fn unused_package(&self, name: &str, fallback: &str) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.root).map_err(|error| {
            format!(
                "Cannot create project library {}: {error}",
                self.root.display()
            )
        })?;
        let stem = match file_stem(name) {
            Some(stem) => stem,
            None => file_stem(fallback).unwrap_or_else(|| "Untitled".into()),
        };
        for ordinal in 1..=10_000_u32 {
            let name = if ordinal == 1 {
                format!("{stem}.deadpan")
            } else {
                format!("{stem} {ordinal}.deadpan")
            };
            let path = self.root.join(name);
            match std::fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(path),
                Err(error) => {
                    return Err(format!("Cannot inspect {}: {error}", path.display()));
                }
                Ok(_) => continue,
            }
        }
        Err(format!("No unused project name is available for {stem}"))
    }
}

/// The first free `<stem> N.deadpan` sibling of `first` (N from 2), for a
/// creator whose chosen name was taken. The creator's no-replace rename still
/// decides; calling again after another collision finds the next one.
pub fn next_free_package(first: &Path) -> Option<PathBuf> {
    let stem = first.file_stem()?.to_str()?;
    let parent = first.parent()?;
    (2..=10_000_u32)
        .map(|ordinal| parent.join(format!("{stem} {ordinal}.deadpan")))
        .find(|candidate| std::fs::symlink_metadata(candidate).is_err())
}

fn project_stem(source: &Path) -> String {
    file_stem(&source.file_stem().unwrap_or_default().to_string_lossy())
        .unwrap_or_else(|| "Untitled".into())
}

/// One bounded file-name component, or None when nothing usable remains.
fn file_stem(original: &str) -> Option<String> {
    let mut result = String::new();
    for character in original.chars() {
        let character = if character.is_control() || matches!(character, '/' | '\\' | ':') {
            ' '
        } else {
            character
        };
        // A separator next to a space (`Title: Subtitle`) leaves one space.
        if character.is_whitespace() && result.ends_with(' ') {
            continue;
        }
        let character = if character.is_whitespace() {
            ' '
        } else {
            character
        };
        if result.len() + character.len_utf8() > 160 {
            break;
        }
        result.push(character);
    }
    let result =
        result.trim_matches(|character: char| character.is_whitespace() || character == '.');
    (!result.is_empty()).then(|| result.into())
}

#[cfg(target_os = "macos")]
fn documents_directory() -> Result<PathBuf, String> {
    use objc2_foundation::{NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask};

    objc2::rc::autoreleasepool(|_| {
        let locations = NSFileManager::defaultManager().URLsForDirectory_inDomains(
            NSSearchPathDirectory::DocumentDirectory,
            NSSearchPathDomainMask::UserDomainMask,
        );
        let location = locations
            .firstObject()
            .and_then(|url| url.path())
            .ok_or("macOS did not provide a Documents directory")?;
        let path = PathBuf::from(location.to_string());
        if !path.is_absolute() {
            return Err("macOS returned a relative Documents directory".into());
        }
        Ok(path)
    })
}

#[cfg(not(target_os = "macos"))]
fn documents_directory() -> Result<PathBuf, String> {
    Err("The native project library is supported only on macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{NodeId, ProjectId, RevisionId};

    fn document() -> ProjectDocument {
        ProjectDocument::new_automatic(
            ProjectId::new("library-test").unwrap(),
            RevisionId::new("initial").unwrap(),
            NodeId::new("root").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn library_uses_injected_documents_and_exclusive_collision_names() {
        let scratch = tempfile::tempdir().unwrap();
        let library = ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap();
        let (first, store) = library
            .create(Path::new("/outside/interview.mp4"), &document())
            .unwrap();
        assert_eq!(
            first,
            scratch.path().join("Documents/Deadpan/interview.deadpan")
        );
        drop(store);
        let (second, _) = library
            .create(Path::new("interview.mov"), &document())
            .unwrap();
        assert_eq!(
            second,
            scratch.path().join("Documents/Deadpan/interview 2.deadpan")
        );
        assert!(ProjectLibrary::from_documents(PathBuf::from("Documents")).is_err());
    }

    #[test]
    fn file_names_are_bounded_without_changing_the_source_label() {
        assert_eq!(project_stem(Path::new("/clips/../...mp4")), "Untitled");
        assert_eq!(project_stem(Path::new("/clips/ a:b\\c\n.mp4")), "a b c");
        let long = format!("{}.mp4", "試".repeat(100));
        let stem = project_stem(Path::new(&long));
        assert!(stem.len() <= 160);
        assert!(!stem.is_empty());
        assert_eq!(
            project_stem(Path::new("/clips/My interview.mp4")),
            "My interview"
        );
    }

    #[test]
    fn remote_titles_become_one_unused_library_name() {
        let scratch = tempfile::tempdir().unwrap();
        let library = ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap();
        let root = scratch.path().join("Documents/Deadpan");
        assert_eq!(
            library
                .unused_package("Caminandes 2: Gran Dillama", "YouTube Z4C82eyhwgU")
                .unwrap(),
            root.join("Caminandes 2 Gran Dillama.deadpan")
        );
        std::fs::create_dir(root.join("Caminandes 2 Gran Dillama.deadpan")).unwrap();
        assert_eq!(
            library
                .unused_package("Caminandes 2: Gran Dillama", "YouTube Z4C82eyhwgU")
                .unwrap(),
            root.join("Caminandes 2 Gran Dillama 2.deadpan")
        );
        for hostile in ["../../escape", "..", "/", " . "] {
            let path = library
                .unused_package(hostile, "YouTube Z4C82eyhwgU")
                .unwrap();
            assert_eq!(path.parent(), Some(root.as_path()), "{hostile}");
        }
        assert_eq!(
            library
                .unused_package("...", "YouTube Z4C82eyhwgU")
                .unwrap(),
            root.join("YouTube Z4C82eyhwgU.deadpan")
        );
    }

    #[test]
    fn taken_names_move_to_the_next_free_sibling() {
        let scratch = tempfile::tempdir().unwrap();
        let first = scratch.path().join("Clip.deadpan");
        assert_eq!(
            next_free_package(&first),
            Some(scratch.path().join("Clip 2.deadpan"))
        );
        std::fs::create_dir(scratch.path().join("Clip 2.deadpan")).unwrap();
        assert_eq!(
            next_free_package(&first),
            Some(scratch.path().join("Clip 3.deadpan"))
        );
    }

    #[test]
    fn a_library_failure_does_not_create_a_project_elsewhere() {
        let scratch = tempfile::tempdir().unwrap();
        let documents = scratch.path().join("Documents");
        std::fs::write(&documents, b"a file cannot be a directory").unwrap();
        let library = ProjectLibrary::from_documents(documents).unwrap();
        let error = library
            .create(Path::new("interview.mp4"), &document())
            .err()
            .unwrap();
        assert!(error.contains("project library"));
        assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 1);
    }
}
