use std::process::Command;

#[test]
fn doctor_reports_real_timing_probe_and_missing_capabilities() {
    let output = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .arg("doctor")
        .output()
        .expect("run doctor");
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["stage"], "development-foundation");
    assert_eq!(report["timing_probe"]["sample_boundary"], 1_602);
    assert_eq!(
        report["render"],
        "automatic-sdr-committed-revision-macos-apfs"
    );
    assert_eq!(
        report["render_entrypoints"],
        serde_json::json!([
            "native-cmd-e-and-render-command",
            "closed-project-headless-render-and-recovery"
        ])
    );
    assert_eq!(
        report["render_preview_choices"],
        serde_json::json!(["commit-and-render", "discard-and-render", "keep-editing"])
    );
    let helpers = report["downloader"]["helpers"].as_array().unwrap();
    assert_eq!(helpers[0]["name"], "yt-dlp");
    assert_eq!(helpers[0]["version"], "2026.08.19");
    assert_eq!(helpers[1]["name"], "deno");
    assert_eq!(report["downloader"]["ejs"], "0.8.0");
    let missing = report["unimplemented"].as_array().unwrap();
    assert!(missing.contains(&serde_json::json!("native-youtube-import")));
    assert!(!missing.contains(&serde_json::json!("export")));
    for capability in [
        "full-render-mastering",
        "hdr-render",
        "open-project-render-ipc",
        "native-render-recovery-browser",
    ] {
        assert!(
            missing.contains(&serde_json::json!(capability)),
            "{capability}"
        );
    }
}

#[test]
fn unsupported_command_fails_without_success_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .arg("not-a-command")
        .output()
        .expect("run unsupported command");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unknown command"));
}

#[cfg(target_os = "macos")]
#[test]
fn project_doctor_names_real_sources_and_times_revision_stages_read_only() {
    let scratch = tempfile::tempdir().expect("scratch");
    let package = scratch.path().join("clip.deadpan");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()
        .expect("fixture");
    let run = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
            .args(arguments)
            .output()
            .expect("run cli");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("valid JSON")
    };
    let package_text = package.to_str().expect("UTF-8");
    run(&[
        "project",
        "create-original",
        package_text,
        fixture.to_str().expect("UTF-8"),
    ]);
    let before = run(&["project", "validate", package_text]);
    let files_before = package_files(&package);
    assert!(
        files_before
            .keys()
            .any(|path| path.ends_with("project.sqlite")),
        "{files_before:?}"
    );
    let report = run(&["doctor", "--project", package_text]);
    // Read-only: no file appears or disappears, and every file keeps its size
    // and modification time, including project.sqlite and any -wal. The one
    // exception is the -shm WAL index, whose read marks every SQLite reader
    // (including `project validate`) updates; its size must not change.
    let files_after = package_files(&package);
    let keys: std::collections::BTreeSet<_> =
        files_before.keys().chain(files_after.keys()).collect();
    let changed: Vec<_> = keys
        .into_iter()
        .filter(|path| {
            let (before, after) = (files_before.get(*path), files_after.get(*path));
            if path.to_string_lossy().ends_with("-shm") {
                before.map(|file| file.0) != after.map(|file| file.0)
            } else {
                before != after
            }
        })
        .map(|path| (path, files_before.get(path), files_after.get(path)))
        .collect();
    assert!(
        changed.is_empty(),
        "doctor changed package files: {changed:?}"
    );
    assert_eq!(report["schema_version"], 1);
    let project = &report["project"];
    assert_eq!(project["revision"], before["revision_id"]);
    for stage in [
        "open_read_only",
        "head_snapshot",
        "validate_durations",
        "plan_compile",
        "anchor_index",
    ] {
        assert!(
            project["single_sample_ms"][stage]
                .as_f64()
                .is_some_and(|ms| ms >= 0.0),
            "{stage}"
        );
    }
    assert_eq!(project["document"]["root_children"], 1);
    assert_eq!(
        project["preview"]["canvas"],
        before["presentation_basis"]["width"]
            .as_u64()
            .map(|width| serde_json::json!([width, before["presentation_basis"]["height"]]))
            .unwrap()
    );
    let sources = project["sources"].as_array().expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["qualification"], "qualified");
    assert_eq!(sources[0]["video"]["codec"], "h264");
    assert!(
        sources[0]["video"]["indexed_frames"]
            .as_u64()
            .is_some_and(|frames| frames > 0)
    );
    assert!(
        sources[0]["video"]["keyframes"]
            .as_u64()
            .is_some_and(|keys| keys > 0)
    );
    // Process-local counters observed this process's own probes: opening and
    // snapshotting the head read revision rows. Nothing was written, no GPU
    // work was submitted and no worker ran.
    let diagnostics = &report["diagnostics"];
    let revisions = &diagnostics["file_io"]["store_revisions"];
    assert!(revisions["read_ops"].as_u64().is_some_and(|ops| ops > 0));
    assert!(
        revisions["read_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0)
    );
    assert_eq!(revisions["write_ops"], 0);
    assert_eq!(diagnostics["gpu_submissions"]["submissions"], 0);
    assert_eq!(diagnostics["model_workers"]["live"]["high"], 0);
    for queue in [
        "picture_preview",
        "thumbnails",
        "playback_prepared",
        "playback_device_packets",
    ] {
        assert_eq!(diagnostics["queue_depths"][queue]["current"], 0, "{queue}");
    }
    assert!(diagnostics["pcm_caches"]["decoded_pcm"]["hits"].is_u64());
    // Diagnostics never write: the head revision is unchanged.
    assert_eq!(
        run(&["project", "validate", package_text])["revision_id"],
        before["revision_id"]
    );
}

/// Every regular file under a package with its size and modification time.
#[cfg(target_os = "macos")]
fn package_files(
    root: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, (u64, std::time::SystemTime)> {
    let mut files = std::collections::BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("read package") {
            let entry = entry.expect("package entry");
            let metadata = std::fs::symlink_metadata(entry.path()).expect("metadata");
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                files.insert(
                    entry.path(),
                    (metadata.len(), metadata.modified().expect("mtime")),
                );
            }
        }
    }
    files
}
