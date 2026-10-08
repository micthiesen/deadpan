//! Capture real immutable project inputs for a separately selected development
//! extension probe. This does not approve/install a provider, run inference,
//! publish a candidate or mutate the project. All JSON arguments are file paths.
//!
//! prepare_extension_context --project PROJECT --revision REVISION
//!   --target TARGET.json --plan PLAN.json --provider SELECTED_PROVIDER.json
//!   --output NEW_DIRECTORY [--options OPTIONS.json]
//!
//! A complete capture has capture.json plus inputs/context.json, chronological
//! context PNGs, optional opposite.png and continuity.bin. References remain
//! relative to NEW_DIRECTORY so it can become the worker ArtifactWorkspace.

use std::{
    collections::BTreeMap,
    error::Error,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use deadpan_cli::generation::conditioning::prepare_extension_scoped_with_provider;
use deadpan_core::{RevisionId, ScopedNodeTarget};
use deadpan_jobs::{ExtensionGenerationPlan, GenerationOptions};
use deadpan_models::{BoundaryClock, ExtensionContext, SelectedExtensionProvider};
use serde::de::DeserializeOwned;
use serde_json::json;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const MAXIMUM_ARGUMENT_BYTES: u64 = 64 * 1024;
const USAGE: &str = "usage: prepare_extension_context --project PROJECT --revision REVISION --target TARGET.json --plan PLAN.json --provider SELECTED_PROVIDER.json --output NEW_DIRECTORY [--options OPTIONS.json]";

fn main() -> Result<()> {
    let arguments = arguments()?;
    let package = PathBuf::from(&arguments["--project"]);
    let revision = RevisionId::new(
        arguments["--revision"]
            .to_str()
            .ok_or("revision must be UTF-8")?,
    )?;
    let target: ScopedNodeTarget = read_json(Path::new(&arguments["--target"]))?;
    let plan: ExtensionGenerationPlan = read_json(Path::new(&arguments["--plan"]))?;
    let selected: SelectedExtensionProvider = read_json(Path::new(&arguments["--provider"]))?;
    let options: GenerationOptions = arguments
        .get("--options")
        .map(|path| read_json(Path::new(path)))
        .transpose()?
        .unwrap_or_default();
    let output = PathBuf::from(&arguments["--output"]);
    match fs::symlink_metadata(&output) {
        Ok(_) => return Err("output must be a new directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let captured = prepare_extension_scoped_with_provider(
        &package,
        &revision,
        &target,
        &plan,
        &selected,
        &options,
        &AtomicBool::new(false),
    )?;
    let manifest: ExtensionContext = serde_json::from_slice(&captured.manifest)?;
    let BoundaryClock::Definition {
        project_id,
        revision_id,
        ..
    } = manifest
        .context()
        .first()
        .ok_or("captured extension has no context")?
        .picture
        .clock()
    else {
        return Err("captured extension has no immutable definition clock".into());
    };
    if revision_id != &revision {
        return Err("captured extension revision differs from requested revision".into());
    }
    // create_dir refuses an existing output, including one created while the
    // bounded capture ran. The completion report is written last.
    fs::DirBuilder::new().mode(0o700).create(&output)?;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(output.join("inputs"))?;
    write_new(&output.join("inputs/context.json"), &captured.manifest)?;
    for (index, png) in captured.context_pngs.iter().enumerate() {
        write_new(&output.join(format!("inputs/context-{index:03}.png")), png)?;
    }
    if let Some(png) = &captured.opposite_png {
        write_new(&output.join("inputs/opposite.png"), png)?;
    }
    write_new(
        &output.join("inputs/continuity.bin"),
        &captured.continuity_signatures,
    )?;
    let report = json!({
        "schema_version": 1,
        "scope": "development input capture only; selected provider is not approved or installed by this tool",
        "project_id": project_id,
        "revision_id": revision,
        "target": target,
        "plan": captured.plan,
        "constraints": captured.constraints,
        "selected_provider": selected,
        "input": {
            "manifest": "inputs/context.json",
            "sha256": captured.manifest_sha256,
            "byte_length": captured.manifest.len()
        },
        "continuity": captured.continuity,
        "context_pngs": captured.context_pngs.len(),
        "opposite_present": captured.opposite_png.is_some()
    });
    write_new(
        &output.join("capture.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", output.join("capture.json").display());
    Ok(())
}

fn arguments() -> Result<BTreeMap<String, OsString>> {
    let mut values = BTreeMap::new();
    let mut arguments = std::env::args_os().skip(1);
    while let Some(key) = arguments.next() {
        let key = key.into_string().map_err(|_| USAGE)?;
        if ![
            "--project",
            "--revision",
            "--target",
            "--plan",
            "--provider",
            "--output",
            "--options",
        ]
        .contains(&key.as_str())
        {
            return Err(USAGE.into());
        }
        let value = arguments.next().ok_or(USAGE)?;
        if values.insert(key, value).is_some() {
            return Err("each capture option may be supplied only once".into());
        }
    }
    if [
        "--project",
        "--revision",
        "--target",
        "--plan",
        "--provider",
        "--output",
    ]
    .iter()
    .any(|key| !values.contains_key(*key))
    {
        return Err(USAGE.into());
    }
    Ok(values)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(i32::try_from(rustix::fs::OFlags::NONBLOCK.bits())?)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAXIMUM_ARGUMENT_BYTES {
        return Err("capture arguments must be regular JSON files of at most 64 KiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAXIMUM_ARGUMENT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len())? > MAXIMUM_ARGUMENT_BYTES {
        return Err("capture argument grew beyond 64 KiB while reading".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
