//! Compiled launch identities for the older native worker family. The
//! informational bundle manifest never supplies executable authority.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::image::{self, invalid, valid_pin};

const COMPILED_PINS: Option<&str> = option_env!("DEADPAN_NATIVE_CONTENT_PINS");
const MAX_PINS_BYTES: usize = 8 * 1024;
const LIBRARIES: [&str; 6] = [
    "Frameworks/libavcodec.62.dylib",
    "Frameworks/libavformat.62.dylib",
    "Frameworks/libavutil.60.dylib",
    "Frameworks/libavfilter.11.dylib",
    "Frameworks/libswresample.6.dylib",
    "Frameworks/libswscale.9.dylib",
];

/// Worker role chosen by the host, never by a project or worker response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeWorker {
    Transcribe,
    Track,
    Media,
}

impl NativeWorker {
    const ALL: [Self; 3] = [Self::Transcribe, Self::Track, Self::Media];

    /// The worker's fixed path below this app's Contents directory.
    pub fn relative_path(self) -> &'static str {
        match self {
            Self::Transcribe => "MacOS/deadpan-transcribe",
            Self::Track => "MacOS/deadpan-track",
            Self::Media => "MacOS/deadpan-media-worker",
        }
    }
}

fn parse_pins(source: Option<&str>) -> io::Result<Option<BTreeMap<String, String>>> {
    let Some(source) = source else {
        return Ok(None);
    };
    if source.len() > MAX_PINS_BYTES {
        return Err(invalid("compiled native image pins exceed their bound"));
    }
    let pins: BTreeMap<String, String> = serde_json::from_str(source).map_err(invalid)?;
    let expected = NativeWorker::ALL
        .into_iter()
        .map(NativeWorker::relative_path)
        .chain(LIBRARIES);
    if pins.len() != NativeWorker::ALL.len() + LIBRARIES.len()
        || expected.into_iter().any(|name| !pins.contains_key(name))
        || pins.values().any(|pin| !valid_pin(pin))
    {
        return Err(invalid(
            "compiled native image pins are missing, extra or invalid",
        ));
    }
    Ok(Some(pins))
}

/// Independent image pins embedded into final App/CLI builds. No filesystem
/// or resource-manifest input can change this report.
pub fn native_content_pins() -> io::Result<Option<BTreeMap<String, String>>> {
    parse_pins(COMPILED_PINS)
}

fn qualify_with(
    current: &Path,
    kind: NativeWorker,
    executable: &Path,
    pins: Option<&str>,
    requires_package: bool,
    qualify: impl Fn(&Path, &[(&str, &str)]) -> io::Result<()>,
) -> io::Result<()> {
    let packaged = super::contents_of(current)
        .as_deref()
        .is_some_and(super::is_packaged);
    let Some(pins) = parse_pins(pins)? else {
        return if packaged || requires_package {
            Err(invalid(
                "packaged native worker requires compiled content pins",
            ))
        } else {
            Ok(())
        };
    };
    let contents = super::contents_of(current)
        .ok_or_else(|| invalid("pin-built native host is outside its application"))?;
    if !packaged {
        return Err(invalid(
            "pin-built native host lost its DeadpanPackaging marker",
        ));
    }
    if executable != contents.join(kind.relative_path()) {
        return Err(invalid(
            "native executable is not its role's packaged sibling",
        ));
    }
    let selected = std::iter::once(kind.relative_path())
        .chain(LIBRARIES)
        .map(|name| (name, pins[name].as_str()))
        .collect::<Vec<_>>();
    qualify(current, &selected)
}

/// Admit an imminent launch on a preparation/CLI worker. In development the
/// explicit supplied path remains available to test workers. Packaged and
/// pin-built hosts can launch only their fixed, independently pinned images.
/// This is admission, not containment against a concurrent same-user writer.
pub fn qualify_native_worker(kind: NativeWorker, executable: &Path) -> io::Result<()> {
    qualify_native_worker_interruptible(
        kind,
        executable,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(60),
    )
}

/// The caller's cancellation and deadline also cover every image read/hash
/// and signature subprocess. One admission has an overall 60-second ceiling.
pub fn qualify_native_worker_interruptible(
    kind: NativeWorker,
    executable: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> io::Result<()> {
    let control = image::Control::new(cancelled, deadline);
    control.check()?;
    let current = image::current_executable()?;
    control.check()?;
    let executable = image::normalize_executable(executable.to_owned())?;
    control.check()?;
    qualify_with(
        &current,
        kind,
        &executable,
        COMPILED_PINS,
        image::any_compiled_runtime_pins(),
        |current, images| image::qualify_interruptible(current, images, control),
    )
}

#[cfg(test)]
mod tests;
