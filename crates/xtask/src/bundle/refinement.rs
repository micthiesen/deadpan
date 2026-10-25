//! Compile independent relocated alignment image pins into final host builds.

use super::*;

// Use the same strict parser as the runtime, not a second hash definition.
#[path = "../../../deadpan-cli/src/youtube/macho_content.rs"]
mod macho_content;

pub(super) const WORKER_ENV: &str = "DEADPAN_ALIGN_WORKER_CONTENT_SHA256";
pub(super) const ORT_ENV: &str = "DEADPAN_ALIGN_ORT_CONTENT_SHA256";
pub(super) const HOSTS: [&str; 2] = ["deadpan-app", "deadpan-cli"];

pub(super) struct Pins {
    pub worker: String,
    pub ort: String,
    pub source_worker_sha256: String,
}

pub(super) fn build_environment(command: &mut Command, pins: Option<&Pins>) {
    // Never let inherited developer environment values choose shipping pins.
    command.env_remove(WORKER_ENV).env_remove(ORT_ENV);
    if let Some(pins) = pins {
        command
            .env(WORKER_ENV, &pins.worker)
            .env(ORT_ENV, &pins.ort);
    }
}

fn content(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    macho_content::content_sha256(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// A forced in-place re-sign can leave old nonzero signature bytes after the
/// replacement superblob. Remove the old allocation first, as for yt-dlp.
/// Keep the strict content parser's rejection of hidden trailing data.
pub(super) fn sign_image(signer: &Signer<'_>, path: &Path, identifier: Option<&str>) -> Result<()> {
    run_tool(
        "codesign",
        &["--remove-signature".as_ref(), path.as_os_str()],
    )?;
    signer.sign(path, identifier, None)
}

struct PrivateStage(PathBuf);
impl Drop for PrivateStage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Stage only the worker and its closed dependency set. Every relocation and
/// signature operation is the same operation later used by full assembly.
pub(super) fn prepare(
    output: &Path,
    built: &BTreeMap<String, PathBuf>,
    prefix_lib: &Path,
    ort: &macho::OrtRuntime,
    signer: &Signer<'_>,
) -> Result<Pins> {
    use std::os::unix::fs::DirBuilderExt;
    let stage = output.join(format!(".Deadpan.alignment-pins-{}", std::process::id()));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&stage)
        .map_err(|error| format!("private alignment pin staging: {error}"))?;
    let stage = PrivateStage(stage);
    let contents = stage.0.join(APP_NAME).join("Contents");
    let macos = contents.join("MacOS");
    let frameworks = contents.join("Frameworks");
    fs::create_dir_all(&macos).map_err(|error| error.to_string())?;
    let source = built
        .get("deadpan-align")
        .ok_or("missing built deadpan-align")?;
    let source_worker_sha256 = sha256_file(source)?;
    let worker = macos.join("deadpan-align");
    copy(source, &worker, 0o755)?;
    if sha256_file(&worker)? != source_worker_sha256 {
        return Err("alignment worker changed while staging its pin".into());
    }
    let libraries = macho::bundle_libraries(
        std::slice::from_ref(&worker),
        &frameworks,
        prefix_lib,
        Some(ort),
    )?;
    let ort = frameworks.join(macho::ORT_LIBRARY_NAME);
    if !libraries.contains(&ort) {
        return Err("alignment pin preparation did not load qualified ORT".into());
    }
    for library in &libraries {
        sign_image(signer, library, None)?;
    }
    sign_image(
        signer,
        &worker,
        Some(&format!("{DEFAULT_BUNDLE_ID}.deadpan-align")),
    )?;
    Ok(Pins {
        worker: content(&worker)?,
        ort: content(&ort)?,
        source_worker_sha256,
    })
}

pub(super) fn verify_images(contents: &Path, pins: &Pins) -> Result<()> {
    for (relative, expected) in [
        ("MacOS/deadpan-align", &pins.worker),
        ("Frameworks/libonnxruntime.1.dylib", &pins.ort),
    ] {
        if content(&contents.join(relative))? != *expected {
            return Err(format!(
                "final {relative} differs from the image pin compiled into the hosts"
            ));
        }
    }
    Ok(())
}

fn check_probe(value: &Value, pins: &Pins) -> Result<()> {
    if value["worker_content_sha256"].as_str() != Some(pins.worker.as_str())
        || value["ort_content_sha256"].as_str() != Some(pins.ort.as_str())
    {
        return Err("host does not carry the independently prepared alignment pins; rebuild without --no-build".into());
    }
    Ok(())
}

/// Both hosts must actually execute their compiled-pin qualifier from the final
/// signed package. In particular, --no-build cannot bless old unpinned hosts.
pub(super) fn check_hosts(app: &Path, pins: &Pins) -> Result<()> {
    verify_images(&app.join("Contents"), pins)?;
    for host in HOSTS {
        let mut command = Command::new(app.join("Contents/MacOS").join(host));
        if host == "deadpan-app" {
            command.arg("--headless");
        }
        let result = command
            .arg("refinement-runtime-qualify")
            .env_clear()
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| format!("{host} alignment qualifier: {error}"))?;
        if !result.status.success() {
            return Err(format!(
                "{host} refused its packaged alignment runtime ({}): {}",
                result.status,
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        let value: Value = serde_json::from_slice(&result.stdout)
            .map_err(|error| format!("{host} alignment qualifier JSON: {error}"))?;
        check_probe(&value, pins)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn clean_signing_of_an_already_signed_image_retains_strict_content_identity() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "deadpan-clean-signature-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let stage = PrivateStage(directory);
        let image = stage.0.join("native-image");
        copy(Path::new("/bin/echo"), &image, 0o755).unwrap();
        let signer = Signer { identity: None };
        // Reproduce an in-place replacement before applying the clean recipe.
        // Current codesign leaves stale nonzero reserved signature bytes here.
        signer
            .sign(&image, Some("dev.deadpan.clean-signature-test"), None)
            .unwrap();
        sign_image(&signer, &image, Some("dev.deadpan.clean-signature-test")).unwrap();
        let first = content(&image).unwrap();
        run_tool(
            "codesign",
            &["--verify".as_ref(), "--strict".as_ref(), image.as_os_str()],
        )
        .unwrap();
        sign_image(&signer, &image, Some("dev.deadpan.clean-signature-test")).unwrap();
        assert_eq!(content(&image).unwrap(), first);
    }

    #[test]
    fn worker_build_removes_inherited_pins_and_host_build_sets_both() {
        let mut command = Command::new("cargo");
        command
            .env(WORKER_ENV, "inherited")
            .env(ORT_ENV, "inherited");
        build_environment(&mut command, None);
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(environment.get(OsStr::new(WORKER_ENV)), Some(&None));
        assert_eq!(environment.get(OsStr::new(ORT_ENV)), Some(&None));
        let pins = Pins {
            worker: "a".repeat(64),
            ort: "b".repeat(64),
            source_worker_sha256: "c".repeat(64),
        };
        build_environment(&mut command, Some(&pins));
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment.get(OsStr::new(WORKER_ENV)),
            Some(&Some(OsStr::new(&pins.worker)))
        );
        assert_eq!(
            environment.get(OsStr::new(ORT_ENV)),
            Some(&Some(OsStr::new(&pins.ort)))
        );
    }

    #[test]
    fn missing_partial_stale_and_changed_host_pin_reports_cannot_pass_no_build() {
        let pins = Pins {
            worker: "a".repeat(64),
            ort: "b".repeat(64),
            source_worker_sha256: "c".repeat(64),
        };
        let correct =
            json!({ "worker_content_sha256": pins.worker, "ort_content_sha256": pins.ort });
        assert!(check_probe(&correct, &pins).is_ok());
        for wrong in [
            json!({}),
            json!({ "worker_content_sha256": pins.worker }),
            json!({ "worker_content_sha256": "d".repeat(64), "ort_content_sha256": pins.ort }),
            json!({ "worker_content_sha256": pins.worker, "ort_content_sha256": "e".repeat(64) }),
        ] {
            assert!(check_probe(&wrong, &pins).is_err());
        }
    }
}
