//! Narrow copy-on-write file cloning for host-selected private namespaces.
//!
//! This adapter only attempts the native macOS clone operation. It does not
//! copy bytes when cloning is unavailable, publish a namespace entry, hash the
//! result, or make the clone durable. Callers own those policies.

use std::ffi::CStr;
use std::fs::File;
use std::io;

/// The result of attempting a native file clone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneOutcome {
    /// The destination was created by the native copy-on-write operation.
    Cloned,
    /// The platform or filesystem does not support the native operation.
    Unsupported,
}

/// Attempt an exclusive native copy-on-write clone into `directory/name`.
///
/// The source and destination directory must already be open descriptors. The
/// name is one non-empty path component; it is never interpreted as a path.
/// Existing destination entries, permission failures, and storage failures are
/// returned unchanged. Only `ENOTSUP`, `EXDEV`, and `ENOSYS` become
/// [`CloneOutcome::Unsupported`]. No byte-copy fallback is performed here.
pub fn try_clone_file(source: &File, directory: &File, name: &CStr) -> io::Result<CloneOutcome> {
    validate_handles(source, directory)?;
    validate_name(name)?;

    platform::try_clone_file(source, directory, name)
}

fn validate_handles(source: &File, directory: &File) -> io::Result<()> {
    let source_type = source.metadata()?.file_type();
    if !source_type.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "clone source must be a regular file",
        ));
    }
    let directory_type = directory.metadata()?.file_type();
    if !directory_type.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "clone destination must be a directory",
        ));
    }
    Ok(())
}

fn validate_name(name: &CStr) -> io::Result<()> {
    let bytes = name.to_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "clone destination name must be one non-empty path component",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::os::fd::AsRawFd;

    const CLONE_NOOWNERCOPY: libc::c_uint = 0x2;

    #[allow(unsafe_code)]
    pub(super) fn try_clone_file(
        source: &File,
        directory: &File,
        name: &CStr,
    ) -> io::Result<CloneOutcome> {
        // SAFETY: the three borrowed descriptors and the NUL-terminated name
        // remain alive for this synchronous call. fclonefileat retains no
        // pointer after returning and creates the destination exclusively.
        let result = unsafe {
            libc::fclonefileat(
                source.as_raw_fd(),
                directory.as_raw_fd(),
                name.as_ptr(),
                CLONE_NOOWNERCOPY,
            )
        };
        if result == 0 {
            return Ok(CloneOutcome::Cloned);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(code) if code == libc::ENOTSUP || code == libc::EXDEV || code == libc::ENOSYS => {
                Ok(CloneOutcome::Unsupported)
            }
            _ => Err(error),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    pub(super) fn try_clone_file(
        _source: &File,
        _directory: &File,
        _name: &CStr,
    ) -> io::Result<CloneOutcome> {
        Ok(CloneOutcome::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    #[cfg(target_os = "macos")]
    use std::fs::OpenOptions;
    use std::io::Write;
    #[cfg(target_os = "macos")]
    use std::io::{Read, Seek, SeekFrom};
    use tempfile::tempdir;

    fn name(value: &str) -> CString {
        CString::new(value).expect("test name has no NUL")
    }

    fn create_source(directory: &std::path::Path) -> io::Result<std::path::PathBuf> {
        let path = directory.join("source");
        let mut file = File::create(&path)?;
        file.write_all(&[0x11; 8192])?;
        Ok(path)
    }

    #[test]
    fn validates_names_and_open_handle_types() -> io::Result<()> {
        let scratch = tempdir()?;
        let source_path = create_source(scratch.path())?;
        let source = File::open(&source_path)?;
        let directory = File::open(scratch.path())?;

        for value in ["", ".", "..", "nested/name"] {
            let error = try_clone_file(&source, &directory, &name(value)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{value:?}");
        }

        let source_directory = File::open(scratch.path())?;
        let error = try_clone_file(&source_directory, &directory, &name("bad-source")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);

        let error = try_clone_file(&source, &source, &name("bad-directory")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        Ok(())
    }

    #[test]
    fn supported_platforms_report_their_native_capability() -> io::Result<()> {
        let scratch = tempdir()?;
        let source_path = create_source(scratch.path())?;
        let source = File::open(source_path)?;
        let directory = File::open(scratch.path())?;
        let result = try_clone_file(&source, &directory, &name("clone"))?;

        #[cfg(not(target_os = "macos"))]
        assert_eq!(result, CloneOutcome::Unsupported);

        #[cfg(target_os = "macos")]
        assert_eq!(result, CloneOutcome::Cloned);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clone_is_exclusive_and_independent_after_both_sides_change() -> io::Result<()> {
        let scratch = tempdir()?;
        let source_path = create_source(scratch.path())?;
        let source = File::open(&source_path)?;
        let directory = File::open(scratch.path())?;
        assert_eq!(
            try_clone_file(&source, &directory, &name("clone"))?,
            CloneOutcome::Cloned
        );

        let clone_path = scratch.path().join("clone");
        let mut source_writer = OpenOptions::new().write(true).open(&source_path)?;
        source_writer.seek(SeekFrom::Start(0))?;
        source_writer.write_all(&[0x22; 4096])?;
        let mut clone_writer = OpenOptions::new().write(true).open(&clone_path)?;
        clone_writer.seek(SeekFrom::Start(4096))?;
        clone_writer.write_all(&[0x33; 4096])?;

        let mut source_bytes = Vec::new();
        File::open(&source_path)?.read_to_end(&mut source_bytes)?;
        let mut clone_bytes = Vec::new();
        File::open(&clone_path)?.read_to_end(&mut clone_bytes)?;
        assert_eq!(&source_bytes[..4096], &[0x22; 4096]);
        assert_eq!(&source_bytes[4096..], &[0x11; 4096]);
        assert_eq!(&clone_bytes[..4096], &[0x11; 4096]);
        assert_eq!(&clone_bytes[4096..], &[0x33; 4096]);
        assert_ne!(source_bytes, clone_bytes);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn existing_destination_and_symlink_are_not_replaced() -> io::Result<()> {
        let scratch = tempdir()?;
        let source_path = create_source(scratch.path())?;
        let source = File::open(&source_path)?;
        let directory = File::open(scratch.path())?;

        let existing_path = scratch.path().join("existing");
        std::fs::write(&existing_path, b"preserve")?;
        let error = try_clone_file(&source, &directory, &name("existing")).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EEXIST));
        assert_eq!(std::fs::read(&existing_path)?, b"preserve");

        let link_path = scratch.path().join("link");
        std::os::unix::fs::symlink(&existing_path, &link_path)?;
        let error = try_clone_file(&source, &directory, &name("link")).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EEXIST));
        assert!(std::fs::read_link(link_path)?.ends_with("existing"));
        Ok(())
    }
}
