//! Locations inside a running packaged `Deadpan.app`.
//!
//! Every executable ships in `Contents/MacOS`, so worker helpers keep resolving
//! beside the current executable. Read-only bundled resources such as the
//! downloader baseline resolve from the running bundle's own `Contents`, never
//! from the working directory or a build-time environment variable.
//!
//! Only bundles built by `cargo xtask bundle` count as packaged: their
//! `Info.plist`, which the main executable's signature binds, carries
//! [`PACKAGING_KEY`]. Developer wrappers such as `tools/build-app.py` output
//! keep development behavior. See [packaging](../../../docs/PACKAGING.md).

use std::path::{Path, PathBuf};

mod image;
mod native;
mod packaging_marker;
mod refinement;
pub use native::{
    NativeWorker, native_content_pins, qualify_native_worker, qualify_native_worker_interruptible,
};
pub use refinement::{qualify_refinement_worker, refinement_content_pins};

/// Shared bounded codesign policy for the native and downloader helpers.
pub(crate) fn application_signer_requirement() -> std::io::Result<Option<String>> {
    image::application_signer_requirement()
}

pub(crate) fn verify_signature(path: &Path, requirement: Option<&str>) -> std::io::Result<()> {
    image::verify_signature(path, requirement)
}

/// Downloader baseline below `Contents`.
pub const HELPERS_DIRECTORY: &str = "Resources/helpers";
/// Bundled FFmpeg libraries below `Contents`.
pub const FRAMEWORKS_DIRECTORY: &str = "Frameworks";
/// `Info.plist` key whose presence marks a packaged bundle.
pub const PACKAGING_KEY: &str = "DeadpanPackaging";
/// Its value for the current layout.
pub const PACKAGING_LAYOUT: &str = "xtask-bundle-1";
const INFO_LIMIT: u64 = 256 * 1024;

/// `X.app/Contents` when `executable` is `X.app/Contents/MacOS/<name>`.
pub fn contents_of(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension()? == "app")
        .then(|| contents.to_owned())
}

/// Whether `contents/Info.plist` (XML) declares the packaged layout.
pub fn is_packaged(contents: &Path) -> bool {
    use rustix::fs::{Mode, OFlags, open};
    use std::io::Read;
    let path = contents.join("Info.plist");
    let Ok(descriptor) = open(
        &path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) else {
        return false;
    };
    let file = std::fs::File::from(descriptor);
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    if !metadata.file_type().is_file() || metadata.len() > INFO_LIMIT {
        return false;
    }
    let mut text = String::new();
    let read = file.take(INFO_LIMIT + 1).read_to_string(&mut text);
    read.is_ok() && text.len() as u64 <= INFO_LIMIT && declares_packaging(&text)
}

fn declares_packaging(info: &str) -> bool {
    packaging_marker::declared(info)
}

/// The running bundle's `Contents`, after resolving symbolic links in the
/// executable path. `None` for a bare Cargo or development executable.
pub fn running_contents() -> Option<PathBuf> {
    let executable = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
    contents_of(&executable)
}

/// The running packaged bundle's `Contents`; `None` for development builds,
/// including developer wrapper bundles.
pub fn packaged_contents() -> Option<PathBuf> {
    running_contents().filter(|contents| is_packaged(contents))
}

fn checked_contents_of(
    executable: &Path,
    requires_package: bool,
) -> std::io::Result<Option<PathBuf>> {
    let contents = contents_of(executable);
    let packaged = contents.as_deref().is_some_and(is_packaged);
    if requires_package && !packaged {
        return Err(image::invalid(
            "pin-built host is outside its packaged application or lost its DeadpanPackaging marker",
        ));
    }
    Ok(contents.filter(|_| packaged))
}

/// Cheap package selection for UI/writer-side runtime discovery. Compiled
/// shipping pins make a missing marker or an escaped host an error, never a
/// development-runtime fallback. Full image admission belongs before launch.
pub fn checked_packaged_contents() -> std::io::Result<Option<PathBuf>> {
    checked_contents_of(
        &image::current_executable()?,
        image::any_compiled_runtime_pins(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_selection_never_downgrades_any_pin_built_host() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("Deadpan.app/Contents/MacOS/deadpan-cli");
        let contents = contents_of(&executable).unwrap();
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        for requires_package in [false, true] {
            std::fs::write(
                contents.join("Info.plist"),
                format!("<plist version=\"1.0\"><dict><key>{PACKAGING_KEY}</key><string>{PACKAGING_LAYOUT}</string></dict></plist>"),
            )
            .unwrap();
            assert_eq!(
                checked_contents_of(&executable, requires_package).unwrap(),
                Some(contents.clone())
            );
            std::fs::remove_file(contents.join("Info.plist")).unwrap();
            assert_eq!(
                checked_contents_of(&executable, requires_package).is_err(),
                requires_package
            );
            std::fs::write(
                contents.join("Info.plist"),
                "<plist version=\"1.0\"><dict><key>DeadpanPackaging</key><string>other</string></dict></plist>",
            )
            .unwrap();
            assert_eq!(
                checked_contents_of(&executable, requires_package).is_err(),
                requires_package
            );
            assert_eq!(
                checked_contents_of(Path::new("/tmp/deadpan-cli"), requires_package).is_err(),
                requires_package
            );
        }
    }

    #[test]
    fn only_contents_macos_inside_an_app_is_a_bundle() {
        assert_eq!(
            contents_of(Path::new("/A/Deadpan.app/Contents/MacOS/deadpan-cli")),
            Some(PathBuf::from("/A/Deadpan.app/Contents"))
        );
        for path in [
            "/A/target/release/deadpan-cli",
            "/A/Deadpan/Contents/MacOS/deadpan-cli",
            "/A/Deadpan.app/Contents/Helpers/deadpan-cli",
            "/A/Deadpan.app/MacOS/deadpan-cli",
            "deadpan-cli",
        ] {
            assert_eq!(contents_of(Path::new(path)), None, "{path}");
        }
    }

    #[test]
    fn only_the_packaging_key_marks_a_packaged_bundle() {
        let directory = tempfile::tempdir().unwrap();
        let contents = directory.path();
        assert!(!is_packaged(contents));
        let plist = |body: &str| {
            std::fs::write(
                contents.join("Info.plist"),
                format!("<?xml version=\"1.0\"?>\n<plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"),
            )
            .unwrap();
        };
        // tools/build-app.py output has no key.
        plist("\t<key>CFBundleExecutable</key>\n\t<string>deadpan-app</string>\n");
        assert!(!is_packaged(contents));
        plist("\t<key>DeadpanPackaging</key>\n\t<string>other</string>\n");
        assert!(!is_packaged(contents));
        plist("\t<key>DeadpanPackaging</key>\n\t<string>xtask-bundle-1</string>\n");
        assert!(is_packaged(contents));
    }
}
