//! Launch trust for the packaged alignment worker and its one ORT library.
//! The final host build embeds pins after the worker's bundle relocation.
//! Resource manifests and worker reports are never executable authorities.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(test)]
use super::image::content;
use super::image::{self, invalid, valid_pin};
#[cfg(test)]
use crate::youtube::helpers::signing;
#[cfg(test)]
use std::fs;

const WORKER_PIN: Option<&str> = option_env!("DEADPAN_ALIGN_WORKER_CONTENT_SHA256");
const ORT_PIN: Option<&str> = option_env!("DEADPAN_ALIGN_ORT_CONTENT_SHA256");
const WORKER: &str = "MacOS/deadpan-align";
const ORT: &str = "Frameworks/libonnxruntime.1.dylib";

/// Independent pins embedded into the final App/CLI, absent in development.
pub fn refinement_content_pins() -> Option<(&'static str, &'static str)> {
    WORKER_PIN.zip(ORT_PIN)
}

fn qualify_using(
    current: &Path,
    pins: (Option<&str>, Option<&str>),
    requires_package: bool,
    qualify: impl Fn(&Path, &[(&str, &str)]) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let contents = super::contents_of(current);
    let packaged = contents.as_deref().is_some_and(super::is_packaged);
    let expected = match pins {
        (None, None) if !packaged && !requires_package => {
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
    qualify(current, &[(WORKER, expected.0), (ORT, expected.1)])?;
    let contents = contents.ok_or_else(|| invalid("pin-built host is outside its application"))?;
    Ok(contents.join(WORKER))
}

#[cfg(test)]
fn qualify_with(
    current: &Path,
    pins: (Option<&str>, Option<&str>),
    hash: impl Fn(&Path) -> io::Result<String>,
    verify: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    qualify_using(current, pins, false, |current, images| {
        image::qualify_with(current, images, &hash, &verify)
    })
}

/// Select the sibling worker in development. A pin-built packaged host verifies
/// its resource seal, executable/library content and signatures before launch.
/// Removing its marker or moving it outside the app never enables development.
pub fn qualify_refinement_worker() -> io::Result<PathBuf> {
    let current = image::current_executable()?;
    qualify_using(
        &current,
        (WORKER_PIN, ORT_PIN),
        image::any_compiled_runtime_pins(),
        image::qualify,
    )
}

#[cfg(test)]
mod tests;
