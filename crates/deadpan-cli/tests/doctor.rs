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
    assert!(
        report["unimplemented"]
            .as_array()
            .unwrap()
            .contains(&"export".into())
    );
}

#[test]
fn unsupported_command_fails_without_success_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .arg("render")
        .output()
        .expect("run unsupported command");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unknown command"));
}
