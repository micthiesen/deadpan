use super::*;
use crate::packs::{Download, InstallProgress, Operation, PackFile, Transport, approved_packs};
use crate::updates::generate_key;
use sha2::Digest;
use std::path::Path;
use std::sync::atomic::AtomicBool;

struct Bytes(Vec<u8>);

impl Transport for Bytes {
    fn fetch(&self, _: &str, offset: u64) -> Result<Download, PackError> {
        Ok(Download {
            offset,
            body: Box::new(io::Cursor::new(self.0[offset as usize..].to_vec())),
        })
    }
}

fn plenty(_: &Path) -> io::Result<u64> {
    Ok(u64::MAX)
}

fn key() -> (Vec<u8>, TrustedKey) {
    let (pkcs8, public_key) = generate_key().unwrap();
    (
        pkcs8,
        TrustedKey {
            id: "test-key".into(),
            public_key,
        },
    )
}

/// A whisper-family pack of one small file at `version`.
fn whisper(version: &str, bytes: &[u8]) -> PackManifest {
    let mut manifest = approved_pack("whisper-base-en").unwrap();
    manifest.pack_version = version.into();
    manifest.files = vec![PackFile {
        license: None,
        name: "ggml-test.bin".into(),
        url: format!("https://huggingface.co/example/{version}/ggml-test.bin"),
        sha256: sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        bytes: bytes.len() as u64,
    }];
    manifest
}

/// A signed bridge data update that keeps the pinned component inventory and
/// changes only its revision directories and pack identity.
fn bridge_pack(version: &str, ltx_revision: &str, gemma_revision: &str) -> PackManifest {
    let mut pack = approved_pack("ltx-2.3-q4-bridge").unwrap();
    pack.pack_version = version.into();
    for file in &mut pack.files {
        let mut parts = file.name.split('/');
        let component = parts.next().unwrap();
        let old_revision = parts.next().unwrap();
        let rest = parts.next().unwrap();
        let revision = if component == "mlx_ltx_q4_pack" {
            ltx_revision
        } else {
            gemma_revision
        };
        file.url = file.url.replace(old_revision, revision);
        file.name = format!("{component}/{revision}/{rest}");
    }
    pack
}

fn signed(pkcs8: &[u8], serial: u64, pack: PackManifest) -> Vec<u8> {
    let update = PackUpdate {
        schema: UPDATE_SCHEMA,
        serial,
        issued: "2026-10-05".into(),
        min_app_version: "0.1.0".into(),
        pack,
    };
    SignedManifest::sign(
        UpdateKind::ModelPack,
        serde_json::to_string(&update).unwrap(),
        "test-key",
        pkcs8,
    )
    .unwrap()
    .to_bytes()
}

/// Stage and activate a version as the host does after its smoke test.
fn install(store: &PackStore, manifest: &PackManifest, bytes: &[u8]) -> InstalledPack {
    let staged = store
        .stage(
            manifest,
            &[],
            &Bytes(bytes.to_vec()),
            plenty,
            &AtomicBool::new(false),
            |_: InstallProgress| {},
        )
        .unwrap();
    store.activate(staged).unwrap()
}

#[test]
fn pointer_directory_sync_failure_reports_the_committed_selection() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let old = ActivePointer {
        schema: POINTER_SCHEMA,
        version: "1".into(),
        previous: None,
    };
    store.write_pointer("whisper-base-en", &old).unwrap();
    let next = ActivePointer {
        schema: POINTER_SCHEMA,
        version: "2".into(),
        previous: Some("1".into()),
    };
    let error = store
        .write_pointer_with_sync("whisper-base-en", &next, |_| {
            Err(io::Error::other("injected directory sync failure"))
        })
        .unwrap_err();
    assert!(matches!(
        &error,
        PackError::Update {
            code: "ModelPackSelectionDurabilityUncertain",
            message,
        } if message.contains("selection is now version 2")
            && message.contains("crash durability is uncertain")
    ));
    assert_eq!(store.pointer("whisper-base-en").unwrap(), Some(next));
}

#[test]
fn a_signed_update_installs_side_by_side_activates_and_rolls_back() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let (pkcs8, trusted) = key();
    let keys = [trusted];
    let baseline = approved_pack("whisper-base-en").unwrap();
    // Without a pointer the compiled version is selected.
    assert_eq!(
        store.selected_with_keys("whisper-base-en", &keys).unwrap(),
        Some(baseline.clone())
    );

    let old_bytes = b"version two weights".to_vec();
    let two = whisper("2.5", &old_bytes);
    let two_signed = signed(&pkcs8, 1, two.clone());
    let admitted = store.admit_update(&two_signed, &keys, &[], false).unwrap();
    assert_eq!(admitted, two);
    // Admission alone selects nothing.
    assert_eq!(
        store.selected_with_keys("whisper-base-en", &keys).unwrap(),
        Some(baseline.clone())
    );
    // Activation needs the installed (smoke-tested) version.
    assert!(matches!(
        store.activate_update(&two_signed, &keys, &[], false),
        Err(PackError::Update {
            code: "ModelPackNotInstalled",
            ..
        })
    ));
    assert!(!root.path().join(".updates").exists());
    install(&store, &two, &old_bytes);
    store
        .activate_update(&two_signed, &keys, &[], false)
        .unwrap();
    assert_eq!(
        store
            .current_with_keys("whisper-base-en", &keys)
            .unwrap()
            .unwrap()
            .manifest,
        two
    );

    let new_bytes = b"version three weights".to_vec();
    let three = whisper("3", &new_bytes);
    let three_signed = signed(&pkcs8, 2, three.clone());
    store
        .admit_update(&three_signed, &keys, &[], false)
        .unwrap();
    // Until the new version is installed and selected, 2.5 stays current.
    assert_eq!(
        store
            .current_with_keys("whisper-base-en", &keys)
            .unwrap()
            .unwrap()
            .manifest,
        two
    );
    install(&store, &three, &new_bytes);
    store
        .activate_update(&three_signed, &keys, &[], false)
        .unwrap();
    let pointer = store.pointer("whisper-base-en").unwrap().unwrap();
    assert_eq!(pointer.version, "3");
    assert_eq!(pointer.previous.as_deref(), Some("2.5"));
    // Both versions remain installed side by side.
    assert!(store.installed(&two).unwrap().is_some());
    assert!(store.installed(&three).unwrap().is_some());
    // The active version cannot be removed out from under consumers.
    assert!(matches!(
        store.remove(&three),
        Err(PackError::Update {
            code: "ModelPackActive",
            ..
        })
    ));

    let rolled = store.rollback_with_keys("whisper-base-en", &keys).unwrap();
    assert_eq!(rolled.manifest, two);
    assert_eq!(
        store
            .current_with_keys("whisper-base-en", &keys)
            .unwrap()
            .unwrap()
            .manifest,
        two
    );
    // Rolling back again returns to 3 (the pointer swaps).
    assert_eq!(
        store
            .rollback_with_keys("whisper-base-en", &keys)
            .unwrap()
            .manifest,
        three
    );

    // A previous version that disappeared cannot be rolled back to.
    store.rollback_with_keys("whisper-base-en", &keys).unwrap();
    store.select_version(&three, &keys).unwrap();
    // Re-applying the older serial is a replay; rollback is the way back.
    assert!(matches!(
        store.admit_update(&two_signed, &keys, &[], false),
        Err(PackError::Update {
            code: "UpdateDowngrade",
            ..
        })
    ));
    fs::remove_dir_all(root.path().join("whisper-base-en/2.5")).unwrap();
    assert!(matches!(
        store.rollback_with_keys("whisper-base-en", &keys),
        Err(PackError::Update {
            code: "ModelPackNotInstalled",
            ..
        })
    ));
}

#[test]
fn a_damaged_selected_version_falls_back_to_the_compiled_pack() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let (pkcs8, trusted) = key();
    let keys = [trusted];
    let bytes = b"weights".to_vec();
    let three = whisper("3", &bytes);
    let three_signed = signed(&pkcs8, 1, three.clone());
    store
        .admit_update(&three_signed, &keys, &[], false)
        .unwrap();
    install(&store, &three, &bytes);
    store
        .activate_update(&three_signed, &keys, &[], false)
        .unwrap();
    // An unknown version cannot be selected directly.
    assert!(store.select_version(&whisper("9", b"x"), &keys).is_err());
    // A truncated file fails `installed`, so the selection falls back to the
    // compiled manifest, which is not installed here.
    fs::write(root.path().join("whisper-base-en/3/ggml-test.bin"), b"w").unwrap();
    assert_eq!(
        store.selected_with_keys("whisper-base-en", &keys).unwrap(),
        approved_pack("whisper-base-en")
    );
    assert!(
        store
            .current_with_keys("whisper-base-en", &keys)
            .unwrap()
            .is_none()
    );
    // A key this build no longer trusts removes the update from the catalog.
    fs::write(root.path().join("whisper-base-en/3/ggml-test.bin"), &bytes).unwrap();
    assert!(
        store
            .current_with_keys("whisper-base-en", &keys)
            .unwrap()
            .is_some()
    );
    assert!(
        !store
            .catalog_with_keys(&[])
            .iter()
            .any(|manifest| manifest.pack_version == "3")
    );
    assert!(
        store
            .current_with_keys("whisper-base-en", &[])
            .unwrap()
            .is_none()
    );
}

#[test]
fn downgrades_replays_and_conflicts_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let (pkcs8, trusted) = key();
    let keys = [trusted];
    let code = |result: Result<PackManifest, PackError>| match result {
        Err(PackError::Update { code, .. }) => code,
        other => panic!("{other:?}"),
    };
    // Older than the compiled version 2.
    assert_eq!(
        code(store.admit_update(&signed(&pkcs8, 1, whisper("1", b"a")), &keys, &[], false)),
        "UpdateDowngrade"
    );
    store
        .admit_update(&signed(&pkcs8, 1, whisper("1", b"a")), &keys, &[], true)
        .unwrap();
    let four = whisper("4", b"b");
    let four_signed = signed(&pkcs8, 5, four.clone());
    store.admit_update(&four_signed, &keys, &[], false).unwrap();
    install(&store, &four, b"b");
    store
        .activate_update(&four_signed, &keys, &[], false)
        .unwrap();
    // A lower serial for a new version is a replay.
    assert_eq!(
        code(store.admit_update(&signed(&pkcs8, 3, whisper("5", b"c")), &keys, &[], false)),
        "UpdateDowngrade"
    );
    // The same version with different files.
    assert_eq!(
        code(store.admit_update(&signed(&pkcs8, 6, whisper("4", b"x")), &keys, &[], false)),
        "UpdateManifestInvalid"
    );
    // Re-applying the identical active envelope is idempotent.
    store
        .activate_update(&four_signed, &keys, &[], false)
        .unwrap_or_else(|error| panic!("{error}"));
    // A failed install never retained its envelope, so a corrected
    // manifest for the same version can still be applied.
    assert!(!root.path().join(".updates/whisper-base-en/1.json").exists());
}

#[test]
fn untrusted_tampered_and_incompatible_updates_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let (pkcs8, trusted) = key();
    let keys = [trusted];
    let code = |result: Result<PackManifest, PackError>| match result {
        Err(PackError::Update { code, .. }) => code,
        other => panic!("{other:?}"),
    };
    let good = signed(&pkcs8, 1, whisper("3", b"a"));
    assert_eq!(
        code(store.admit_update(&good, &[], &[], false)),
        "UpdateUntrusted"
    );
    let mut envelope = SignedManifest::parse(&good).unwrap();
    envelope.payload = envelope.payload.replace("\"3\"", "\"30\"");
    assert_eq!(
        code(store.admit_update(&envelope.to_bytes(), &keys, &[], false)),
        "UpdateSignatureInvalid"
    );
    // A pack family this build does not know.
    let mut unknown = whisper("3", b"a");
    unknown.pack_id = "unknown-pack".into();
    assert_eq!(
        code(store.admit_update(&signed(&pkcs8, 1, unknown), &keys, &[], false)),
        "UpdateIncompatible"
    );
    // A runtime this build does not ship.
    let mut runtime = whisper("3", b"a");
    runtime.runtime_versions = vec!["9.9.9".into()];
    assert_eq!(
        code(store.admit_update(&signed(&pkcs8, 1, runtime), &keys, &[], false)),
        "UpdateIncompatible"
    );
    // Bridge updates may replace data only within the shipped component,
    // q4 configuration and operation contract.
    let mut valid_bridge = bridge_pack("2", &"a".repeat(40), &"b".repeat(40));
    let transformer = valid_bridge
        .files
        .iter_mut()
        .find(|file| file.name.ends_with("/transformer-dev.safetensors"))
        .unwrap();
    transformer.sha256 = "e".repeat(64);
    let signed_bridge = signed(&pkcs8, 1, valid_bridge.clone());
    assert_eq!(
        store
            .admit_update(&signed_bridge, &keys, &valid_bridge.license_ids(), false)
            .unwrap(),
        valid_bridge
    );
    let mut unsupported = bridge_pack("3", &"c".repeat(40), &"d".repeat(40));
    unsupported.operations.push(Operation::Transcribe);
    unsupported.constraints.audio = approved_pack("whisper-base-en").unwrap().constraints.audio;
    unsupported
        .constraints
        .conditioning
        .push(super::super::constraints::Conditioning::MonoAudio);
    assert_eq!(
        code(store.admit_update(
            &signed(&pkcs8, 2, unsupported),
            &keys,
            &["ltx-2".into(), "gemma".into()],
            false,
        )),
        "UpdateIncompatible"
    );
    let mut changed_quantization = bridge_pack("3", &"c".repeat(40), &"d".repeat(40));
    let quantization = changed_quantization
        .files
        .iter_mut()
        .find(|file| file.name.ends_with("/quantize_config.json"))
        .unwrap();
    quantization.sha256 = "f".repeat(64);
    assert_eq!(
        code(store.admit_update(
            &signed(&pkcs8, 2, changed_quantization),
            &keys,
            &["ltx-2".into(), "gemma".into()],
            false,
        )),
        "UpdateIncompatible"
    );
    let mut changed_component = bridge_pack("3", &"c".repeat(40), &"d".repeat(40));
    changed_component.files[0].name =
        "mlx_unapproved/cccccccccccccccccccccccccccccccccccccccc/LICENSE".into();
    assert_eq!(
        code(store.admit_update(
            &signed(&pkcs8, 2, changed_component),
            &keys,
            &["ltx-2".into(), "gemma".into()],
            false,
        )),
        "UpdateIncompatible"
    );
    // A newer minimum app version.
    let update = PackUpdate {
        schema: UPDATE_SCHEMA,
        serial: 1,
        issued: "2026-10-05".into(),
        min_app_version: "999.0".into(),
        pack: whisper("3", b"a"),
    };
    let future = SignedManifest::sign(
        UpdateKind::ModelPack,
        serde_json::to_string(&update).unwrap(),
        "test-key",
        &pkcs8,
    )
    .unwrap();
    assert_eq!(
        code(store.admit_update(&future.to_bytes(), &keys, &[], false)),
        "UpdateIncompatible"
    );
    // A downloader manifest is never a pack manifest.
    let downloader = SignedManifest::sign(
        UpdateKind::Downloader,
        envelope.payload.clone(),
        "test-key",
        &pkcs8,
    )
    .unwrap();
    assert_eq!(
        code(store.admit_update(&downloader.to_bytes(), &keys, &[], false)),
        "UpdateSignatureInvalid"
    );
    assert!(!root.path().join(".updates").join("unknown-pack").exists());
    assert_eq!(approved_packs().len(), 2);
}

#[test]
fn a_valid_signature_cannot_expand_the_shipped_runtime_constraints() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let (pkcs8, trusted) = key();
    for mutate in [
        |pack: &mut PackManifest| pack.constraints.hardware.minimum_macos.major = 27,
        |pack: &mut PackManifest| {
            pack.constraints.weight_precisions = vec![super::super::constraints::Precision::Float16]
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .frame_counts
                .maximum = 105
        },
        |pack: &mut PackManifest| pack.constraints.bridge.as_mut().unwrap().width.maximum = 1024,
        |pack: &mut PackManifest| {
            pack.constraints
                .bridge
                .as_mut()
                .unwrap()
                .maximum_project_frames = 181
        },
    ] {
        let mut pack = bridge_pack("2", &"a".repeat(40), &"b".repeat(40));
        mutate(&mut pack);
        pack.validate().unwrap();
        assert!(matches!(
            store.admit_update(
                &signed(&pkcs8, 1, pack),
                std::slice::from_ref(&trusted),
                &["ltx-2".into(), "gemma".into()],
                false
            ),
            Err(PackError::Update {
                code: "UpdateIncompatible",
                ..
            })
        ));
        assert!(!root.path().join(".active").exists());
    }
}

#[test]
fn new_or_changed_licenses_need_explicit_acceptance() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let (pkcs8, trusted) = key();
    let keys = [trusted];
    // Unchanged compiled terms need nothing.
    assert!(licenses_to_accept(&whisper("3", b"a")).is_empty());
    let mut changed = whisper("3", b"a");
    changed.licenses[0].terms = "New terms that differ from the compiled pack.".into();
    assert_eq!(licenses_to_accept(&changed), ["mit"]);
    let signed = signed(&pkcs8, 1, changed);
    assert!(matches!(
        store.admit_update(&signed, &keys, &[], false),
        Err(PackError::LicenseNotAccepted { .. })
    ));
    store
        .admit_update(&signed, &keys, &["mit".into()], false)
        .unwrap();
    let mut renamed = whisper("3", b"a");
    renamed.licenses[0].id = "mit-2".into();
    assert_eq!(licenses_to_accept(&renamed), ["mit-2"]);
}

#[test]
fn torn_retained_files_are_replaced_and_notes_explain_fallbacks() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let (pkcs8, trusted) = key();
    let keys = [trusted];
    let bytes = b"weights".to_vec();
    let three = whisper("3", &bytes);
    let three_signed = signed(&pkcs8, 1, three.clone());
    install(&store, &three, &bytes);
    // An interrupted earlier write left a torn envelope.
    let retained = root.path().join(".updates/whisper-base-en/3.json");
    fs::create_dir_all(retained.parent().unwrap()).unwrap();
    fs::write(&retained, &three_signed[..20]).unwrap();
    store
        .activate_update(&three_signed, &keys, &[], false)
        .unwrap();
    assert_eq!(fs::read(&retained).unwrap(), three_signed);
    assert_eq!(
        store
            .selection_note_with_keys("whisper-base-en", &keys)
            .unwrap(),
        None
    );
    // Untrusted now: the note says why the compiled version is used.
    let note = store
        .selection_note_with_keys("whisper-base-en", &[])
        .unwrap()
        .unwrap();
    assert!(note.contains("untrusted"), "{note}");
    fs::remove_file(root.path().join("whisper-base-en/3/ggml-test.bin")).unwrap();
    let note = store
        .selection_note_with_keys("whisper-base-en", &keys)
        .unwrap()
        .unwrap();
    assert!(note.contains("files are missing"), "{note}");
}

#[test]
fn shared_pointer_directories_are_refused() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let active = root.path().join(".active");
    fs::create_dir_all(&active).unwrap();
    fs::set_permissions(&active, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(store.pointer("whisper-base-en").is_err());
    fs::set_permissions(&active, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(store.pointer("whisper-base-en").unwrap().is_none());
}

#[test]
fn concurrent_updates_and_rollbacks_never_select_a_replayed_serial() {
    use std::sync::{Arc, Barrier};
    for round in 0..20 {
        let root = tempfile::tempdir().unwrap();
        let store = PackStore::new(root.path().to_owned());
        let (pkcs8, trusted) = key();
        let keys = Arc::new([trusted]);
        let older = whisper("3", b"three");
        let newer = whisper("4", b"four");
        // Both versions finished their (smoke-tested) installs; only the
        // locked activation step races.
        install(&store, &older, b"three");
        install(&store, &newer, b"four");
        let older_signed = Arc::new(signed(&pkcs8, 1, older));
        let newer_signed = Arc::new(signed(&pkcs8, 2, newer));
        let barrier = Arc::new(Barrier::new(3));
        let spawn = |signed: Option<Arc<Vec<u8>>>| {
            let (store, keys, barrier) = (store.clone(), Arc::clone(&keys), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                match signed {
                    Some(signed) => store
                        .activate_update(&signed, keys.as_slice(), &[], false)
                        .map(|_| ()),
                    None => store
                        .rollback_with_keys("whisper-base-en", keys.as_slice())
                        .map(|_| ()),
                }
            })
        };
        let threads = [
            spawn(Some(Arc::clone(&older_signed))),
            spawn(Some(Arc::clone(&newer_signed))),
            spawn(None),
        ];
        let results: Vec<_> = threads.map(|thread| thread.join().unwrap()).into();
        // The newer serial always activates; the older one either went first
        // or was refused as a replay once serial 2 was retained.
        assert!(results[1].is_ok(), "round {round}: {:?}", results[1]);
        if let Err(error) = &results[0] {
            assert!(
                matches!(
                    error,
                    PackError::Update {
                        code: "UpdateDowngrade",
                        ..
                    }
                ),
                "round {round}: {error}"
            );
        }
        let pointer = store.pointer("whisper-base-en").unwrap().unwrap();
        // A rollback may only have run after an activation recorded a
        // previous version; it never leaves the pointer naming an unknown or
        // missing version.
        let selected = store
            .selected_with_keys("whisper-base-en", keys.as_slice())
            .unwrap()
            .unwrap();
        assert_eq!(selected.pack_version, pointer.version, "round {round}");
        assert!(store.installed(&selected).unwrap().is_some());
        if results[2].is_err() {
            assert_eq!(pointer.version, "4", "round {round}");
        }
        assert!(
            root.path()
                .join(".updates/whisper-base-en/4.json")
                .is_file(),
            "round {round}"
        );
        assert!(
            !fs::read_dir(root.path().join(".updates/whisper-base-en"))
                .unwrap()
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with(".retain")),
            "round {round}: a temporary file was left behind"
        );
    }
}
