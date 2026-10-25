//! Launch trust for the packaged alignment worker and its one ORT library.
//! The final host build embeds pins after the worker's bundle relocation.
//! Resource manifests and worker reports are never executable authorities.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::youtube::{helpers::signing, macho_content};

const WORKER_PIN: Option<&str> = option_env!("DEADPAN_ALIGN_WORKER_CONTENT_SHA256");
const ORT_PIN: Option<&str> = option_env!("DEADPAN_ALIGN_ORT_CONTENT_SHA256");
const WORKER: &str = "MacOS/deadpan-align";
const ORT: &str = "Frameworks/libonnxruntime.1.dylib";
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

/// Independent pins embedded into the final App/CLI, absent in development.
pub fn refinement_content_pins() -> Option<(&'static str, &'static str)> {
    WORKER_PIN.zip(ORT_PIN)
}

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

fn valid_pin(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn content(path: &Path) -> io::Result<String> {
    use rustix::fs::{Mode, OFlags, open};
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let file = fs::File::from(descriptor);
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err(invalid("refinement image is not a bounded regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(invalid("refinement image changed while reading"));
    }
    macho_content::content_sha256(&bytes).map_err(invalid)
}

/// Reject symlinks in every component controlled by the package, including
/// the app directory. Canonical ancestors above the app are outside its scope.
fn regular_inside(app: &Path, path: &Path, directory: bool) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let user = rustix::process::getuid().as_raw();
    let private = |metadata: &fs::Metadata| {
        (metadata.uid() == user || metadata.uid() == 0) && metadata.mode() & 0o022 == 0
    };
    let relative = path
        .strip_prefix(app)
        .map_err(|_| invalid("refinement image escaped its application"))?;
    let mut cursor = app.to_path_buf();
    let metadata = fs::symlink_metadata(&cursor)?;
    if !metadata.file_type().is_dir() || !private(&metadata) {
        return Err(invalid("refinement application is not a real directory"));
    }
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(invalid("noncanonical refinement image path"));
        }
        cursor.push(component);
        let metadata = fs::symlink_metadata(&cursor)?;
        let kind = metadata.file_type();
        let last = cursor == path;
        if !private(&metadata)
            || (last && !directory && !kind.is_file())
            || ((!last || directory) && !kind.is_dir())
        {
            return Err(invalid(
                "refinement package contains a symlink, shared writable entry or wrong file type",
            ));
        }
    }
    Ok(())
}

fn qualify_with(
    current: &Path,
    pins: (Option<&str>, Option<&str>),
    hash: impl Fn(&Path) -> io::Result<String>,
    verify: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let contents = super::contents_of(current);
    let packaged = contents.as_deref().is_some_and(super::is_packaged);
    let expected = match pins {
        (None, None) if !packaged => {
            return current
                .parent()
                .map(|parent| parent.join("deadpan-align"))
                .ok_or_else(|| invalid("executable has no directory"));
        }
        (Some(worker), Some(ort)) if valid_pin(worker) && valid_pin(ort) => (worker, ort),
        _ => {
            return Err(invalid(
                "packaged refinement requires both compiled content pins",
            ));
        }
    };
    let contents = contents.ok_or_else(|| invalid("pin-built host is outside its application"))?;
    if !packaged {
        return Err(invalid("pin-built host lost its DeadpanPackaging marker"));
    }
    let app = contents
        .parent()
        .ok_or_else(|| invalid("application parent"))?;
    let worker = contents.join(WORKER);
    let ort = contents.join(ORT);
    let main = contents.join("MacOS/deadpan-app");
    let info = contents.join("Info.plist");
    for path in [
        current,
        main.as_path(),
        worker.as_path(),
        ort.as_path(),
        info.as_path(),
    ] {
        regular_inside(app, path, false)?;
    }
    for (path, expected) in [(&worker, expected.0), (&ort, expected.1)] {
        if hash(path)? != expected {
            return Err(invalid(format!(
                "{} differs from the host's compiled content pin",
                path.display()
            )));
        }
    }
    // Check the complete resource seal, own host and both independent images.
    // The caller supplies the expected application signer when one exists.
    for path in [
        app,
        current,
        main.as_path(),
        worker.as_path(),
        ort.as_path(),
    ] {
        verify(path)?;
    }
    Ok(worker)
}

/// Select the sibling worker in development. A pin-built packaged host verifies
/// its resource seal, executable/library content and signatures before launch.
/// Removing its marker or moving it outside the app never enables development.
pub fn qualify_refinement_worker() -> io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    // Resolve only ancestors above the executable directory. Package-owned
    // symlinks remain visible to regular_inside.
    let parent = current
        .parent()
        .ok_or_else(|| invalid("executable directory"))?;
    let current = parent
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .filter(|_| super::contents_of(&current).is_some())
        .map(|ancestor| {
            let canonical = fs::canonicalize(ancestor)?;
            let relative = current.strip_prefix(ancestor).map_err(invalid)?;
            Ok::<_, io::Error>(canonical.join(relative))
        })
        .transpose()?
        .unwrap_or(current);
    qualify_with(&current, (WORKER_PIN, ORT_PIN), content, |path| {
        let requirement = signing::application_requirement().map_err(invalid)?;
        signing::verify(path, requirement.as_deref()).map_err(invalid)
    })
}

#[cfg(test)]
mod tests;
