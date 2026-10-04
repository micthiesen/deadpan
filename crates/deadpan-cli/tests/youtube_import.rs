//! Headless YouTube import refusals and helper status through the binary.
#![cfg(target_os = "macos")]

use std::process::Command;

fn run(arguments: &[&str]) -> (bool, serde_json::Value, serde_json::Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()
        .expect("run deadpan-cli");
    let parse = |bytes: &[u8]| serde_json::from_slice(bytes).unwrap_or(serde_json::Value::Null);
    (
        output.status.success(),
        parse(&output.stdout),
        parse(&output.stderr),
    )
}

#[test]
fn invalid_urls_and_missing_helpers_create_nothing() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("never.deadpan");
    let package = package.to_str().unwrap();
    let helpers = scratch.path().join("helpers");
    let helpers = helpers.to_str().unwrap();
    for (url, code) in [
        (
            "http://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "YouTubeUrlInvalid",
        ),
        ("https://vimeo.com/123", "YouTubeUrlInvalid"),
        (
            "https://www.youtube.com/playlist?list=PL123",
            "YouTubePlaylistNeedsVideo",
        ),
        ("https://youtu.be/dQw4w9WgXcQ", "DownloaderNotInstalled"),
    ] {
        let (success, _, error) = run(&[
            "project",
            "create-from-url",
            package,
            url,
            "--helpers",
            helpers,
        ]);
        assert!(!success);
        assert_eq!(error["error"]["code"], code, "{url}");
        assert!(!std::path::Path::new(package).exists());
    }
    let (success, _, error) = run(&["project", "create-from-url", package]);
    assert!(!success);
    assert_eq!(error["error"]["code"], "InvalidInput");
}

#[test]
fn status_reports_pins_without_installing() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().to_str().unwrap();
    let (success, report, _) = run(&["downloader", "status", "--root", root]);
    assert!(success);
    let helpers = report["helpers"].as_array().unwrap();
    assert_eq!(helpers.len(), 2);
    assert!(helpers.iter().all(|helper| helper["installed"] == false));
    assert_eq!(report["ejs"]["version"], "0.8.0");
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
    let (success, _, _) = run(&["downloader", "status", "--root", "relative"]);
    assert!(!success);
}

#[test]
fn failed_creation_leaves_nothing_and_the_path_stays_reusable() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("clip.deadpan");
    let package_text = package.to_str().unwrap();
    let not_video = scratch.path().join("notes.mp4");
    std::fs::write(&not_video, b"not a video").unwrap();
    let (success, _, error) = run(&[
        "project",
        "create-original",
        package_text,
        not_video.to_str().unwrap(),
    ]);
    assert!(!success, "{error}");
    let leftovers: Vec<_> = std::fs::read_dir(scratch.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, [std::ffi::OsString::from("notes.mp4")]);

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()
        .unwrap();
    let (success, created, error) = run(&[
        "project",
        "create-original",
        package_text,
        fixture.to_str().unwrap(),
    ]);
    assert!(success, "{error}");
    assert_eq!(created["created"]["single_source"]["state"], "ready");
    let (success, report, _) = run(&["project", "validate", package_text]);
    assert!(success && report["valid"] == true);
    // An existing project is never replaced.
    let (success, _, error) = run(&[
        "project",
        "create-original",
        package_text,
        fixture.to_str().unwrap(),
    ]);
    assert!(!success);
    assert_eq!(error["error"]["code"], "InvalidInput");
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 2);
}
