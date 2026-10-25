//! Compile native worker and closed FFmpeg image identities into final hosts.
//!
//! The private preparation repeats the final bundle's relocation and clean
//! signing. Informational resource manifests cannot authorize an image.

use super::*;

pub(super) const PIN_ENV: &str = "DEADPAN_NATIVE_CONTENT_PINS";
pub(super) const WORKERS: [&str; 3] = [
    "deadpan-transcribe",
    "deadpan-track",
    "deadpan-media-worker",
];

pub(super) struct Pins {
    pub content_pins: BTreeMap<String, String>,
    source_workers: BTreeMap<String, String>,
}

pub(super) fn build_environment(command: &mut Command, pins: Option<&Pins>) -> Result<()> {
    // A caller's environment can never select the identities in a shipping host.
    command.env_remove(PIN_ENV);
    if let Some(pins) = pins {
        command.env(
            PIN_ENV,
            serde_json::to_string(&pins.content_pins).map_err(|error| error.to_string())?,
        );
    }
    Ok(())
}

struct PrivateStage(PathBuf);
impl Drop for PrivateStage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Freeze all three native workers and the complete union of their non-system
/// dependency closures before compiling either host. The dependency collector
/// admits only the qualified FFmpeg prefix; these workers do not load ORT.
pub(super) fn prepare(
    output: &Path,
    built: &BTreeMap<String, PathBuf>,
    prefix_lib: &Path,
    signer: &Signer<'_>,
) -> Result<Pins> {
    use std::os::unix::fs::DirBuilderExt;

    let stage = output.join(format!(".Deadpan.native-pins-{}", std::process::id()));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&stage)
        .map_err(|error| format!("private native pin staging: {error}"))?;
    let stage = PrivateStage(stage);
    let contents = stage.0.join(APP_NAME).join("Contents");
    let macos = contents.join("MacOS");
    let frameworks = contents.join("Frameworks");
    fs::create_dir_all(&macos).map_err(|error| error.to_string())?;
    let mut source_workers = BTreeMap::new();
    let mut workers = Vec::new();
    for name in WORKERS {
        let source = built
            .get(name)
            .ok_or_else(|| format!("missing built {name}"))?;
        let hash = sha256_file(source)?;
        let worker = macos.join(name);
        copy(source, &worker, 0o755)?;
        if sha256_file(&worker)? != hash {
            return Err(format!("{name} changed while staging its independent pin"));
        }
        source_workers.insert(name.to_owned(), hash);
        workers.push(worker);
    }
    let libraries = macho::bundle_libraries(&workers, &frameworks, prefix_lib, None)?;
    if libraries.is_empty() {
        return Err("native pin preparation found no qualified FFmpeg libraries".into());
    }
    for library in &libraries {
        refinement::sign_image(signer, library, None)?;
    }
    for worker in &workers {
        let name = worker
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or("native worker has no UTF-8 name")?;
        refinement::sign_image(signer, worker, Some(&format!("{DEFAULT_BUNDLE_ID}.{name}")))?;
    }
    let mut content_pins = BTreeMap::new();
    for image in workers.iter().chain(&libraries) {
        let relative = image
            .strip_prefix(&contents)
            .map_err(|error| error.to_string())?
            .to_str()
            .ok_or("native pin path is not UTF-8")?
            .to_owned();
        content_pins.insert(relative, refinement::content(image)?);
    }
    Ok(Pins {
        content_pins,
        source_workers,
    })
}

/// A host-only rebuild may not change the Cargo images from which its
/// independently relocated worker identities were prepared.
pub(super) fn verify_sources(built: &BTreeMap<String, PathBuf>, pins: &Pins) -> Result<()> {
    for (name, expected) in &pins.source_workers {
        let path = built
            .get(name)
            .ok_or_else(|| format!("missing built {name} after host pin rebuild"))?;
        if sha256_file(path)? != *expected {
            return Err(format!(
                "host pin rebuild changed the independently prepared {name}"
            ));
        }
    }
    Ok(())
}

pub(super) fn verify_images(contents: &Path, pins: &Pins) -> Result<()> {
    for (relative, expected) in &pins.content_pins {
        if refinement::content(&contents.join(relative))? != *expected {
            return Err(format!(
                "final {relative} differs from the native image pin compiled into the hosts"
            ));
        }
    }
    Ok(())
}

fn check_probe(value: &Value, pins: &Pins) -> Result<()> {
    let expected = serde_json::to_value(&pins.content_pins).map_err(|error| error.to_string())?;
    if value["content_pins"] != expected {
        return Err("host does not carry the independently prepared native image pins; rebuild without --no-build".into());
    }
    Ok(())
}

/// Exercise each final host's admission implementation after the complete
/// resource seal exists. In particular, --no-build cannot bless stale hosts.
pub(super) fn check_hosts(app: &Path, pins: &Pins) -> Result<()> {
    verify_images(&app.join("Contents"), pins)?;
    for host in refinement::HOSTS {
        let mut command = Command::new(app.join("Contents/MacOS").join(host));
        if host == "deadpan-app" {
            command.arg("--headless");
        }
        let result = command
            .arg("native-runtime-qualify")
            .env_clear()
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| format!("{host} native qualifier: {error}"))?;
        if !result.status.success() {
            return Err(format!(
                "{host} refused its packaged native runtime ({}): {}",
                result.status,
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        let value: Value = serde_json::from_slice(&result.stdout)
            .map_err(|error| format!("{host} native qualifier JSON: {error}"))?;
        check_probe(&value, pins)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins() -> Pins {
        Pins {
            content_pins: BTreeMap::from([
                ("MacOS/deadpan-transcribe".into(), "a".repeat(64)),
                ("MacOS/deadpan-track".into(), "b".repeat(64)),
                ("MacOS/deadpan-media-worker".into(), "c".repeat(64)),
                ("Frameworks/libavcodec.62.dylib".into(), "d".repeat(64)),
            ]),
            source_workers: BTreeMap::new(),
        }
    }

    #[test]
    fn worker_build_removes_inherited_pin_map_and_hosts_get_canonical_map() {
        let mut command = Command::new("cargo");
        command.env(PIN_ENV, "inherited");
        build_environment(&mut command, None).unwrap();
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(environment.get(OsStr::new(PIN_ENV)), Some(&None));
        let pins = pins();
        build_environment(&mut command, Some(&pins)).unwrap();
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        let encoded = serde_json::to_string(&pins.content_pins).unwrap();
        assert_eq!(
            environment.get(OsStr::new(PIN_ENV)),
            Some(&Some(OsStr::new(&encoded)))
        );
    }

    #[test]
    fn missing_partial_stale_or_extra_native_pin_reports_refuse_no_build() {
        let pins = pins();
        let correct = json!({ "content_pins": pins.content_pins });
        assert!(check_probe(&correct, &pins).is_ok());
        let mut missing_worker = pins.content_pins.clone();
        missing_worker.remove("MacOS/deadpan-track");
        let mut missing_library = pins.content_pins.clone();
        missing_library.remove("Frameworks/libavcodec.62.dylib");
        let mut changed = pins.content_pins.clone();
        changed.insert("MacOS/deadpan-media-worker".into(), "e".repeat(64));
        let mut extra = pins.content_pins.clone();
        extra.insert("Frameworks/unprepared.dylib".into(), "f".repeat(64));
        for wrong in [
            json!({}),
            json!({ "content_pins": null }),
            json!({ "content_pins": missing_worker }),
            json!({ "content_pins": missing_library }),
            json!({ "content_pins": changed }),
            json!({ "content_pins": extra }),
        ] {
            assert!(check_probe(&wrong, &pins).is_err(), "{wrong}");
        }
    }

    #[test]
    fn host_rebuild_must_preserve_every_prepared_native_worker_source() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "deadpan-native-source-pins-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let stage = PrivateStage(directory);
        let mut built = BTreeMap::new();
        let mut pins = pins();
        for name in WORKERS {
            let path = stage.0.join(name);
            fs::write(&path, name).unwrap();
            pins.source_workers
                .insert(name.to_owned(), sha256_file(&path).unwrap());
            built.insert(name.to_owned(), path);
        }
        verify_sources(&built, &pins).unwrap();
        for name in WORKERS {
            let path = &built[name];
            fs::write(path, "changed").unwrap();
            assert!(verify_sources(&built, &pins).is_err(), "{name}");
            fs::write(path, name).unwrap();
            verify_sources(&built, &pins).unwrap();
        }
        built.remove("deadpan-track");
        assert!(verify_sources(&built, &pins).is_err());
    }
}
