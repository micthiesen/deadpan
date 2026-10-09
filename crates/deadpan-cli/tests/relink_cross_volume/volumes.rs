use super::support::{Result, Run, require};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::MetadataExt, path::PathBuf, process::Command};

/// Cleanup authority comes only from this private image's exact path in
/// hdiutil's inventory. Normal detach also checks its exact private mountpoint;
/// failed attach cleanup can detach an unmounted entity of that same image.
/// Never select a disk from an unrelated entry or an old saved device number.
pub struct Volume<'a> {
    run: &'a Run,
    name: &'static str,
    image: PathBuf,
    pub mount: PathBuf,
    pub device: u64,
    finished: bool,
}

impl<'a> Volume<'a> {
    pub fn create(run: &'a Run, name: &'static str) -> Result<Self> {
        let image = run.root.join(format!("volume-{name}.dmg"));
        let mount = run.root.join(format!("mount-{name}"));
        fs::create_dir(&mount)?;
        let mut volume = Self {
            run,
            name,
            image,
            mount,
            device: 0,
            finished: false,
        };
        run.success(
            &format!("create-{name}"),
            Command::new("hdiutil")
                .args([
                    "create", "-size", "128m", "-fs", "APFS", "-layout", "NONE", "-type", "UDIF",
                    "-volname",
                ])
                .arg(format!("Deadpan-relink-{name}"))
                .arg(&volume.image),
        )?;
        let attached = run.success(
            &format!("attach-{name}"),
            Command::new("hdiutil")
                .args([
                    "attach",
                    "-plist",
                    "-nobrowse",
                    "-noautoopen",
                    "-mountpoint",
                ])
                .arg(&volume.mount)
                .arg(&volume.image),
        )?;
        volume.device = fs::metadata(&volume.mount)?.dev();
        let attached = run.plist_json(&format!("attach-{name}-json"), &attached)?;
        let entity = volume
            .mount_entity(&attached)?
            .ok_or("attach did not report its private mountpoint")?;
        require(
            volume.device != fs::metadata(&run.root)?.dev(),
            "private APFS mount did not change filesystem device",
        )?;
        let observed = volume
            .owned_inventory()?
            .ok_or("attached image missing from hdiutil inventory")?;
        let observed_entity = volume
            .mount_entity(&observed)?
            .ok_or("inventory lost private mountpoint")?;
        require(
            observed_entity["dev-entry"] == entity["dev-entry"],
            "attach and inventory device identities differ",
        )?;
        run.save(&format!("volume-{name}.json"), &json!({"image":volume.image,"mount":volume.mount,
            "volume_device":volume.device,"host_device":fs::metadata(&run.root)?.dev(),"entity":entity}))?;
        Ok(volume)
    }

    fn mount_entity(&self, image: &Value) -> Result<Option<Value>> {
        let entities = image["system-entities"]
            .as_array()
            .ok_or("image has no system entities")?;
        require(
            entities.len() <= 32,
            "image entity count exceeds fixture bound",
        )?;
        let matches: Vec<_> = entities
            .iter()
            .filter(|entity| {
                entity["mount-point"]
                    .as_str()
                    .is_some_and(|path| std::path::Path::new(path) == self.mount)
            })
            .collect();
        require(
            matches.len() <= 1,
            "private mountpoint has ambiguous devices",
        )?;
        let Some(entity) = matches.first() else {
            return Ok(None);
        };
        let device = entity["dev-entry"]
            .as_str()
            .ok_or("mounted entity has no device")?;
        validate_device(device)?;
        Ok(Some((*entity).clone()))
    }

    fn owned_inventory(&self) -> Result<Option<Value>> {
        let info = self.run.success(
            &format!("info-{}", self.name),
            Command::new("hdiutil").args(["info", "-plist"]),
        )?;
        let info = self
            .run
            .plist_json(&format!("info-{}-json", self.name), &info)?;
        let images = info["images"]
            .as_array()
            .ok_or("hdiutil info has no image list")?;
        require(
            images.len() <= 256,
            "hdiutil image count exceeds fixture bound",
        )?;
        let mut found = None;
        for image in images {
            let path = image["image-path"]
                .as_str()
                .ok_or("hdiutil image has no backing path")?;
            if image_matches(std::path::Path::new(path), &self.image)? {
                require(found.is_none(), "owned image is attached more than once")?;
                found = Some(image.clone());
            }
        }
        Ok(found)
    }

    pub fn assert_detached(&self, label: &str) -> Result {
        let observed = self.owned_inventory()?;
        let mount_device = match fs::metadata(&self.mount) {
            Ok(metadata) => Some(metadata.dev()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        self.run.save(&format!("{label}.json"), &json!({"image":self.image,"mount":self.mount,
            "attached":observed.is_some(),"mount_device":mount_device,"former_volume_device":self.device,
            "host_device":fs::metadata(&self.run.root)?.dev(),"owned_inventory":observed}))?;
        require(
            observed.is_none(),
            "detached Original image was mounted again",
        )?;
        require(
            mount_device.is_none() || mount_device == Some(fs::metadata(&self.run.root)?.dev()),
            "detached mountpoint still belongs to a mounted volume",
        )
    }

    pub fn detach(&self) -> Result {
        self.detach_owned(false)
    }

    fn detach_owned(&self, allow_partial_attach: bool) -> Result {
        if let Some(image) = self.owned_inventory()? {
            let entity = match self.mount_entity(&image)? {
                Some(entity) => {
                    if !allow_partial_attach {
                        require(
                            fs::metadata(&self.mount)?.dev() == self.device
                                && self.device != fs::metadata(&self.run.root)?.dev(),
                            "private mountpoint changed filesystem device",
                        )?;
                    }
                    entity
                }
                None if allow_partial_attach => image["system-entities"]
                    .as_array()
                    .and_then(|entities| {
                        entities
                            .iter()
                            .find(|entity| entity["dev-entry"].is_string())
                    })
                    .cloned()
                    .ok_or("partial owned attachment has no device")?,
                None => return Err("owned image moved away from its private mountpoint".into()),
            };
            let device = entity["dev-entry"].as_str().ok_or("missing owned device")?;
            validate_device(device)?;
            self.run.success(
                &format!("detach-{}", self.name),
                Command::new("hdiutil").arg("detach").arg(device),
            )?;
        }
        self.assert_detached(&format!("detached-{}", self.name))
    }

    pub fn finish(&mut self) -> Result {
        self.detach_owned(true)?;
        self.finished = true;
        Ok(())
    }
}

fn image_matches(path: &std::path::Path, owned: &std::path::Path) -> std::io::Result<bool> {
    if !path.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "hdiutil image path is not absolute",
        ));
    }
    // The fixture root is canonical. Match an owned spelling even if its
    // backing file disappeared while the image remains attached. A failed
    // resolution must never prove that an image is absent.
    let spelling = path.strip_prefix("/tmp").map_or_else(
        |_| path.to_owned(),
        |suffix| std::path::Path::new("/private/tmp").join(suffix),
    );
    if spelling == owned {
        return Ok(true);
    }
    Ok(path.canonicalize()? == owned)
}

#[test]
fn inventory_path_matching_retains_missing_owned_paths_and_refuses_ambiguity() {
    let root = tempfile::tempdir_in("/tmp").expect("scratch");
    let root = root.path().canonicalize().expect("canonical scratch");
    let owned = root.join("owned.dmg");
    assert!(image_matches(&owned, &owned).expect("missing exact owned path"));
    let alias =
        std::path::Path::new("/tmp").join(owned.strip_prefix("/private/tmp").expect("private tmp"));
    assert!(image_matches(&alias, &owned).expect("missing alias"));
    let unrelated = root.join("unrelated.dmg");
    fs::write(&unrelated, []).expect("unrelated file");
    assert!(!image_matches(&unrelated, &owned).expect("unrelated resolvable file"));
    assert!(image_matches(&root.join("unknown.dmg"), &owned).is_err());
    assert!(image_matches(std::path::Path::new("relative.dmg"), &owned).is_err());
}

fn validate_device(device: &str) -> Result {
    require(
        device.strip_prefix("/dev/disk").is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.len() <= 32
                && suffix.as_bytes()[0].is_ascii_digit()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b's')
        }),
        "owned image entity has invalid device name",
    )
}

impl Drop for Volume<'_> {
    fn drop(&mut self) {
        if !self.finished
            && let Err(error) = self.finish()
        {
            let _ = self.run.save(
                &format!("cleanup-{}-unconfirmed.json", self.name),
                &json!({"image":self.image,"mount":self.mount,"error":error.to_string()}),
            );
            eprintln!(
                "owned relink volume {} cleanup remains unconfirmed: {error}",
                self.name
            );
        }
    }
}
