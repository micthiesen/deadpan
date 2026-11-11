use super::*;
use deadpan_models::packs::{Download, PackError};
use deadpan_models::updates::generate_key;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;

/// Serves fixed bytes per URL and counts requests.
struct Releases(HashMap<String, Vec<u8>>, Mutex<u32>);

impl Transport for Releases {
    fn fetch(&self, url: &str, offset: u64) -> Result<Download, PackError> {
        *self.1.lock().unwrap() += 1;
        let bytes = self
            .0
            .get(url)
            .ok_or_else(|| PackError::Transport(format!("no {url}")))?;
        Ok(Download {
            offset,
            body: Box::new(io::Cursor::new(bytes[offset as usize..].to_vec())),
        })
    }
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Fixture {
    root: tempfile::TempDir,
    pkcs8: Vec<u8>,
    keys: Vec<TrustedKey>,
    transport: Releases,
}

/// A real Apple-signed Mach-O stands in for both helpers so code-signature
/// and content-pin checks run for real; `variant` appends bytes to change the
/// hash of one release.
fn release(name: &str, version: &str, variant: u8) -> (HelperRelease, Vec<u8>) {
    let mut bytes = fs::read("/usr/bin/true").unwrap();
    if variant != 0 {
        // Different bytes but the same code: a re-signed copy.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("copy");
        fs::write(&path, &bytes).unwrap();
        for arguments in [
            vec!["--remove-signature"],
            vec![
                "--force",
                "-s",
                "-",
                "--identifier",
                if variant == 1 { "one" } else { "two" },
            ],
        ] {
            assert!(
                std::process::Command::new("/usr/bin/codesign")
                    .args(arguments)
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        bytes = fs::read(&path).unwrap();
    }
    let content = super::super::macho_content::content_sha256(&bytes).unwrap();
    let release = HelperRelease {
        name: name.into(),
        version: version.into(),
        license: "MIT".into(),
        url: format!(
            "https://github.com/example/{name}/releases/download/{version}/{name}-{variant}"
        ),
        download_sha256: sha(&bytes),
        download_bytes: bytes.len() as u64,
        packaging: ReleasePackaging::Executable,
        executable: name.replace('-', "_"),
        executable_sha256: sha(&bytes),
        executable_bytes: bytes.len() as u64,
        content_sha256: (name == "yt-dlp").then_some(content),
        // Apple's signature, or the ad hoc identifier of a re-signed copy.
        signer: (name == "deno").then(|| match variant {
            0 => "anchor apple".to_owned(),
            1 => "identifier \"one\"".to_owned(),
            _ => "identifier \"two\"".to_owned(),
        }),
    };
    (release, bytes)
}

fn fixture() -> Fixture {
    let (pkcs8, public_key) = generate_key().unwrap();
    Fixture {
        root: tempfile::tempdir().unwrap(),
        pkcs8,
        keys: vec![TrustedKey {
            id: "test-key".into(),
            public_key,
        }],
        transport: Releases(HashMap::new(), Mutex::new(0)),
    }
}

impl Fixture {
    fn manifest(
        &mut self,
        serial: u64,
        yt_dlp: (&str, u8),
        deno: (&str, u8),
    ) -> DownloaderManifest {
        let mut helpers = Vec::new();
        for (name, (version, variant)) in [("yt-dlp", yt_dlp), ("deno", deno)] {
            let (release, bytes) = release(name, version, variant);
            self.transport.0.insert(release.url.clone(), bytes);
            helpers.push(release);
        }
        DownloaderManifest {
            schema: MANIFEST_SCHEMA,
            serial,
            issued: "2026-10-05".into(),
            min_app_version: "0.1.0".into(),
            platform: PLATFORM.into(),
            ejs_version: "0.9.0".into(),
            helpers,
            notes: None,
        }
    }

    fn sign(&self, manifest: &DownloaderManifest) -> Vec<u8> {
        SignedManifest::sign(
            UpdateKind::Downloader,
            serde_json::to_string(manifest).unwrap(),
            "test-key",
            &self.pkcs8,
        )
        .unwrap()
        .to_bytes()
    }

    fn update(
        &self,
        signed: &[u8],
        allow_downgrade: bool,
        matches: bool,
    ) -> Result<UpdateOutcome, CliError> {
        update(
            self.root.path(),
            signed,
            UpdateOptions {
                keys: &self.keys,
                allow_downgrade,
            },
            &self.transport,
            &AtomicBool::new(false),
            &|helpers: &Helpers| {
                Ok(ProbeReport {
                    yt_dlp: Some(format!("stable@{}", helpers.yt_dlp_version)),
                    ejs: Some(helpers.ejs_version.clone()),
                    js_runtimes: Some(format!("deno-{}", helpers.deno_version)),
                    deno: Some(format!("deno {} (stable)", helpers.deno_version)),
                    matches_pins: matches,
                })
            },
            &mut |_| {},
        )
    }

    fn baseline(&self) -> HelperSource {
        HelperSource::Managed(self.root.path().to_owned())
    }

    fn selection(&self) -> Result<Selection, CliError> {
        select_with_keys(self.root.path(), self.baseline(), &self.keys)
    }
}

fn code(result: Result<impl std::fmt::Debug, CliError>) -> &'static str {
    match result {
        Err(CliError::Import(error)) => error.code,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_signed_update_installs_probes_activates_and_rolls_back() {
    let mut fixture = fixture();
    let manifest = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    let signed = fixture.sign(&manifest);
    let outcome = fixture.update(&signed, false, true).unwrap();
    assert_eq!(outcome.serial, 1);
    assert_eq!(outcome.previous, Selected::Baseline);
    // Versioned directories, the retained envelope and the state file.
    let root = &fixture.root.path().to_owned();
    assert!(root.join("yt-dlp/2026.09.30/yt_dlp").is_file());
    assert!(root.join("deno/2.9.7/deno").is_file());
    assert_eq!(
        fs::read(root.join("updates/manifests/1.json")).unwrap(),
        signed
    );
    let selection = fixture.selection().unwrap();
    assert_eq!(selection.note, None);
    assert_eq!(selection.source.kind(), "update");
    let helpers = Helpers::resolve_source(&selection.source).unwrap();
    assert_eq!(helpers.yt_dlp_version, "2026.09.30");
    assert_eq!(helpers.ejs_version, "0.9.0");
    helpers.recheck().unwrap();

    // A second update keeps the first as the previous selection.
    let newer = fixture.manifest(2, ("2026.10.04", 0), ("2.9.8", 1));
    fixture.update(&fixture.sign(&newer), false, true).unwrap();
    let state = state(root).unwrap().unwrap();
    assert_eq!(state.active, Selected::Update { serial: 2 });
    assert_eq!(state.previous, Some(Selected::Update { serial: 1 }));
    assert_eq!(state.highest_serial, 2);

    // Rollback restores serial 1; the files of both stay installed.
    let rolled = rollback(root, &fixture.baseline(), &fixture.keys, false)
        .unwrap()
        .state;
    assert_eq!(rolled.active, Selected::Update { serial: 1 });
    assert_eq!(rolled.previous, Some(Selected::Update { serial: 2 }));
    assert!(root.join("deno/2.9.8/deno").is_file());
    let helpers = Helpers::resolve_source(&fixture.selection().unwrap().source).unwrap();
    assert_eq!(helpers.deno_version, "2.9.7");

    // Rolling back to the baseline needs a verified baseline install.
    assert_eq!(
        code(rollback(root, &fixture.baseline(), &fixture.keys, true)),
        "DownloaderNotInstalled"
    );
    assert!(!YT_DLP.directory(root).exists());
    // Re-applying the older serial is a replay even though it is retained;
    // rollback is the way back. With explicit permission it reactivates
    // without downloading anything.
    assert_eq!(
        code(fixture.update(&signed, false, true)),
        "UpdateDowngrade"
    );
    let requests = *fixture.transport.1.lock().unwrap();
    fixture.update(&signed, true, true).unwrap();
    assert_eq!(*fixture.transport.1.lock().unwrap(), requests);
}

#[test]
fn a_failed_probe_leaves_the_previous_selection_active() {
    let mut fixture = fixture();
    let manifest = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    assert_eq!(
        code(fixture.update(&fixture.sign(&manifest), false, false)),
        "UpdateProbeFailed"
    );
    assert_eq!(state(fixture.root.path()).unwrap(), None);
    assert_eq!(fixture.selection().unwrap().source.kind(), "managed");
    // The envelope was not retained, so a corrected manifest with the same
    // serial can still be applied.
    assert!(
        !fixture
            .root
            .path()
            .join("updates/manifests/1.json")
            .exists()
    );
    let corrected = fixture.manifest(1, ("2026.09.30", 0), ("2.9.8", 1));
    fixture
        .update(&fixture.sign(&corrected), false, true)
        .unwrap();
}

#[test]
fn downgrades_and_replays_need_explicit_permission() {
    let mut fixture = fixture();
    let old = fixture.manifest(1, ("2026.01.01", 0), ("2.9.7", 0));
    let old_signed = fixture.sign(&old);
    assert_eq!(
        code(fixture.update(&old_signed, false, true)),
        "UpdateDowngrade"
    );
    assert!(!fixture.root.path().join("yt-dlp").exists());
    fixture.update(&old_signed, true, true).unwrap();
    // The explicit acceptance holds against this build's baseline.
    let selection = fixture.selection().unwrap();
    assert_eq!(selection.source.kind(), "update");
    // Against another baseline it would be superseded.
    let mut state = state(fixture.root.path()).unwrap().unwrap();
    state.below_baseline_accepted = Some(vec!["yt-dlp 2020.01.01".into()]);
    write_state(fixture.root.path(), &state).unwrap();
    let selection = fixture.selection().unwrap();
    assert_eq!(selection.source.kind(), "managed");
    assert!(selection.note.unwrap().contains("baseline is newer"));

    let newer = fixture.manifest(5, ("2026.10.01", 0), ("2.9.7", 0));
    fixture.update(&fixture.sign(&newer), false, true).unwrap();
    let replay = fixture.manifest(3, ("2026.10.02", 0), ("2.9.7", 0));
    assert_eq!(
        code(fixture.update(&fixture.sign(&replay), false, true)),
        "UpdateDowngrade"
    );
    // A different envelope reusing an installed serial.
    let conflict = fixture.manifest(5, ("2026.10.03", 0), ("2.9.7", 0));
    assert_eq!(
        code(fixture.update(&fixture.sign(&conflict), false, true)),
        "UpdateManifestInvalid"
    );
}

#[test]
fn integrity_failures_refuse_and_incompatibility_falls_back() {
    let mut fixture = fixture();
    let manifest = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    fixture
        .update(&fixture.sign(&manifest), false, true)
        .unwrap();
    let root = &fixture.root.path().to_owned();

    // A key this build no longer trusts: the baseline, with the reason.
    let selection = select_with_keys(root, fixture.baseline(), &[]).unwrap();
    assert_eq!(selection.source.kind(), "managed");
    assert!(selection.note.unwrap().contains("no longer trusts"));

    // A changed executable is an integrity failure, not a fallback.
    let executable = root.join("yt-dlp/2026.09.30/yt_dlp");
    let original = fs::read(&executable).unwrap();
    let mut tampered = original.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    fs::write(&executable, &tampered).unwrap();
    let source = fixture.selection().unwrap().source;
    assert_eq!(
        code(Helpers::resolve_source(&source)),
        "DownloaderHelperInvalid"
    );
    fs::write(&executable, &original).unwrap();
    Helpers::resolve_source(&source).unwrap();

    // A retained envelope edited under a trusted key.
    let retained = root.join("updates/manifests/1.json");
    let text = fs::read_to_string(&retained).unwrap();
    fs::write(&retained, text.replace("2026.09.30", "2026.09.31")).unwrap();
    assert_eq!(code(fixture.selection()), "DownloaderHelperInvalid");
    fs::remove_file(&retained).unwrap();
    assert_eq!(code(fixture.selection()), "DownloaderHelperInvalid");
}

#[test]
fn incompatible_or_malformed_manifests_never_install() {
    let mut fixture = fixture();
    let mut future = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    future.min_app_version = "999.0".into();
    assert_eq!(
        code(fixture.update(&fixture.sign(&future), false, true)),
        "UpdateIncompatible"
    );
    let base = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    let mut cases: Vec<DownloaderManifest> = Vec::new();
    let mut insecure = base.clone();
    insecure.helpers[0].url = insecure.helpers[0].url.replace("https://", "http://");
    cases.push(insecure);
    let mut unpinned = base.clone();
    unpinned.helpers[0].content_sha256 = None;
    cases.push(unpinned);
    let mut unsigned = base.clone();
    unsigned.helpers[1].signer = None;
    cases.push(unsigned);
    let mut reordered = base.clone();
    reordered.helpers.reverse();
    cases.push(reordered);
    let mut traversal = base.clone();
    traversal.helpers[0].version = "../x".into();
    cases.push(traversal);
    let mut platform = base.clone();
    platform.platform = "linux-x86_64".into();
    for case in cases {
        assert_eq!(
            code(fixture.update(&fixture.sign(&case), false, true)),
            "UpdateManifestInvalid",
            "{case:?}"
        );
    }
    assert_eq!(
        code(fixture.update(&fixture.sign(&platform), false, true)),
        "UpdateIncompatible"
    );
    // An untrusted signature and a pack-kind envelope.
    let signed = fixture.sign(&base);
    let keys = std::mem::take(&mut fixture.keys);
    assert_eq!(
        code(fixture.update(&signed, false, true)),
        "UpdateUntrusted"
    );
    fixture.keys = keys;
    let pack = SignedManifest::sign(
        UpdateKind::ModelPack,
        serde_json::to_string(&base).unwrap(),
        "test-key",
        &fixture.pkcs8,
    )
    .unwrap();
    assert_eq!(
        code(fixture.update(&pack.to_bytes(), false, true)),
        "UpdateSignatureInvalid"
    );
    // A release whose bytes differ from the manifest never publishes.
    let mut wrong = base;
    wrong.helpers[0].executable_sha256 = sha(b"other");
    wrong.helpers[0].download_sha256 = sha(b"other");
    assert_eq!(
        code(fixture.update(&fixture.sign(&wrong), false, true)),
        "DownloaderInstallFailed"
    );
    assert!(!fixture.root.path().join("yt-dlp/2026.09.30").exists());
    assert_eq!(state(fixture.root.path()).unwrap(), None);
}

#[test]
fn a_content_pin_mismatch_refuses_installation() {
    let mut fixture = fixture();
    let mut manifest = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    manifest.helpers[0].content_sha256 = Some(sha(b"some other code"));
    assert_eq!(
        code(fixture.update(&fixture.sign(&manifest), false, true)),
        "DownloaderInstallFailed"
    );
    assert!(!fixture.root.path().join("yt-dlp/2026.09.30").exists());
}

#[test]
fn retained_serials_bound_replays_without_state() {
    let mut fixture = fixture();
    let first = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    let first_signed = fixture.sign(&first);
    fixture.update(&first_signed, false, true).unwrap();
    let second = fixture.manifest(2, ("2026.10.04", 0), ("2.9.7", 0));
    fixture.update(&fixture.sign(&second), false, true).unwrap();
    let root = fixture.root.path().to_owned();
    assert_eq!(retained_floor(&root, &fixture.keys), 2);

    // Deleting state.json does not let the older envelope back in.
    fs::remove_file(root.join("updates/state.json")).unwrap();
    assert_eq!(
        code(fixture.update(&first_signed, false, true)),
        "UpdateDowngrade"
    );
    // Editing state.json to select it is treated as a replay.
    let mut edited = UpdateState::initial();
    edited.active = Selected::Update { serial: 1 };
    edited.highest_serial = 1;
    write_state(&root, &edited).unwrap();
    let selection = fixture.selection().unwrap();
    assert_eq!(selection.source.kind(), "managed");
    assert!(selection.note.unwrap().contains("replay"));

    // An explicit rollback choice is honored.
    edited.active = Selected::Update { serial: 2 };
    edited.previous = Some(Selected::Update { serial: 1 });
    write_state(&root, &edited).unwrap();
    let rolled = rollback(&root, &fixture.baseline(), &fixture.keys, false).unwrap();
    assert_eq!(rolled.state.older_serial_accepted, Some(1));
    assert_eq!(rolled.state.highest_serial, 2);
    assert_eq!(fixture.selection().unwrap().source.kind(), "update");
}

#[test]
fn rollback_to_the_baseline_recovers_an_unreadable_state() {
    // A verified helper set elsewhere stands in for the baseline.
    let mut other = fixture();
    let manifest = other.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    other.update(&other.sign(&manifest), false, true).unwrap();
    let baseline = other.selection().unwrap().source;

    let mut fixture = fixture();
    let manifest = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    fixture
        .update(&fixture.sign(&manifest), false, true)
        .unwrap();
    let root = fixture.root.path().to_owned();
    fs::write(root.join("updates/state.json"), b"{torn").unwrap();
    assert_eq!(code(fixture.selection()), "DownloaderHelperInvalid");
    assert_eq!(
        code(rollback(&root, &baseline, &fixture.keys, false)),
        "DownloaderHelperInvalid"
    );
    let outcome = rollback(&root, &baseline, &fixture.keys, true).unwrap();
    assert!(outcome.recovered.unwrap().contains("state"));
    assert_eq!(outcome.state.active, Selected::Baseline);
    assert_eq!(outcome.state.highest_serial, 1);
    assert_eq!(fixture.selection().unwrap().source.kind(), "managed");
}

#[test]
fn torn_envelopes_are_replaced_and_shared_directories_refused() {
    use std::os::unix::fs::PermissionsExt;
    let mut fixture = fixture();
    let manifest = fixture.manifest(1, ("2026.09.30", 0), ("2.9.7", 0));
    let signed = fixture.sign(&manifest);
    let root = fixture.root.path().to_owned();
    let retained = root.join("updates/manifests/1.json");
    fs::create_dir_all(retained.parent().unwrap()).unwrap();
    fs::set_permissions(root.join("updates"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(
        retained.parent().unwrap(),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    fs::write(&retained, &signed[..10]).unwrap();
    fixture.update(&signed, false, true).unwrap();
    assert_eq!(fs::read(&retained).unwrap(), signed);
    fs::set_permissions(root.join("updates"), fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(code(fixture.selection()), "DownloaderHelperInvalid");
    fs::set_permissions(root.join("updates"), fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(fixture.selection().unwrap().source.kind(), "update");
}
