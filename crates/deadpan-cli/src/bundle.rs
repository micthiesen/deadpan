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
    use std::io::Read;
    let path = contents.join("Info.plist");
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return false;
    };
    if !metadata.file_type().is_file() || metadata.len() > INFO_LIMIT {
        return false;
    }
    let mut text = String::new();
    let read =
        std::fs::File::open(&path).and_then(|file| file.take(INFO_LIMIT).read_to_string(&mut text));
    read.is_ok() && declares_packaging(&text)
}

fn declares_packaging(info: &str) -> bool {
    let key = format!("<key>{PACKAGING_KEY}</key>");
    info.find(&key).is_some_and(|at| {
        info[at + key.len()..]
            .trim_start()
            .starts_with(&format!("<string>{PACKAGING_LAYOUT}</string>"))
    })
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

#[cfg(test)]
mod tests {
    use super::*;

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
