//! Pre-exec authority for the packaged private Python/model runtime.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use super::ai_inventory::{self, Entry};
use super::image::{self, Control, invalid, valid_pin};

pub const AI_RUNTIME_DIRECTORY: &str = "Resources/ai-runtime";
pub const AI_RUNTIME_INVENTORY: &str = "Resources/ai-runtime-inventory.json";
const COMPILED_INVENTORY: Option<&str> = option_env!("DEADPAN_AI_RUNTIME_INVENTORY_SHA256");

/// Executable and import roots selected by the host. Model weights remain
/// separately admitted pack data and are never Python import roots.
#[derive(Clone, Copy, Debug)]
pub struct AiRuntimePaths<'a> {
    pub python: &'a Path,
    pub worker_script: &'a Path,
    pub runtime_source: &'a Path,
    pub ffmpeg: &'a Path,
    pub ffprobe: &'a Path,
}

pub fn ai_runtime_inventory_pin() -> Option<&'static str> {
    COMPILED_INVENTORY
}

fn read_inventory(path: &Path, control: Control<'_>) -> io::Result<Vec<u8>> {
    use rustix::fs::{Mode, OFlags, open};
    control.check()?;
    let mut file = File::from(open(
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > ai_inventory::MAX_MANIFEST_BYTES as u64 {
        return Err(invalid(
            "AI runtime inventory is not a bounded regular file",
        ));
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
        if bytes.len() + count > ai_inventory::MAX_MANIFEST_BYTES {
            return Err(invalid("AI runtime inventory grew beyond its bound"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    if bytes.len() as u64 != metadata.len() {
        return Err(invalid("AI runtime inventory changed while reading"));
    }
    Ok(bytes)
}

fn required_paths(
    root: &Path,
    paths: AiRuntimePaths<'_>,
    inventory: &ai_inventory::Inventory,
) -> io::Result<()> {
    for (provided, relative, kind) in [
        (paths.python, "python/bin/python3.12", 0),
        (paths.worker_script, "worker/worker.py", 1),
        (paths.runtime_source, "ltx-2-mlx", 2),
        (paths.ffmpeg, "bin/ffmpeg", 0),
        (paths.ffprobe, "bin/ffprobe", 0),
    ] {
        if provided != root.join(relative) {
            return Err(invalid(format!(
                "AI runtime {relative} is not its fixed packaged path"
            )));
        }
        let correct = matches!(
            (inventory.entries.get(relative), kind),
            (Some(Entry::Macho { .. }), 0)
                | (Some(Entry::File { .. }), 1)
                | (Some(Entry::Directory { .. }), 2)
        );
        if !correct {
            return Err(invalid(format!(
                "AI runtime inventory does not contain required {relative}"
            )));
        }
    }
    Ok(())
}

fn qualify_with(
    current: &Path,
    paths: AiRuntimePaths<'_>,
    pin: Option<&str>,
    requires_package: bool,
    control: Control<'_>,
    verify: impl Fn(&Path, &[(&str, &str)], Control<'_>) -> io::Result<()>,
) -> io::Result<()> {
    control.check()?;
    let Some(contents) = super::checked_contents_of(current, requires_package || pin.is_some())?
    else {
        return Ok(());
    };
    let pin = pin
        .filter(|pin| valid_pin(pin))
        .ok_or_else(|| invalid("packaged AI runtime requires a valid compiled inventory pin"))?;
    let app = contents
        .parent()
        .ok_or_else(|| invalid("application parent"))?;
    let manifest = contents.join(AI_RUNTIME_INVENTORY);
    image::regular_inside(app, &manifest)?;
    let bytes = read_inventory(&manifest, control)?;
    let digest = ai_inventory::digest(&bytes);
    control.check()?;
    if digest != pin {
        return Err(invalid(
            "AI runtime inventory differs from the host's compiled pin",
        ));
    }
    let expected = ai_inventory::parse(&bytes)?;
    control.check()?;
    let root = contents.join(AI_RUNTIME_DIRECTORY);
    required_paths(&root, paths, &expected)?;
    let actual = ai_inventory::collect(
        &root,
        || control.check(),
        |bytes| image::content_bytes(bytes, control),
    )?;
    control.check()?;
    if actual != expected {
        let changed = actual
            .entries
            .keys()
            .chain(expected.entries.keys())
            .find(|path| actual.entries.get(*path) != expected.entries.get(*path));
        return Err(invalid(format!(
            "AI runtime differs from its compiled inventory at {}",
            changed.map_or("root permissions", String::as_str)
        )));
    }
    let images = expected
        .entries
        .iter()
        .filter_map(|(path, entry)| match entry {
            Entry::Macho { content_sha256, .. } => Some((
                format!("{AI_RUNTIME_DIRECTORY}/{path}"),
                content_sha256.as_str(),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let images = images
        .iter()
        .map(|(path, hash)| (path.as_str(), *hash))
        .collect::<Vec<_>>();
    verify(current, &images, control)
}

/// Verify the complete closed runtime before any Python bytecode or native
/// image executes. Call only on the preparation/check/inference job thread.
pub fn qualify_ai_runtime(
    paths: AiRuntimePaths<'_>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> io::Result<()> {
    let control = Control::new(cancelled, deadline);
    control.check()?;
    let current = image::current_executable()?;
    qualify_with(
        &current,
        paths,
        COMPILED_INVENTORY,
        image::any_compiled_runtime_pins(),
        control,
        image::qualify_interruptible,
    )
}

/// The packaged host must remain sealed even when the owner deliberately
/// selects an external developer runtime. This grants no bundled-data bypass;
/// the launch caller classifies every supplied path before choosing this API.
pub fn qualify_ai_host(cancelled: &AtomicBool, deadline: Instant) -> io::Result<()> {
    let control = Control::new(cancelled, deadline);
    control.check()?;
    let current = image::current_executable()?;
    if super::checked_contents_of(&current, image::any_compiled_runtime_pins())?.is_some() {
        image::qualify_interruptible(&current, &[], control)?;
    }
    control.check()
}

#[cfg(test)]
mod tests;
