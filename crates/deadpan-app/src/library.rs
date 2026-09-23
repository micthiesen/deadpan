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
}

fn project_stem(source: &Path) -> String {
    let original = source.file_stem().unwrap_or_default().to_string_lossy();
    let mut result = String::new();
    for character in original.chars() {
        let character = if character.is_control() || matches!(character, '/' | '\\' | ':') {
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
    if result.is_empty() {
        "Untitled".into()
    } else {
        result.into()
    }
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
