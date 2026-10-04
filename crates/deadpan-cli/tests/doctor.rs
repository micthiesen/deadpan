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
