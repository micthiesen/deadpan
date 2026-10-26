//! Freeze the entire executable AI runtime before compiling either host.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::super::{Result, Signer, refinement, run_tool, strip_attributes};
use super::{Built, DIRECTORY, install, sign};

#[path = "../../../../deadpan-cli/src/bundle/ai_inventory.rs"]
mod inventory;

pub(super) const PIN_ENV: &str = "DEADPAN_AI_RUNTIME_INVENTORY_SHA256";
pub const INVENTORY: &str = "Resources/ai-runtime-inventory.json";

/// A private, signed snapshot which survives the host build. Neither final
/// assembly nor notice generation reads the mutable build cache again.
pub struct Prepared {
    pub built: Built,
    pub inventory_sha256: String,
    pub signed: (usize, usize),
    inventory: Vec<u8>,
    frozen_runtime: String,
    frozen_notices: String,
    _stage: PrivateStage,
}

struct PrivateStage(PathBuf);

impl Drop for PrivateStage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn exact_hash(bytes: &[u8]) -> io::Result<String> {
    Ok(inventory::digest(bytes))
}

/// Exact bytes as well as paths, modes and links. Unlike the launch inventory,
/// this private build fingerprint includes each complete code signature.
fn exact_tree(root: &Path) -> Result<String> {
    let value =
        inventory::collect(root, || Ok(()), exact_hash).map_err(|error| error.to_string())?;
    let bytes = inventory::canonical_bytes(&value).map_err(|error| error.to_string())?;
    Ok(inventory::digest(&bytes))
}

fn launch_inventory(root: &Path) -> Result<Vec<u8>> {
    let value = inventory::collect(
        root,
        || Ok(()),
        |bytes| refinement::macho_content::content_sha256(bytes).map_err(io::Error::other),
    )
    .map_err(|error| error.to_string())?;
    let bytes = inventory::canonical_bytes(&value).map_err(|error| error.to_string())?;
    inventory::parse(&bytes).map_err(|error| error.to_string())?;
    Ok(bytes)
}

pub(in crate::bundle) fn build_environment(command: &mut Command, prepared: Option<&Prepared>) {
    // Do this even for --without-ai-runtime and the first worker-only build.
    command.env_remove(PIN_ENV);
    if let Some(prepared) = prepared {
        command.env(PIN_ENV, &prepared.inventory_sha256);
    }
}

/// Copy, compare and clean-sign the private tree exactly once. The source
/// report is already in memory; notices are copied into this same private
/// stage, so later cache activity cannot change the published license record.
pub(in crate::bundle) fn prepare(
    built: Built,
    output: &Path,
    signer: &Signer<'_>,
) -> Result<Prepared> {
    use std::os::unix::fs::DirBuilderExt;

    let stage = output.join(format!(".Deadpan.ai-pins-{}", std::process::id()));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&stage)
        .map_err(|error| format!("private AI pin staging: {error}"))?;
    let stage = PrivateStage(stage);
    let source_runtime = exact_tree(&built.runtime)?;
    let source_notices = exact_tree(&built.notices)?;
    let contents = stage.0.join("Deadpan.app/Contents");
    fs::create_dir_all(&contents).map_err(|error| error.to_string())?;
    let runtime = install(&built, &contents)?;
    let notices = stage.0.join("notices");
    run_tool("ditto", &[built.notices.as_os_str(), notices.as_os_str()])?;
    if exact_tree(&built.runtime)? != source_runtime
        || exact_tree(&runtime)? != source_runtime
        || exact_tree(&built.notices)? != source_notices
        || exact_tree(&notices)? != source_notices
    {
        return Err("AI runtime or notices changed while freezing the build input".into());
    }
    strip_attributes(&stage.0)?;
    // No entitlement exceptions: the qualified Python/MLX/Metal runtime runs
    // under the same hardened policy as the final frozen copy.
    let signed = sign(&runtime, &|path, identifier| {
        refinement::sign_image(signer, path, identifier)
    })?;
    let inventory = launch_inventory(&runtime)?;
    let inventory_sha256 = inventory::digest(&inventory);
    let frozen_runtime = exact_tree(&runtime)?;
    let frozen_notices = exact_tree(&notices)?;
    Ok(Prepared {
        built: Built {
            runtime,
            notices,
            report: built.report,
        },
        inventory_sha256,
        signed,
        inventory,
        frozen_runtime,
        frozen_notices,
        _stage: stage,
    })
}

impl Prepared {
    /// Recheck across the host compilation and every publication read.
    pub(in crate::bundle) fn verify_frozen(&self) -> Result<()> {
        if exact_tree(&self.built.runtime)? != self.frozen_runtime
            || exact_tree(&self.built.notices)? != self.frozen_notices
        {
            return Err(
                "private frozen AI runtime or notices changed during the host build".into(),
            );
        }
        Ok(())
    }

    /// Publish the already signed copy. Never sign it again: final bytes must
    /// be the exact independently prepared images used to compile the hosts.
    pub(in crate::bundle) fn install(&self, contents: &Path) -> Result<PathBuf> {
        self.verify_frozen()?;
        let runtime = install(&self.built, contents)?;
        self.verify_frozen()?;
        self.verify_installed(&runtime)?;
        let path = contents.join(INVENTORY);
        let mut manifest = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        manifest
            .write_all(&self.inventory)
            .map_err(|error| error.to_string())?;
        drop(manifest);
        self.verify_inventory_file(contents)?;
        Ok(runtime)
    }

    pub(in crate::bundle) fn verify_inventory_file(&self, contents: &Path) -> Result<()> {
        use rustix::fs::{Mode, OFlags, open};
        let descriptor = open(
            contents.join(INVENTORY),
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| error.to_string())?;
        let manifest = fs::File::from(descriptor);
        let metadata = manifest.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() != self.inventory.len() as u64 {
            return Err("final AI inventory resource changed type or size".into());
        }
        let mut bytes = Vec::new();
        manifest
            .take(self.inventory.len() as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes != self.inventory {
            return Err("final AI inventory resource differs from the compiled input".into());
        }
        Ok(())
    }

    pub(in crate::bundle) fn verify_installed(&self, runtime: &Path) -> Result<()> {
        if launch_inventory(runtime)? != self.inventory
            || exact_tree(runtime)? != self.frozen_runtime
        {
            return Err(
                "final AI runtime differs from the inventory independently compiled into its hosts"
                    .into(),
            );
        }
        Ok(())
    }

    pub(in crate::bundle) fn verify_installed_notices(&self, notices: &Path) -> Result<()> {
        if exact_tree(notices)? != self.frozen_notices {
            return Err("final AI notices differ from the private frozen inputs".into());
        }
        Ok(())
    }
}

fn check_probe(value: &serde_json::Value, expected: Option<&str>) -> Result<()> {
    let expected = expected
        .map(|value| serde_json::Value::String(value.to_owned()))
        .unwrap_or(serde_json::Value::Null);
    if value.get("inventory_sha256") != Some(&expected) {
        return Err("host does not carry the independently prepared AI inventory pin; rebuild without --no-build".into());
    }
    Ok(())
}

fn require_absent(contents: &Path) -> Result<()> {
    for relative in [DIRECTORY, INVENTORY] {
        let path = contents.join(relative);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{}: {error}", path.display())),
            Ok(_) => {
                return Err(format!(
                    "--without-ai-runtime unexpectedly contains {}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Before the build directly executes any Python smoke check, both final hosts
/// must exercise their complete independent inventory/signature admission.
pub(in crate::bundle) fn check_hosts(app: &Path, prepared: Option<&Prepared>) -> Result<()> {
    let contents = app.join("Contents");
    if let Some(prepared) = prepared {
        prepared.verify_frozen()?;
        prepared.verify_installed(&contents.join(DIRECTORY))?;
        prepared.verify_inventory_file(&contents)?;
    } else {
        require_absent(&contents)?;
    }
    for host in refinement::HOSTS {
        let mut command = Command::new(contents.join("MacOS").join(host));
        if host == "deadpan-app" {
            command.arg("--headless");
        }
        let result = command
            .arg("ai-runtime-qualify")
            .env_clear()
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| format!("{host} AI qualifier: {error}"))?;
        if !result.status.success() {
            return Err(format!(
                "{host} refused its packaged AI runtime ({}): {}",
                result.status,
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&result.stdout)
            .map_err(|error| format!("{host} AI qualifier JSON: {error}"))?;
        check_probe(
            &value,
            prepared.map(|prepared| prepared.inventory_sha256.as_str()),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::ffi::OsStr;

    fn stage(label: &str) -> PrivateStage {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "deadpan-ai-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        PrivateStage(directory)
    }

    fn fixture() -> Prepared {
        let stage = stage("frozen");
        let runtime = stage.0.join("runtime");
        let notices = stage.0.join("notices");
        fs::create_dir(&runtime).unwrap();
        fs::create_dir(&notices).unwrap();
        fs::write(runtime.join("worker.py"), "print('frozen')").unwrap();
        fs::write(notices.join("LICENSE"), "frozen license").unwrap();
        let inventory = launch_inventory(&runtime).unwrap();
        Prepared {
            frozen_runtime: exact_tree(&runtime).unwrap(),
            frozen_notices: exact_tree(&notices).unwrap(),
            inventory_sha256: inventory::digest(&inventory),
            built: Built {
                runtime,
                notices,
                report: json!({}),
            },
            inventory,
            signed: (0, 0),
            _stage: stage,
        }
    }

    #[test]
    fn every_build_clears_inherited_ai_pins_and_only_hosts_receive_prepared_pins() {
        let mut command = Command::new("cargo");
        command.env(PIN_ENV, "inherited");
        build_environment(&mut command, None);
        let environment: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(environment.get(OsStr::new(PIN_ENV)), Some(&None));
        let prepared = fixture();
        build_environment(&mut command, Some(&prepared));
        let environment: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment.get(OsStr::new(PIN_ENV)),
            Some(&Some(OsStr::new(&prepared.inventory_sha256)))
        );
    }

    #[test]
    fn ai_pin_probes_distinguish_missing_unpinned_stale_and_without_runtime_hosts() {
        let pin = "a".repeat(64);
        assert!(check_probe(&json!({ "inventory_sha256": pin }), Some(&pin)).is_ok());
        assert!(check_probe(&json!({ "inventory_sha256": null }), None).is_ok());
        for wrong in [
            json!({}),
            json!({ "inventory_sha256": null }),
            json!({ "inventory_sha256": "b".repeat(64) }),
        ] {
            assert!(check_probe(&wrong, Some(&pin)).is_err());
        }
        assert!(check_probe(&json!({}), None).is_err());
        assert!(check_probe(&json!({ "inventory_sha256": pin }), None).is_err());
    }

    #[test]
    fn private_frozen_worker_and_notices_cannot_change_while_hosts_build() {
        let prepared = fixture();
        prepared.verify_frozen().unwrap();
        let script = prepared.built.runtime.join("worker.py");
        fs::write(&script, "print('changed')").unwrap();
        assert!(prepared.verify_frozen().is_err());
        fs::write(&script, "print('frozen')").unwrap();
        prepared.verify_frozen().unwrap();
        let license = prepared.built.notices.join("LICENSE");
        fs::write(&license, "changed license").unwrap();
        assert!(prepared.verify_frozen().is_err());
        fs::write(&license, "frozen license").unwrap();
        prepared.verify_frozen().unwrap();
    }

    #[test]
    fn copied_runtime_must_match_the_full_closed_inventory() {
        let prepared = fixture();
        let copy = stage("final-copy");
        let runtime = copy.0.join("runtime");
        fs::create_dir(&runtime).unwrap();
        fs::copy(
            prepared.built.runtime.join("worker.py"),
            runtime.join("worker.py"),
        )
        .unwrap();
        prepared.verify_installed(&runtime).unwrap();
        fs::write(runtime.join("extra.pth"), "import unexpected").unwrap();
        assert!(prepared.verify_installed(&runtime).is_err());
        fs::remove_file(runtime.join("extra.pth")).unwrap();
        fs::write(runtime.join("worker.py"), "print('changed')").unwrap();
        assert!(prepared.verify_installed(&runtime).is_err());
    }

    #[test]
    fn without_runtime_refuses_either_a_tree_or_an_inventory() {
        let scratch = stage("without");
        let contents = scratch.0.join("Contents");
        fs::create_dir_all(contents.join("Resources")).unwrap();
        require_absent(&contents).unwrap();
        let runtime = contents.join(DIRECTORY);
        fs::create_dir(&runtime).unwrap();
        assert!(require_absent(&contents).is_err());
        fs::remove_dir(&runtime).unwrap();
        fs::write(contents.join(INVENTORY), "{}").unwrap();
        assert!(require_absent(&contents).is_err());
    }

    #[test]
    fn final_inventory_resource_must_keep_the_exact_compiled_bytes() {
        let prepared = fixture();
        let scratch = stage("manifest");
        let contents = scratch.0.join("Contents");
        fs::create_dir_all(contents.join("Resources")).unwrap();
        let manifest = contents.join(INVENTORY);
        fs::write(&manifest, &prepared.inventory).unwrap();
        prepared.verify_inventory_file(&contents).unwrap();
        let mut changed = prepared.inventory.clone();
        changed[0] ^= 1;
        fs::write(&manifest, changed).unwrap();
        assert!(prepared.verify_inventory_file(&contents).is_err());
        fs::write(&manifest, "{}").unwrap();
        assert!(prepared.verify_inventory_file(&contents).is_err());
        fs::remove_file(&manifest).unwrap();
        std::os::unix::fs::symlink(&prepared.built.runtime, &manifest).unwrap();
        assert!(prepared.verify_inventory_file(&contents).is_err());
    }
}
