//! Verified Original-source components and a real runtime activation check.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use deadpan_analysis::refinement::PronunciationLexicon;
use deadpan_models::packs::{Operation, PackFile, PackManifest, PackStore};
use rustix::fs::{Mode, OFlags};
use sha2::Digest;

use crate::CliError;
use crate::refinement::{RefinementError, RefinementRuntime};
use deadpan_jobs::transcription::ModelInput;

/// Load this on an analysis worker. The lexicon is owned and hash-verified;
/// the inference worker separately re-verifies its checkpoint and statistics.
#[derive(Debug, Clone)]
pub struct RefinementModels {
    pub checkpoint: ModelInput,
    pub statistics: ModelInput,
    pub lexicon: PronunciationLexicon,
    pub dictionary_sha256: String,
    pub phones_sha256: String,
    pub pack_id: String,
    pub pack_version: String,
}

fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), RefinementError> {
    if cancelled.load(Ordering::Acquire) {
        Err(RefinementError::Cancelled)
    } else if Instant::now() >= deadline {
        Err(RefinementError::Deadline)
    } else {
        Ok(())
    }
}

fn component(value: Option<&PackFile>) -> Result<&PackFile, CliError> {
    value.ok_or_else(|| {
        RefinementError::Configuration("refinement pack lacks a required component").into()
    })
}

fn read_text(
    directory: &Path,
    file: &PackFile,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<String, CliError> {
    check(cancelled, deadline)?;
    if file.bytes > deadpan_analysis::refinement::MAX_DATA_BYTES as u64 {
        return Err(
            RefinementError::Configuration("refinement dictionary exceeds byte bound").into(),
        );
    }
    let descriptor = rustix::fs::open(
        directory.join(&file.name),
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    let mut source = File::from(descriptor);
    let metadata = source.metadata()?;
    if !metadata.is_file() || metadata.len() != file.bytes {
        return Err(
            RefinementError::Protocol("dictionary size or regular-file identity".into()).into(),
        );
    }
    let mut bytes = Vec::with_capacity(file.bytes as usize);
    let mut hash = sha2::Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        check(cancelled, deadline)?;
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if bytes.len() as u64 + count as u64 > file.bytes {
            return Err(RefinementError::Protocol("dictionary grew while reading".into()).into());
        }
        hash.update(&buffer[..count]);
        bytes.extend_from_slice(&buffer[..count]);
    }
    check(cancelled, deadline)?;
    let actual: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if bytes.len() as u64 != file.bytes || actual != file.sha256 {
        return Err(RefinementError::Protocol(
            "dictionary SHA256 differs from the approved pack".into(),
        )
        .into());
    }
    String::from_utf8(bytes)
        .map_err(|_| RefinementError::Protocol("dictionary is not UTF-8".into()).into())
}

fn load(
    manifest: &PackManifest,
    directory: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<RefinementModels, CliError> {
    check(cancelled, deadline)?;
    manifest.validate()?;
    let checkpoint = component(manifest.refinement_checkpoint_file())?;
    let statistics = component(manifest.refinement_statistics_file())?;
    let dictionary = component(manifest.refinement_dictionary_file())?;
    let phones = component(manifest.refinement_phones_file())?;
    let dictionary_text = read_text(directory, dictionary, cancelled, deadline)?;
    let phones_text = read_text(directory, phones, cancelled, deadline)?;
    let lexicon = PronunciationLexicon::parse(&dictionary_text, &phones_text)
        .map_err(|error| RefinementError::Protocol(error.to_string()))?;
    check(cancelled, deadline)?;
    Ok(RefinementModels {
        checkpoint: super::model_input(checkpoint, Some(directory.join(&checkpoint.name)))?,
        statistics: super::model_input(statistics, Some(directory.join(&statistics.name)))?,
        lexicon,
        dictionary_sha256: dictionary.sha256.clone(),
        phones_sha256: phones.sha256.clone(),
        pack_id: manifest.pack_id.clone(),
        pack_version: manifest.pack_version.clone(),
    })
}

/// The active verified lexicon and pinned inference inputs, if installed.
pub fn installed_refinement_models(
    root: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Option<RefinementModels>, CliError> {
    check(cancelled, deadline)?;
    let store = PackStore::new(root.to_path_buf());
    for manifest in deadpan_models::packs::approved_packs() {
        if manifest.supports(Operation::RefineWords)
            && let Some(installed) = store.current(&manifest.pack_id)?
        {
            return load(
                &installed.manifest,
                &installed.directory,
                cancelled,
                deadline,
            )
            .map(Some);
        }
    }
    Ok(None)
}

pub(super) fn smoke(
    manifest: &PackManifest,
    directory: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), CliError> {
    let models = load(manifest, directory, cancelled, deadline)?;
    let result = crate::refinement::infer(
        &RefinementRuntime::beside_current_executable()?,
        &models.checkpoint,
        &models.statistics,
        &vec![0.; 64_000],
        "pack-smoke-refinement",
        cancelled,
        deadline,
    )?;
    if result.frames != 201 || result.log_probs.len() != 201 * 41 {
        return Err(RefinementError::Protocol("runtime smoke output shape".into()).into());
    }
    // Frozen independent PyTorch/reference-ONNX observations of 4 s silence,
    // including boundary and interior rows. A hash-only install is insufficient.
    for (frame, label, expected) in [
        (0, 9, -38.05878829956055),
        (0, 40, -157.64198303222656),
        (100, 9, -29.946212768554688),
        (100, 40, -92.23995971679688),
        (200, 9, -26.198474884033203),
        (200, 40, -89.13595581054688),
    ] {
        if (result.log_probs[frame * 41 + label] - expected).abs() > 0.001 {
            return Err(RefinementError::Protocol("runtime smoke numerical result".into()).into());
        }
    }
    Ok(())
}
