use std::process::Command;

#[test]
fn native_binary_runs_headless_without_initializing_a_window() {
    let output = Command::new(env!("CARGO_BIN_EXE_deadpan-app"))
        .args(["--headless", "doctor"])
        .output()
        .expect("headless app");
    assert!(output.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON report only");
    assert_eq!(report["application"], "deadpan");
    assert!(output.stderr.is_empty());
}

#[test]
fn headless_errors_keep_the_same_structured_protocol() {
    let output = Command::new(env!("CARGO_BIN_EXE_deadpan-app"))
        .args(["--headless", "unsupported"])
        .output()
        .expect("headless app");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("JSON error only");
    assert_eq!(report["error"]["code"], "InvalidInput");
}

#[test]
fn native_binary_inspects_saves_and_runs_macros_without_a_window() {
    use deadpan_store::{AccessMode, ProjectStore};
    use serde_json::json;

    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("native-headless-macro.deadpan");
    let path = package.to_str().unwrap();
    let run = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_deadpan-app"))
            .arg("--headless")
            .args(arguments)
            .output()
            .expect("headless macro process");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    run(&["project", "create", path]);
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    let inspected = run(&["macro", "inspect", path]);
    assert_eq!(inspected["bank_version"], 0);
    let file = scratch.path().join("macro.json");
    let mut request = json!({
        "protocol":1,"project_id":document.project_id(),
        "expected_revision":document.revision_id(),"expected_bank_version":0,
        "operation":{"type":"save","register":"a","program":{"instructions":[
            {"type":"move_frames","forward":true,"count":2}
        ]}}
    });
    std::fs::write(&file, serde_json::to_vec(&request).unwrap()).unwrap();
    let saved = run(&["macro", path, "--json", file.to_str().unwrap()]);
    assert_eq!(saved["committed_registers"]["bank_version"], 1);
    assert!(saved["committed_revision"].is_null());
    let inspected = run(&["macro", "inspect", path, "--register", "a"]);
    assert_eq!(
        inspected["registers"][0]["program"],
        request["operation"]["program"]
    );
    request["expected_bank_version"] = json!(1);
    request["operation"] = json!({
        "type":"run","register":"a","parent":document.root(),"cursor":0,"count":2
    });
    std::fs::write(&file, serde_json::to_vec(&request).unwrap()).unwrap();
    let moved = run(&["macro", path, "--json", file.to_str().unwrap()]);
    assert_eq!(moved["committed"], false);
    assert_eq!(
        moved["context"]["cursor"], 0,
        "empty Sequence clamps motion"
    );
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    assert_eq!(reader.snapshot().unwrap(), document);
    assert_eq!(reader.register_version().unwrap(), 1);
    assert_eq!(reader.history_availability().unwrap(), (false, false));
}

#[test]
fn native_binary_dispatches_private_render_workers_without_a_window_or_argument_wrapper() {
    use deadpan_cli::{encoded_render, render_worker};
    for (arguments, expected) in [
        (
            vec![render_worker::PRIVATE_WORKER_ARGUMENT, "/tmp"],
            "render picture worker:",
        ),
        (
            vec![encoded_render::PRIVATE_WORKER_ARGUMENT, "/tmp"],
            "render encode worker:",
        ),
        (
            vec![encoded_render::verification::PRIVATE_WORKER_ARGUMENT],
            "render verification:",
        ),
        (
            vec![encoded_render::admission::PRIVATE_WORKER_ARGUMENT],
            "render encoder probe:",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_deadpan-app"))
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("private native worker");
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).expect("worker diagnostic");
        assert!(error.starts_with(expected), "{error}");
        assert!(error.contains("expected one"), "{error}");
    }
}
