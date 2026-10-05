//! Optional startup keymap bytes. Parsing and binding policy belong to the caller.
//! This reader never creates files or directories and never falls back to cwd.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_BYTES: usize = 256 * 1024;
const TOO_LARGE: &str = "exceeds the 262144-byte (256 KiB) keymap limit";

#[derive(Debug)]
pub struct KeymapFile {
    /// The selected path, including read failures; absent only if macOS could
    /// not resolve its user Application Support directory.
    pub path: Option<PathBuf>,
    pub contents: Result<Option<Vec<u8>>, String>,
}

pub fn load() -> KeymapFile {
    match application_support_directory() {
        Ok(directory) => read_from(directory.join("Deadpan/keymap.json")),
        Err(error) => KeymapFile {
            path: None,
            contents: Err(error),
        },
    }
}

/// Explicit path injection keeps tests and replay away from the user's keymap.
/// Missing files (including missing parents or symlink targets) are optional;
/// other open, type, size and read failures are returned with the selected path.
pub fn read_from(path: PathBuf) -> KeymapFile {
    let contents = read_contents(&path);
    KeymapFile {
        path: Some(path),
        contents,
    }
}

fn read_contents(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !path.is_absolute() {
        return Err(format!(
            "The keymap path must be absolute: {}",
            path.display()
        ));
    }
    // Open first, without waiting for a FIFO peer. Check the resulting descriptor,
    // so a path replacement cannot substitute a special file after a path check.
    // Ordinary symlinks are intentionally followed; no file is created or changed.
    let descriptor = match rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(format!("Cannot open keymap {}: {error}", path.display())),
    };
    let file = File::from(descriptor);
    let metadata = file
        .metadata()
        .map_err(|error| format!("Cannot inspect keymap {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("Keymap {} must be a regular file", path.display()));
    }
    if metadata.len() > MAX_BYTES as u64 {
        return Err(format!("Keymap {} {TOO_LARGE}", path.display()));
    }
    // Metadata is only an early refusal. The descriptor may grow afterward;
    // the byte limit is enforced again on the actual read, with one excess byte.
    read_bounded(file)
        .map(Some)
        .map_err(|error| format!("Cannot read keymap {}: {error}", path.display()))
}

fn read_bounded(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut contents = Vec::new();
    reader
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut contents)
        .map_err(|error| error.to_string())?;
    if contents.len() > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    Ok(contents)
}

#[cfg(target_os = "macos")]
pub(crate) fn application_support_directory() -> Result<PathBuf, String> {
    use objc2_foundation::{NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask};

    objc2::rc::autoreleasepool(|_| {
        let locations = NSFileManager::defaultManager().URLsForDirectory_inDomains(
            NSSearchPathDirectory::ApplicationSupportDirectory,
            NSSearchPathDomainMask::UserDomainMask,
        );
        let location = locations
            .firstObject()
            .and_then(|url| url.path())
            .ok_or("macOS did not provide a user Application Support directory for the keymap")?;
        let path = PathBuf::from(location.to_string());
        if !path.is_absolute() {
            return Err(
                "macOS returned a relative Application Support directory for the keymap".into(),
            );
        }
        Ok(path)
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn application_support_directory() -> Result<PathBuf, String> {
    Err("The startup user keymap location is supported only on macOS".into())
}

#[cfg(test)]
mod tests;
