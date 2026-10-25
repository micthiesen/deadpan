//! Shared immutable image admission for bundled native runtimes.

use std::cell::RefCell;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::youtube::macho_content;

mod signer;

const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ADMISSION_DURATION: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
pub(super) struct Control<'a> {
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl<'a> Control<'a> {
    pub(super) fn new(cancelled: &'a AtomicBool, deadline: Instant) -> Self {
        Self {
            cancelled,
            deadline: deadline.min(Instant::now() + MAX_ADMISSION_DURATION),
        }
    }

    pub(super) fn check(self) -> io::Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "native runtime qualification cancelled",
            ))
        } else if Instant::now() >= self.deadline {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "native runtime qualification deadline elapsed",
            ))
        } else {
            Ok(())
        }
    }
}

/// A partial shipping build cannot re-enable development by removing a marker
/// or entering a subsystem whose own pin variable was accidentally omitted.
pub(super) const fn any_compiled_runtime_pins() -> bool {
    any_runtime_pins([
        option_env!("DEADPAN_NATIVE_CONTENT_PINS"),
        option_env!("DEADPAN_ALIGN_WORKER_CONTENT_SHA256"),
        option_env!("DEADPAN_ALIGN_ORT_CONTENT_SHA256"),
    ])
}

const fn any_runtime_pins(pins: [Option<&str>; 3]) -> bool {
    pins[0].is_some() || pins[1].is_some() || pins[2].is_some()
}

pub(super) fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

pub(super) fn valid_pin(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
pub(super) fn content(path: &Path) -> io::Result<String> {
    let cancelled = AtomicBool::new(false);
    content_interruptible(
        path,
        Control::new(&cancelled, Instant::now() + MAX_ADMISSION_DURATION),
    )
}

fn content_interruptible(path: &Path, control: Control<'_>) -> io::Result<String> {
    use rustix::fs::{Mode, OFlags, open};
    control.check()?;
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut file = fs::File::from(descriptor);
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err(invalid("native image is not a bounded regular file"));
    }
    let mut bytes = Vec::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        control.check()?;
        let count = file.read(&mut chunk)?;
        control.check()?;
        if count == 0 {
            break;
        }
        if (bytes.len() + count) as u64 > MAX_IMAGE_BYTES {
            return Err(invalid("native image grew beyond its bound"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    if bytes.len() as u64 != metadata.len() {
        return Err(invalid("native image changed while reading"));
    }
    let result = macho_content::content_sha256_interruptible(&bytes, || control.check().is_ok());
    control.check()?;
    result.map_err(invalid)
}

/// Reject aliases and shared-write entries from the app directory downward.
pub(super) fn regular_inside(app: &Path, path: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let user = rustix::process::getuid().as_raw();
    let private = |metadata: &fs::Metadata| {
        (metadata.uid() == user || metadata.uid() == 0) && metadata.mode() & 0o022 == 0
    };
    let relative = path
        .strip_prefix(app)
        .map_err(|_| invalid("native image escaped its application"))?;
    let mut cursor = app.to_path_buf();
    let metadata = fs::symlink_metadata(&cursor)?;
    if !metadata.file_type().is_dir() || !private(&metadata) {
        return Err(invalid(
            "native application is not a real private directory",
        ));
    }
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(invalid("noncanonical native image path"));
        }
        cursor.push(component);
        let metadata = fs::symlink_metadata(&cursor)?;
        let kind = metadata.file_type();
        if !private(&metadata)
            || (cursor == path && !kind.is_file())
            || (cursor != path && !kind.is_dir())
        {
            return Err(invalid(
                "native package contains a symlink, shared writable entry or wrong file type",
            ));
        }
    }
    Ok(())
}

/// Resolve ancestors outside the app while preserving package-owned symlinks
/// for the explicit checks above. Both requested and host paths use this rule.
pub(super) fn normalize_executable(path: PathBuf) -> io::Result<PathBuf> {
    if let Some(contents) = super::contents_of(&path) {
        let ancestor = contents
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| invalid("application ancestor"))?;
        let canonical = fs::canonicalize(ancestor)?;
        let relative = path.strip_prefix(ancestor).map_err(invalid)?;
        Ok(canonical.join(relative))
    } else {
        Ok(path)
    }
}

pub(super) fn current_executable() -> io::Result<PathBuf> {
    normalize_executable(std::env::current_exe()?)
}

/// Every image is independently content-bound before any signature checks.
/// The package seal and both host executables must remain valid as well.
#[cfg(test)]
pub(super) fn qualify_with(
    current: &Path,
    images: &[(&str, &str)],
    hash: impl Fn(&Path) -> io::Result<String>,
    verify: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let cancelled = AtomicBool::new(false);
    qualify_with_control(
        current,
        images,
        Control::new(&cancelled, Instant::now() + MAX_ADMISSION_DURATION),
        hash,
        verify,
    )
}

fn qualify_with_control(
    current: &Path,
    images: &[(&str, &str)],
    control: Control<'_>,
    hash: impl Fn(&Path) -> io::Result<String>,
    verify: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    control.check()?;
    let contents = super::contents_of(current)
        .ok_or_else(|| invalid("pin-built host is outside its application"))?;
    if !super::is_packaged(&contents) {
        return Err(invalid("pin-built host lost its DeadpanPackaging marker"));
    }
    let app = contents
        .parent()
        .ok_or_else(|| invalid("application parent"))?;
    let main = contents.join("MacOS/deadpan-app");
    for path in [current, main.as_path(), &contents.join("Info.plist")] {
        control.check()?;
        regular_inside(app, path)?;
    }
    for (relative, expected) in images {
        if !valid_pin(expected) {
            return Err(invalid("invalid compiled native image pin"));
        }
        let path = contents.join(relative);
        control.check()?;
        regular_inside(app, &path)?;
        let actual = hash(&path)?;
        control.check()?;
        if actual != *expected {
            return Err(invalid(format!(
                "{} differs from the host's compiled content pin",
                path.display()
            )));
        }
    }
    for path in [app, current, main.as_path()] {
        control.check()?;
        verify(path)?;
    }
    for (relative, _) in images {
        control.check()?;
        verify(&contents.join(relative))?;
    }
    control.check()
}

pub(super) fn qualify(current: &Path, images: &[(&str, &str)]) -> io::Result<()> {
    let cancelled = AtomicBool::new(false);
    qualify_interruptible(
        current,
        images,
        Control::new(&cancelled, Instant::now() + MAX_ADMISSION_DURATION),
    )
}

pub(super) fn qualify_interruptible(
    current: &Path,
    images: &[(&str, &str)],
    control: Control<'_>,
) -> io::Result<()> {
    let requirement = RefCell::new(None);
    qualify_with_control(
        current,
        images,
        control,
        |path| content_interruptible(path, control),
        |path| {
            let mut captured = requirement.borrow_mut();
            if captured.is_none() {
                *captured = Some(signer::checked_application_requirement(current, control)?);
            }
            signer::verify(path, captured.as_ref().and_then(Option::as_deref), control)
        },
    )
}

pub(super) fn application_signer_requirement() -> io::Result<Option<String>> {
    let cancelled = AtomicBool::new(false);
    let control = Control::new(&cancelled, Instant::now() + MAX_ADMISSION_DURATION);
    control.check()?;
    let current = current_executable()?;
    signer::checked_application_requirement(&current, control)
}

pub(super) fn verify_signature(path: &Path, requirement: Option<&str>) -> io::Result<()> {
    let cancelled = AtomicBool::new(false);
    signer::verify(
        path,
        requirement,
        Control::new(&cancelled, Instant::now() + MAX_ADMISSION_DURATION),
    )
}

#[cfg(test)]
mod tests;
