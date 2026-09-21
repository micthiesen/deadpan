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
