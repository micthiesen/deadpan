//! The user's gag presets: a built-in recipe with its exact parameters kept
//! under a name in Application Support and shared by every project
//! (specification §8.4). A preset holds no media and no project identity, so
//! it means the same thing in any project: `:gag NAME` expands it exactly as
//! `:gag` with those parameters would. Fixed-content local recipes stay in
//! their project because they show that project's Original.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use deadpan_core::GagRecipe;
use serde::{Deserialize, Serialize};

/// Preset count and file size bounds.
pub const MAX_PRESETS: usize = 128;
const MAX_BYTES: u64 = 256 * 1024;
const FILE_VERSION: u32 = 1;

/// Entries stay raw JSON until each is read, so one entry that this Deadpan
/// cannot read (a hand edit, or a newer recipe) is skipped and kept, never
/// the reason to lose the rest.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    version: u32,
    presets: BTreeMap<String, serde_json::Value>,
}

/// The readable presets, and the names of entries that were skipped.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub presets: BTreeMap<String, GagRecipe>,
    pub skipped: Vec<String>,
}

/// Where the presets live. Replays and tests use a private file; headless
/// and worker paths never read the user's library.
#[derive(Debug, Clone, Default)]
pub struct GagPresets {
    path: Option<PathBuf>,
}

impl GagPresets {
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn at(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// `Application Support/Deadpan/gag-presets.json` for the native app.
    pub fn native() -> Self {
        Self {
            path: crate::keymap_file::application_support_directory()
                .ok()
                .map(|directory| directory.join("Deadpan").join("gag-presets.json")),
        }
    }

    fn path(&self) -> Result<&Path, String> {
        self.path
            .as_deref()
            .ok_or_else(|| "No Application Support directory is available for gag presets.".into())
    }

    fn read_wire(path: &Path) -> Result<Wire, String> {
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Wire {
                    version: FILE_VERSION,
                    presets: BTreeMap::new(),
                });
            }
            Err(error) => return Err(format!("Gag presets could not be read: {error}")),
        };
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Gag presets could not be read: {error}"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("The gag preset file is larger than 256 KiB.".into());
        }
        let wire: Wire = serde_json::from_slice(&bytes)
            .map_err(|error| format!("The gag preset file is invalid: {error}"))?;
        if wire.version != FILE_VERSION {
            return Err(format!(
                "The gag preset file has version {}; this Deadpan reads version {FILE_VERSION}.",
                wire.version
            ));
        }
        if wire.presets.len() > MAX_PRESETS {
            return Err(format!(
                "The gag preset file has more than {MAX_PRESETS} presets."
            ));
        }
        Ok(wire)
    }

    /// Every readable preset, or none when the file does not exist yet.
    pub fn load(&self) -> Result<Loaded, String> {
        let wire = Self::read_wire(self.path()?)?;
        let mut loaded = Loaded::default();
        for (name, value) in wire.presets {
            match (
                check_name(&name),
                serde_json::from_value::<GagRecipe>(value),
            ) {
                (Ok(()), Ok(recipe)) => {
                    loaded.presets.insert(name, recipe);
                }
                _ => loaded.skipped.push(name),
            }
        }
        Ok(loaded)
    }

    /// One preset by name.
    pub fn get(&self, name: &str) -> Result<GagRecipe, String> {
        check_name(name)?;
        let loaded = self.load()?;
        if loaded.skipped.iter().any(|skipped| skipped == name) {
            return Err(format!(
                "The saved gag preset {name} cannot be read by this Deadpan; save it again with :gag-save."
            ));
        }
        loaded.presets.get(name).copied().ok_or_else(|| {
            format!("No gag or saved gag preset is named {name}. :gag-presets lists them.")
        })
    }

    /// Save or replace `name` under an exclusive lock: read, change and write
    /// a complete new file to a unique temporary name, then rename it over
    /// the old one, so concurrent saves never lose each other's presets and a
    /// failure never leaves a partial library. Unreadable entries are kept.
    pub fn save(&self, name: &str, recipe: GagRecipe) -> Result<bool, String> {
        check_name(name)?;
        let path = self.path()?;
        let directory = path
            .parent()
            .ok_or("The gag preset location has no directory.")?;
        let failed = |error: std::io::Error| format!("Gag presets could not be saved: {error}");
        std::fs::create_dir_all(directory).map_err(failed)?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join(".gag-presets.lock"))
            .map_err(failed)?;
        lock.lock().map_err(failed)?;
        let mut wire = Self::read_wire(path)?;
        let value = serde_json::to_value(recipe).map_err(|error| error.to_string())?;
        let replaced = wire.presets.insert(name.to_owned(), value).is_some();
        if wire.presets.len() > MAX_PRESETS {
            return Err(format!("At most {MAX_PRESETS} gag presets can be saved."));
        }
        let bytes = serde_json::to_vec_pretty(&wire).map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let temporary = directory.join(format!(".gag-presets-{}-{stamp}.tmp", std::process::id()));
        let written = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temporary, path)
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(failed(error));
        }
        lock.unlock().map_err(failed)?;
        Ok(replaced)
    }
}

/// Lowercase letters, digits and hyphens, at most 32, never a built-in name.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 32
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("A gag preset name is 1 to 32 lowercase letters, digits or hyphens.".into());
    }
    if crate::navigation::gag::BUILT_IN.contains(&name) {
        return Err(format!(
            "{name} is a built-in gag; choose another preset name."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    fn recipe(plays: u32) -> GagRecipe {
        GagRecipe::OneMoreTime {
            version: 1,
            plays: NonZeroU32::new(plays).unwrap(),
            gap: deadpan_core::PauseLength::Frames {
                frames: NonZeroU32::new(12).unwrap(),
            },
            shorten: deadpan_core::PauseLength::Frames {
                frames: NonZeroU32::new(3).unwrap(),
            },
            variation: None,
        }
    }

    #[test]
    fn presets_round_trip_in_one_bounded_file_and_refuse_bad_names() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("Deadpan").join("gag-presets.json");
        let presets = GagPresets::at(file.clone());
        assert!(
            presets.load().unwrap().presets.is_empty(),
            "a missing file is empty"
        );
        assert!(!presets.save("stutter-4", recipe(4)).unwrap());
        assert!(presets.save("stutter-4", recipe(4)).unwrap(), "replaced");
        assert_eq!(presets.get("stutter-4").unwrap(), recipe(4));
        assert!(presets.get("missing").is_err());
        for bad in ["", "Loud", "one more", "one-more-time", &"x".repeat(33)] {
            assert!(presets.save(bad, recipe(4)).is_err(), "{bad:?}");
        }
        // No temporary file is left behind.
        let leftovers: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        // A foreign version is reported, never replaced silently.
        std::fs::write(&file, b"{\"version\":2,\"presets\":{}}").unwrap();
        assert!(presets.load().is_err());
        assert!(presets.save("again", recipe(4)).is_err());
    }

    #[test]
    fn one_unreadable_entry_is_skipped_and_kept_while_the_rest_load_and_save() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("gag-presets.json");
        std::fs::write(
            &file,
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "presets": {
                    "good": recipe(3),
                    "broken": {"recipe": "shrug", "version": 1},
                    "Bad Name": recipe(3)
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let presets = GagPresets::at(file.clone());
        let loaded = presets.load().unwrap();
        assert_eq!(loaded.presets.keys().collect::<Vec<_>>(), ["good"]);
        assert_eq!(loaded.skipped, ["Bad Name", "broken"]);
        assert!(
            presets
                .get("broken")
                .unwrap_err()
                .contains("cannot be read")
        );
        presets.save("more", recipe(5)).unwrap();
        let raw = std::fs::read_to_string(&file).unwrap();
        assert!(
            raw.contains("\"broken\"") && raw.contains("\"more\""),
            "{raw}"
        );
    }

    #[test]
    fn concurrent_saves_keep_every_preset() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("gag-presets.json");
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let presets = GagPresets::at(file.clone());
                std::thread::spawn(move || {
                    presets
                        .save(&format!("preset-{index}"), recipe(index + 2))
                        .unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(GagPresets::at(file).load().unwrap().presets.len(), 8);
    }
}
