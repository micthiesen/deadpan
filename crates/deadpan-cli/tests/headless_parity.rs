#![cfg(any(target_os = "macos", target_os = "linux"))]
//! DP-21 parity commands that the native app derives from its focus: Original
//! copies into registers, whole-Original reuse, placed-sound edits, storage
//! cleanup, clock confirmation and backup restore. Each runs on a closed
//! project's own writer and, through a stand-in owner serving the live
//! endpoint, on an open one, with stale requests refused and dry runs
//! writing nothing.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deadpan_cli::host::Endpoint;
use deadpan_core::{NodeKind, RegisterName, RegisterValue};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

#[path = "support/live_owner.rs"]
mod live_owner;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()
        .expect("deadpan-cli runs")
}

fn parsed(output: &Output) -> Value {
    serde_json::from_slice(if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    })
    .unwrap_or_else(|_| {
        panic!(
            "not JSON: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn success(arguments: &[&str]) -> Value {
    let output = cli(arguments);
    assert!(
        output.status.success(),
        "{arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    parsed(&output)
}

fn failure(arguments: &[&str]) -> Value {
    let output = cli(arguments);
    assert!(
        !output.status.success(),
        "{arguments:?} unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    parsed(&output)
}

fn write(scratch: &Path, name: &str, value: &Value) -> PathBuf {
    let path = scratch.join(name);
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// A one-Original project with an audio-only catalog sound registered.
fn project(scratch: &Path) -> Result<PathBuf> {
    let video = scratch.join("original.mp4");
    fs::write(
        &video,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let package = scratch.join("parity.deadpan");
    success(&["project", "create-original", text(&package), text(&video)]);
    let sound = scratch.join("sound.wav");
    fs::write(
        &sound,
        include_bytes!("../../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav"),
    )?;
    let retained = success(&["project", "retain-original", text(&package), text(&sound)]);
    let revision = head(&package)?;
    let request = write(
        scratch,
        "sound-registration.json",
        &json!({
            "protocol": 1,
            "registration": {
                "expected_revision": revision, "new_revision": "sound-registered",
                "original": retained["retained_original"]["record"]["object"]["content"],
                "new_asset_id": "boing", "label": "Boing", "insertion": null
            },
            "streams": {"type":"audio_only","stream":0,"interpretation":"mono"}
        }),
    );
    success(&[
        "project",
        "register-source",
        text(&package),
        "--request-json",
        text(&request),
    ]);
    Ok(package)
}

fn head(package: &Path) -> Result<String> {
    Ok(ProjectStore::open(package, AccessMode::ReadOnly)?
        .head_revision()?
        .as_str()
        .to_owned())
}

fn bank_version(package: &Path) -> Result<u64> {
    Ok(ProjectStore::open(package, AccessMode::ReadOnly)?
        .registers()?
        .version)
}

fn yank(package: &Path, register: &str, ordinals: [u64; 2], bank: u64) -> Result<Value> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    Ok(json!({
        "protocol": 1,
        "project_id": document.project_id(),
        "expected_revision": document.revision_id(),
        "expected_bank_version": bank,
        "operation": {"type":"yank_original","register":register,
            "ordinals":{"start":ordinals[0],"end":ordinals[1]}}
    }))
}

#[test]
fn original_copies_fill_named_registers_and_paste_like_a_native_yank() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = project(scratch.path())?;
    let path = text(&package);
    let request = write(
        scratch.path(),
        "yank.json",
        &yank(&package, "o", [3, 9], 0)?,
    );

    let preview = success(&["macro", path, "--json", text(&request), "--dry-run"]);
    assert_eq!(preview["operation"], "yank_original", "{preview}");
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["bank_version"], 1);
    assert_eq!(preview["original"]["ordinals"], json!({"start":3,"end":9}));
    assert_eq!(bank_version(&package)?, 0, "a dry run writes nothing");

    let saved = success(&["macro", path, "--json", text(&request)]);
    assert_eq!(saved["committed"], true, "{saved}");
    assert!(saved["committed_revision"].is_null(), "bank only");
    assert_eq!(saved["committed_registers"]["bank_version"], 1);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let named = &bank.entries[&RegisterName::new('o')?];
    assert!(matches!(
        named.as_ref(),
        RegisterValue::Original { ordinals, .. } if *ordinals == (3..9)
    ));
    // Like `y` in Original, the unnamed register holds the same copy.
    assert_eq!(&bank.entries[&RegisterName::unnamed()], named);
    drop(store);

    // The stale bank version and an empty or out-of-range copy are refused.
    let stale = failure(&["macro", path, "--json", text(&request)]);
    assert_eq!(stale["error"]["code"], "RegisterInvalid", "{stale}");
    for ordinals in [[4, 4], [0, 1_000_000]] {
        let bad = write(
            scratch.path(),
            "bad.json",
            &yank(&package, "o", ordinals, 1)?,
        );
        assert!(!cli(&["macro", path, "--json", text(&bad)]).status.success());
    }
    assert_eq!(bank_version(&package)?, 1);

    // The copy pastes through the ordinary semantic Apply.
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let before = document.nodes().len();
    drop(store);
    let paste = write(
        scratch.path(),
        "paste.json",
        &json!({
            "protocol": 1, "project_id": document.project_id(),
            "expected_revision": document.revision_id(), "expected_bank_version": 1,
            "operation": {"type":"apply","parent":document.root(),"cursor":0,
                "selected_child": document.children(document.root()).next(),
                "program":{"instructions":[{"type":"paste","register":"o","before":true}]}}
        }),
    );
    let pasted = success(&["macro", path, "--json", text(&paste)]);
    assert!(pasted["committed_revision"].is_string(), "{pasted}");
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(after.nodes().len() > before);
    assert_eq!(after.assets().len(), document.assets().len());
    Ok(())
}

#[test]
fn whole_original_reuse_inserts_the_registered_original_without_a_new_asset() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = project(scratch.path())?;
    let path = text(&package);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    drop(store);
    let root = document.root().as_str().to_owned();
    let revision = document.revision_id().as_str().to_owned();
    let arguments = [
        "project",
        "insert-original",
        path,
        "--parent",
        &root,
        "--index",
        "1",
        "--expected",
        &revision,
    ];
    let mut dry = arguments.to_vec();
    dry.push("--dry-run");
    let preview = success(&dry);
    assert_eq!(preview["committed"], false, "{preview}");
    assert_eq!(head(&package)?, revision);

    let inserted = success(&arguments);
    assert_eq!(inserted["committed"], true, "{inserted}");
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(after.assets(), document.assets(), "the Original is reused");
    let children: Vec<_> = after.children(after.root()).cloned().collect();
    assert_eq!(
        children.len(),
        document.children(document.root()).count() + 1
    );
    let NodeKind::Source { source } = &after.nodes()[&children[1]].kind else {
        panic!("a Source beat")
    };
    let original = document
        .nodes()
        .values()
        .find_map(|node| match &node.kind {
            NodeKind::Source { source } => Some(source),
            _ => None,
        })
        .unwrap();
    assert_eq!(source.video, original.video);
    assert_eq!(source.video_mapping, original.video_mapping);

    // The captured revision is no longer current.
    let stale = failure(&arguments);
    assert_eq!(stale["error"]["code"], "RevisionConflict", "{stale}");
    Ok(())
}

fn sound(scratch: &Path, package: &Path, edit: Value, dry_run: bool) -> Output {
    let request = write(
        scratch,
        "sound.json",
        &json!({"protocol":1,"expected_revision":head(package).unwrap(),"edit":edit}),
    );
    let mut arguments = vec!["sound", text(package), "--json", text(&request)];
    if dry_run {
        arguments.push("--dry-run");
    }
    cli(&arguments)
}

#[test]
fn placed_sound_edits_derive_the_native_events() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = project(scratch.path())?;
    let place = json!({"type":"place","asset":"boing","at":{"frame":2},"id":"bang"});

    let preview = parsed(&sound(scratch.path(), &package, place.clone(), true));
    assert_eq!(preview["committed"], false, "{preview}");
    assert_eq!(preview["sound_id"], "bang");
    let placed = parsed(&sound(scratch.path(), &package, place.clone(), false));
    assert_eq!(placed["committed"], true, "{placed}");
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let id = deadpan_core::SoundId::new("bang")?;
    let event = document.sounds()[&id].clone();
    // Exactly the event the native `,s` derives at that Edit frame.
    let receipt = store.source_qualification(
        document.assets()[&deadpan_core::AssetId::new("boing")?]
            .source_qualification
            .as_ref()
            .unwrap(),
    )?;
    let plan = deadpan_plan::RenderPlan::compile(&document)?;
    let at = document
        .presentation_basis()
        .frame_rate
        .audio_boundary(deadpan_core::ProjectFrame(2))?;
    assert_eq!(
        deadpan_cli::sound_events::placement(
            &document,
            &plan,
            &receipt,
            &deadpan_core::AssetId::new("boing")?,
            at
        )?,
        event
    );
    drop(store);
    // Placing the same identity again is refused.
    let again = parsed(&sound(scratch.path(), &package, place, false));
    assert_eq!(again["error"]["code"], "SoundEditRefused", "{again}");

    let gained = parsed(&sound(
        scratch.path(),
        &package,
        json!({"type":"set","id":"bang","gain_step_millidecibels":-3000,"edges":"hard"}),
        false,
    ));
    assert_eq!(gained["committed"], true, "{gained}");
    let nudged = parsed(&sound(
        scratch.path(),
        &package,
        json!({"type":"nudge","id":"bang","frames":3}),
        false,
    ));
    assert_eq!(nudged["committed"], true, "{nudged}");
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let event = &document.sounds()[&id];
    assert_eq!(event.gain_millidecibels, -3000);
    assert_eq!(event.start_edge, deadpan_core::AudioEdgePolicy::Hard);
    assert_eq!(event.end_edge, deadpan_core::AudioEdgePolicy::Hard);

    let cut = parsed(&sound(
        scratch.path(),
        &package,
        json!({"type":"cut","id":"bang","at":10}),
        false,
    ));
    assert_eq!(cut["committed"], true, "{cut}");
    let both = parsed(&sound(
        scratch.path(),
        &package,
        json!({"type":"set","id":"bang","gain_millidecibels":0,"gain_step_millidecibels":1}),
        false,
    ));
    assert_eq!(both["error"]["code"], "SoundEditRefused");
    let deleted = parsed(&sound(
        scratch.path(),
        &package,
        json!({"type":"delete","id":"bang"}),
        false,
    ));
    assert_eq!(deleted["committed"], true, "{deleted}");
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(document.sounds().is_empty());

    // A stale revision is refused before anything is derived.
    let request = write(
        scratch.path(),
        "stale.json",
        &json!({"protocol":1,"expected_revision":"not-the-head",
            "edit":{"type":"delete","id":"bang"}}),
    );
    let stale = failure(&["sound", text(&package), "--json", text(&request)]);
    assert_eq!(stale["error"]["code"], "RevisionConflict", "{stale}");

    // The bundled sting, byte for byte, for retain-original/register-source.
    let sting = scratch.path().join("sting.wav");
    let written = success(&["sound", "--write-sting", text(&sting)]);
    assert_eq!(written["written"], true);
    assert_eq!(fs::read(&sting)?, deadpan_audio::triumphant_sting_wav());
    assert_eq!(
        success(&["sound", "--write-sting", text(&sting)])["written"],
        false
    );
    fs::write(&sting, b"other")?;
    assert!(
        !cli(&["sound", "--write-sting", text(&sting)])
            .status
            .success()
    );
    Ok(())
}

/// The open-project route: the same commands, served by the owner's writer.
#[test]
fn open_project_commands_run_on_the_owner_writer() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = project(scratch.path())?;
    let path = text(&package);
    let backup = success(&["project", "backup", path]);
    let backup_id = backup["created"]["backup"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let initial = head(&package)?;
    let mut owner = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    owner.set_generation_context_resolver(std::sync::Arc::new(
        deadpan_cli::generation_context::BoundaryContextResolver::default(),
    ));

    // Without an endpoint, writes report the unreachable owner.
    let request = write(
        scratch.path(),
        "yank.json",
        &yank(&package, "o", [1, 4], 0)?,
    );
    let refused = failure(&["macro", path, "--json", text(&request)]);
    assert_eq!(refused["error"]["code"], "HostOwnerUnavailable");

    let mut endpoint = Endpoint::bind(&mut owner)?;
    let (output, executed) = live_owner::serve(&mut owner, &mut endpoint, || {
        cli(&["macro", path, "--json", text(&request)])
    });
    let saved = parsed(&output);
    assert_eq!(saved["committed_registers"]["bank_version"], 1, "{saved}");
    assert_eq!(executed, 1);

    let (output, _) = live_owner::serve(&mut owner, &mut endpoint, || {
        sound(
            scratch.path(),
            &package,
            json!({"type":"place","asset":"boing","at":{"sample":4800}}),
            false,
        )
    });
    let placed = parsed(&output);
    assert_eq!(placed["committed"], true, "{placed}");
    assert_eq!(owner.snapshot()?.sounds().len(), 1);

    for flag in ["--clean", "--confirm-clock"] {
        let (output, _) = live_owner::serve(&mut owner, &mut endpoint, || {
            cli(&["project", "storage", path, flag])
        });
        let report = parsed(&output);
        assert!(output.status.success(), "{report}");
        assert_eq!(report["variant_expiry"]["dry_run"], false, "{report}");
        if flag == "--clean" {
            assert_eq!(report["cleanup"]["dry_run"], false, "{report}");
        }
    }
    // Dry runs read beside the owner and never contact it.
    let (output, executed) = live_owner::serve(&mut owner, &mut endpoint, || {
        cli(&["project", "storage", path, "--clean", "--dry-run"])
    });
    assert!(output.status.success());
    assert_eq!(parsed(&output)["cleanup"]["dry_run"], true);
    assert_eq!(executed, 0);

    // A stale expected revision refuses the restore on the owner.
    let (output, _) = live_owner::serve(&mut owner, &mut endpoint, || {
        cli(&[
            "project",
            "restore",
            path,
            &backup_id,
            "--expected",
            "not-the-head",
        ])
    });
    assert_eq!(parsed(&output)["error"]["code"], "RevisionConflict");
    let current = head(&package)?;
    let (output, executed) = live_owner::serve(&mut owner, &mut endpoint, || {
        cli(&[
            "project",
            "restore",
            path,
            &backup_id,
            "--expected",
            &current,
        ])
    });
    let restored = parsed(&output);
    assert!(output.status.success(), "{restored}");
    assert_eq!(executed, 1);
    assert_eq!(restored["restored"]["restored"]["revision_id"], initial);
    assert_eq!(owner.head_revision()?.as_str(), initial);
    assert!(owner.snapshot()?.sounds().is_empty());
    Ok(())
}

#[test]
fn reports_list_ai_variants_and_gag_steps_and_refuse_unknown_attempts() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = project(scratch.path())?;
    let path = text(&package);
    let revision = head(&package)?;

    let variants = success(&["ai-variants", path, "--joins"]);
    assert_eq!(variants["pauses"], json!([]), "{variants}");
    assert_eq!(variants["interrupted"], json!([]));
    assert_eq!(variants["retention_seconds"], 7 * 24 * 60 * 60);
    let missing = failure(&["ai-variants", path, "--hold", "no-such-pause"]);
    assert_eq!(missing["error"]["code"], "GenerationVariantUnavailable");
    for verb in ["select-hold", "discard-hold", "keep-hold"] {
        let refused = failure(&[verb, path, "--request", "r", "--attempt", "a"]);
        assert_eq!(
            refused["error"]["code"], "GenerationVariantUnavailable",
            "{verb}: {refused}"
        );
    }
    let dismissed = failure(&["dismiss-attempt", path, "--request", "r", "--attempt", "a"]);
    assert!(dismissed["error"]["code"].is_string(), "{dismissed}");

    // :gag-inspect's steps for an explicit recipe, at the project's rate.
    let recipe = write(
        scratch.path(),
        "gag.json",
        &json!({"recipe":"long_answer","version":deadpan_core::GAG_RECIPE_VERSION,
            "pause":{"unit":"milliseconds","milliseconds":1500},
            "scale":serde_json::to_value(deadpan_core::ExactRatio::new(27, 20)?)?}),
    );
    let inspected = success(&["gag-inspect", path, "--json", text(&recipe)]);
    assert_eq!(inspected["name"], "The Long Answer", "{inspected}");
    let steps = inspected["steps"].as_array().unwrap();
    assert_eq!(
        steps.len(),
        inspected["instructions"].as_array().unwrap().len()
    );
    assert!(steps[0].as_str().unwrap().contains("1500ms"), "{inspected}");

    // Files-only cleanup leaves AI variants alone, like Storage P/R.
    let files = success(&["project", "storage", path, "--clean", "--files-only"]);
    assert_eq!(
        files["variant_expiry"]["status"], "not_requested",
        "{files}"
    );
    assert_eq!(files["cleanup"]["dry_run"], false);
    assert_eq!(head(&package)?, revision, "nothing here is an edit");

    // Storage P then R: a dry run's plan removes exactly its files, bound
    // to the project state it was previewed in.
    let preview = cli(&[
        "project",
        "storage",
        path,
        "--clean",
        "--files-only",
        "--dry-run",
    ]);
    assert!(preview.status.success());
    let plan_file = scratch.path().join("plan.json");
    fs::write(&plan_file, &preview.stdout)?;
    let planned = parsed(&preview);
    assert_eq!(
        planned["plan"]["revision_id"],
        revision.as_str(),
        "{planned}"
    );
    assert!(planned["plan_hash"].is_string());
    let removed = success(&[
        "project",
        "storage",
        path,
        "--clean",
        "--files-only",
        "--plan",
        text(&plan_file),
    ]);
    assert_eq!(removed["cleanup"]["dry_run"], false, "{removed}");
    // A plan needs --files-only, and a tampered plan is refused.
    assert!(
        !cli(&[
            "project",
            "storage",
            path,
            "--clean",
            "--plan",
            text(&plan_file)
        ])
        .status
        .success()
    );
    let mut tampered = planned.clone();
    tampered["plan"]["grace_seconds"] = json!(1);
    let tampered_file = write(scratch.path(), "tampered.json", &tampered);
    let refused = failure(&[
        "project",
        "storage",
        path,
        "--clean",
        "--files-only",
        "--plan",
        text(&tampered_file),
    ]);
    assert_eq!(refused["error"]["code"], "StoragePlanInvalid", "{refused}");
    // After an edit the plan is stale.
    let recipe_request = write(
        scratch.path(),
        "edit.json",
        &json!({"protocol":1,"expected_revision":head(&package)?,
            "edit":{"type":"place","asset":"boing","at":{"frame":0}}}),
    );
    success(&["sound", path, "--json", text(&recipe_request)]);
    let stale = failure(&[
        "project",
        "storage",
        path,
        "--clean",
        "--files-only",
        "--plan",
        text(&plan_file),
    ]);
    assert_eq!(stale["error"]["code"], "StoragePlanStale", "{stale}");
    Ok(())
}
