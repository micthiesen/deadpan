#![cfg(target_os = "macos")]
//! Named Original moments through the shipped CLI and the live writer.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use deadpan_cli::host::Endpoint;
use deadpan_store::moments::{Action, MomentCatalog, MomentId, MomentName, MomentTags, Request};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

#[path = "support/live_owner.rs"]
mod live_owner;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str], succeeds: bool) -> Value {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        succeeds,
        "{arguments:?}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(if succeeds {
        &output.stdout
    } else {
        &output.stderr
    })
    .unwrap()
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn create(package: &Path) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()
        .unwrap();
    cli(
        &["project", "create-original", text(package), text(&fixture)],
        true,
    );
}

fn catalog(package: &Path) -> Result<MomentCatalog> {
    Ok(serde_json::from_value(
        cli(&["project", "moments", text(package)], true)["moments"].clone(),
    )?)
}

fn request(catalog: &MomentCatalog, action: Action) -> Request {
    Request {
        project_id: catalog.project_id.clone(),
        expected_version: catalog.version,
        action,
    }
}

fn write_request(path: &Path, request: &Request) -> Result {
    fs::write(
        path,
        serde_json::to_vec(&json!({"protocol":1,"request":request}))?,
    )?;
    Ok(())
}

fn apply(package: &Path, file: &Path, dry_run: bool, succeeds: bool) -> Value {
    let mut arguments = vec!["project", "moment", text(package), "--json", text(file)];
    if dry_run {
        arguments.push("--dry-run");
    }
    cli(&arguments, succeeds)
}

#[test]
fn cli_moments_create_rename_retag_delete_without_touching_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("moments.deadpan");
    let file = scratch.path().join("request.json");
    create(&package);
    let head = ProjectStore::open(&package, AccessMode::ReadOnly)?
        .snapshot()?
        .revision_id()
        .clone();
    let empty = catalog(&package)?;
    assert!(empty.entries.is_empty());
    let id = MomentId::new("moment-one")?;
    write_request(
        &file,
        &request(
            &empty,
            Action::Create {
                id: id.clone(),
                name: MomentName::new("Reaction shot")?,
                tags: MomentTags::from_input(["Reaction", "detail"])?,
                start: 3,
                end: 12,
            },
        ),
    )?;
    let dry = apply(&package, &file, true, true);
    assert_eq!(dry["committed"], false);
    assert_eq!(
        dry["outcome"]["catalog"]["entries"][0]["tags"],
        json!(["detail", "reaction"])
    );
    assert_eq!(catalog(&package)?, empty, "a dry run writes nothing");
    let saved = apply(&package, &file, false, true);
    assert_eq!(saved["outcome"]["changed"], true);
    // The same request again is refused as stale.
    let stale = apply(&package, &file, false, false);
    assert_eq!(stale["error"]["code"], "MomentsInvalid", "{stale}");

    let current = catalog(&package)?;
    assert_eq!(current.version, 1);
    write_request(
        &file,
        &request(
            &current,
            Action::Rename {
                id: id.clone(),
                name: MomentName::new("Stare")?,
            },
        ),
    )?;
    apply(&package, &file, false, true);
    let current = catalog(&package)?;
    write_request(
        &file,
        &request(
            &current,
            Action::Retag {
                id: id.clone(),
                tags: MomentTags::from_input(["breath"])?,
            },
        ),
    )?;
    apply(&package, &file, false, true);
    let current = catalog(&package)?;
    assert_eq!(current.entries[0].name.as_str(), "Stare");
    assert_eq!(current.entries[0].tags, MomentTags::from_input(["breath"])?);
    assert_eq!((current.entries[0].start, current.entries[0].end), (3, 12));

    // Past the Original's last picture is refused without a write.
    write_request(
        &file,
        &request(
            &current,
            Action::Create {
                id: MomentId::new("too-long")?,
                name: MomentName::new("Too long")?,
                tags: MomentTags::default(),
                start: 0,
                end: 1_000_000,
            },
        ),
    )?;
    apply(&package, &file, false, false);
    assert_eq!(catalog(&package)?, current);

    write_request(&file, &request(&current, Action::Delete { id }))?;
    apply(&package, &file, false, true);
    assert!(catalog(&package)?.entries.is_empty());
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?.revision_id(), &head);
    store.validate_full()?;
    Ok(())
}

#[test]
fn cli_moments_reach_an_open_projects_live_writer() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("live.deadpan");
    let file = scratch.path().join("request.json");
    create(&package);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let mut endpoint = Endpoint::bind(&mut store)?;
    let initial = store.moment_catalog()?;
    write_request(
        &file,
        &request(
            &initial,
            Action::Create {
                id: MomentId::new("live")?,
                name: MomentName::new("Live moment")?,
                tags: MomentTags::from_input(["reaction"])?,
                start: 0,
                end: 4,
            },
        ),
    )?;
    // Reads may use their own read-only connection beside the writer.
    let (listed, _) = live_owner::serve(&mut store, &mut endpoint, || {
        cli(&["project", "moments", text(&package)], true)
    });
    assert_eq!(listed["moments"]["version"], 0);
    let (saved, count) = live_owner::serve(&mut store, &mut endpoint, || {
        apply(&package, &file, false, true)
    });
    assert_eq!(count, 1);
    assert_eq!(saved["committed"], true);
    assert_eq!(store.moment_catalog()?.entries.len(), 1);
    Ok(())
}
