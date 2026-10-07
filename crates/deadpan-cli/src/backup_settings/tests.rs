use std::os::unix::fs::MetadataExt;

use super::*;

#[test]
fn missing_settings_use_defaults_without_creating_a_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Deadpan/backups.json");
    let loaded = Settings::load_from(&path).unwrap();
    assert_eq!(loaded.settings, Settings::default());
    assert_eq!(loaded.source, Source::Default);
    assert!(!path.exists());
}

#[test]
fn settings_are_bounded_and_strict_on_construction_and_deserialization() {
    assert!(Settings::new(0, 48, 4096).is_err());
    assert!(Settings::new(1441, 48, 4096).is_err());
    assert!(Settings::new(15, 7, 4096).is_err());
    assert!(Settings::new(15, 257, 4096).is_err());
    assert!(Settings::new(15, 48, 255).is_err());
    assert!(Settings::new(15, 48, 65_537).is_err());
    assert!(
        serde_json::from_str::<Settings>(
            r#"{"schema":1,"interval_minutes":15,"max_count":48,"budget_mib":4096,"other":true}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<Settings>(
            r#"{"schema":2,"interval_minutes":15,"max_count":48,"budget_mib":4096}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<Settings>(
            r#"{"schema":1,"interval_minutes":0,"max_count":48,"budget_mib":4096}"#
        )
        .is_err()
    );
}

#[test]
fn settings_survive_restart_and_are_private() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Deadpan/backups.json");
    let settings = Settings::new(90, 128, 8192).unwrap();
    let saved = settings.save_to(&path).unwrap();
    assert_eq!(saved.warning, None);
    assert_eq!(Settings::load_from(&path).unwrap().settings, settings);
    assert_eq!(fs::symlink_metadata(path).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn invalid_settings_file_is_not_treated_as_missing() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("backups.json");
    fs::write(&path, b"not json").unwrap();
    assert!(matches!(
        Settings::load_from(&path),
        Err(SettingsError::InvalidFile { .. })
    ));
}

#[test]
fn settings_refuse_links_and_special_files_without_waiting_for_a_writer() {
    let root = tempfile::tempdir().unwrap();
    let regular = root.path().join("regular.json");
    Settings::default().save_to(&regular).unwrap();
    let link = root.path().join("linked.json");
    std::os::unix::fs::symlink(&regular, &link).unwrap();
    assert!(Settings::load_from(&link).is_err());
    let fifo = root.path().join("fifo");
    #[cfg(not(target_os = "macos"))]
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("mkfifo");
        command.arg(&fifo);
        assert!(
            deadpan_native_process::spawn(&mut command)
                .unwrap()
                .wait()
                .unwrap()
                .success()
        );
    }
    assert!(matches!(
        Settings::load_from(&fifo),
        Err(SettingsError::InvalidFile { .. })
    ));
    assert!(matches!(
        Settings::load_from(root.path()),
        Err(SettingsError::InvalidFile { .. })
    ));
}

#[test]
fn untrusted_settings_keep_every_backup_without_changing_default_cadence() {
    let settings = Settings::default();
    let policy = settings.policy_without_pruning();
    assert_eq!(policy.interval, Duration::from_secs(15 * 60));
    assert_eq!(policy.keep_recent, usize::MAX);
    assert_eq!(policy.max_count, usize::MAX);
    assert_eq!(policy.max_total_bytes, u64::MAX);

    let backups = (0..20)
        .map(|index| deadpan_store::backups::BackupInfo {
            id: format!("backup-{index}"),
            path: PathBuf::from(format!("backup-{index}.sqlite")),
            reason: deadpan_store::backups::BackupReason::Periodic,
            created_unix_ms: 1_000_000 - index * 1_000,
            database_bytes: 1024 * 1024 * 1024,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        deadpan_store::backups::retained(&backups, &policy, 1_000_000).len(),
        backups.len()
    );
}
