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
