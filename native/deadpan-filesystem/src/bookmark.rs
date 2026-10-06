//! macOS file bookmarks: an opaque record of where a file is, which the
//! system can resolve after the file was moved or renamed on its volume.
//!
//! Bookmarks are created with default options: the system records the file's
//! identity as well as its path, so a file moved or renamed on its volume is
//! still found (the old `PreferFileIDResolution` option is unsupported on
//! current macOS and deprecated in the bindings). They are regular, not
//! security-scoped: Deadpan is not sandboxed. Resolution never shows UI and
//! never mounts volumes. A resolved path is a candidate only; nothing here
//! reads, opens or trusts the file's content.
//!
//! The only unsafe code is the documented resolution call, whose `is_stale`
//! output pointer refers to a live local.

use std::io;
use std::path::{Path, PathBuf};

/// What a bookmark resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub path: PathBuf,
    /// The system suggests recreating the bookmark (for example after a move).
    pub stale: bool,
}

/// Record where `path` is. The file must exist.
pub fn create(path: &Path) -> io::Result<Vec<u8>> {
    platform::create(path)
}

/// Where a bookmark's file is now, if the system can find it.
pub fn resolve(bookmark: &[u8]) -> io::Result<Resolved> {
    if bookmark.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an empty bookmark names no file",
        ));
    }
    platform::resolve(bookmark)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::Bool;
    use objc2_foundation::{
        NSData, NSError, NSString, NSURL, NSURLBookmarkCreationOptions,
        NSURLBookmarkResolutionOptions,
    };

    fn failure(kind: io::ErrorKind, error: &NSError) -> io::Error {
        io::Error::new(kind, error.localizedDescription().to_string())
    }

    pub(super) fn create(path: &Path) -> io::Result<Vec<u8>> {
        let text = path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "bookmarks need a UTF-8 path")
        })?;
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "bookmarks need an absolute path",
            ));
        }
        autoreleasepool(|_| {
            let url = NSURL::fileURLWithPath(&NSString::from_str(text));
            url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                NSURLBookmarkCreationOptions::empty(),
                None,
                None,
            )
            .map(|data| data.to_vec())
            .map_err(|error| failure(io::ErrorKind::NotFound, &error))
        })
    }

    #[allow(unsafe_code)]
    pub(super) fn resolve(bookmark: &[u8]) -> io::Result<Resolved> {
        autoreleasepool(|_| {
            let data = NSData::with_bytes(bookmark);
            let mut stale = Bool::NO;
            // SAFETY: `is_stale` points at a live, writable local `Bool` for
            // the duration of this synchronous call, as the method requires;
            // the system does not retain it. Every other argument is a valid
            // retained object or `None`.
            let url: Result<Retained<NSURL>, Retained<NSError>> = unsafe {
                NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                    &data,
                    NSURLBookmarkResolutionOptions::WithoutUI
                        | NSURLBookmarkResolutionOptions::WithoutMounting,
                    None,
                    &mut stale,
                )
            };
            let url = url.map_err(|error| failure(io::ErrorKind::NotFound, &error))?;
            let path = url.path().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "the bookmark names no file path")
            })?;
            Ok(Resolved {
                path: PathBuf::from(path.to_string()),
                stale: stale.as_bool(),
            })
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    pub(super) fn create(_path: &Path) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "file bookmarks require macOS",
        ))
    }

    pub(super) fn resolve(_bookmark: &[u8]) -> io::Result<Resolved> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "file bookmarks require macOS",
        ))
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn a_bookmark_follows_a_renamed_and_moved_file() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().canonicalize().unwrap();
        let original = root.join("clip.mov");
        std::fs::write(&original, b"bytes").unwrap();
        let bookmark = create(&original).unwrap();
        assert!(!bookmark.is_empty());
        assert_eq!(resolve(&bookmark).unwrap().path, original);
        let elsewhere = root.join("moved");
        std::fs::create_dir(&elsewhere).unwrap();
        let moved = elsewhere.join("renamed.mov");
        std::fs::rename(&original, &moved).unwrap();
        let resolved = resolve(&bookmark).unwrap();
        assert_eq!(resolved.path, moved);
    }

    #[test]
    fn missing_files_and_garbage_fail_explicitly() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().canonicalize().unwrap().join("gone.wav");
        assert!(create(&file).is_err());
        assert!(create(Path::new("relative.wav")).is_err());
        std::fs::write(&file, b"x").unwrap();
        let bookmark = create(&file).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(resolve(&bookmark).is_err());
        assert!(resolve(&[]).is_err());
        assert!(resolve(b"not a bookmark").is_err());
    }
}
