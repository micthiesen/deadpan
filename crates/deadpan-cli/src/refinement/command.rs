use super::{AcceptanceRequest, PreparedRefinement, RefinementError, RefinementRuntime};
use deadpan_analysis::refinement::RefinementOfferAvailability;
use deadpan_analysis::{CorrectedTranscript, Corrections};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};
use sha2::Digest;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

const USAGE: &str = "usage: refine-words <project.deadpan> --word <zero-based-index> [--accept <proposal-id>] [--models-root <directory>]";

struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}

fn acceptance(
    prepared: &PreparedRefinement,
    store: &ProjectStore,
    word: usize,
) -> Result<Option<AcceptanceRequest>, crate::CliError> {
    let Some(offer) = prepared
        .review
        .offers()
        .iter()
        .find(|offer| offer.word_index == word)
    else {
        return Ok(None);
    };
    if offer.availability != RefinementOfferAvailability::Approximate {
        return Ok(None);
    }
    let snapshot = store.transcript_selection(&prepared.selection_guard.key)?;
    if snapshot.guard != prepared.selection_guard {
        return Err(RefinementError::Configuration("analysis changed before review").into());
    }
    let base = snapshot
        .corrections
        .as_ref()
        .map(|stored| stored.corrections.clone())
        .unwrap_or_else(|| Corrections::empty(Corrections::clock_of(&snapshot.proposal)));
    let current = match snapshot.corrections {
        Some(_) => base
            .apply_to_transcript(&snapshot.proposal)
            .map_err(|error| RefinementError::Protocol(error.to_string()))?,
        None => CorrectedTranscript::recognized(snapshot.proposal.clone()),
    };
    // Retain the exact same pin/neighbor admission used by the native sheet.
    if prepared
        .review
        .accept(&snapshot.proposal, &current, &base, &[word])
        .is_err()
    {
        return Ok(None);
    }
    let bounds = offer
        .centisecond_bounds
        .ok_or(RefinementError::Configuration(
            "available offer has no bounds",
        ))?;
    Ok(Some(AcceptanceRequest {
        protocol: 1,
        expected_revision: prepared.expected_revision.clone(),
        guard: prepared.selection_guard.clone(),
        word,
        start_cs: bounds.start_cs,
        end_cs: bounds.end_cs,
    }))
}

fn fingerprint(request: &AcceptanceRequest) -> Result<String, serde_json::Error> {
    let mut hash = sha2::Sha256::new();
    hash.update(b"deadpan-word-refinement-1\0");
    hash.update(serde_json::to_vec(request)?);
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn run(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || crate::CliError::Usage(USAGE.into());
    let (path, mut rest) = arguments.split_first().ok_or_else(usage)?;
    let mut word = None;
    let mut accepted = None;
    let mut model_root = None;
    while !rest.is_empty() {
        let [flag, value, tail @ ..] = rest else {
            return Err(usage());
        };
        match *flag {
            "--word" if word.is_none() => word = Some(value.parse::<usize>().map_err(|_| usage())?),
            "--accept" if accepted.is_none() => {
                if value.len() != 64
                    || !value
                        .bytes()
                        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
                {
                    return Err(usage());
                }
                accepted = Some(*value);
            }
            "--models-root" if model_root.is_none() => model_root = Some(PathBuf::from(value)),
            _ => return Err(usage()),
        }
        rest = tail;
    }
    let word = word.ok_or_else(usage)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(Vec::new());
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, cancelled.clone())?);
    }
    let deadline = Instant::now() + Duration::from_millis(deadpan_jobs::refinement::MAX_TIMEOUT_MS);
    let root = match model_root {
        Some(root) => root,
        None => crate::models::default_root()?,
    };
    let reader = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?;
    let receipt = crate::transcription::analysed_receipt(&reader, None)?;
    let words = crate::speech::stored_words(&reader, &receipt.original().content().to_string())
        .ok_or(RefinementError::Configuration(
            "the Original needs a stored transcript first",
        ))?;
    super::validate_language(&words.key.language)?;
    let models = crate::models::installed_refinement_models(&root, &cancelled, deadline)?.ok_or(
        RefinementError::Configuration("install phoneme-ctc-en in Models first"),
    )?;
    let end = word.checked_add(1).ok_or_else(usage)?;
    let prepared = super::prepare(
        &reader,
        &words.key,
        word..end,
        &RefinementRuntime::beside_current_executable()?,
        &models,
        &cancelled,
        deadline,
    )?;
    if words.selection_guard.as_ref() != Some(&prepared.selection_guard) {
        return Err(RefinementError::Configuration(
            "the selected analysis changed before preparation; request refinement again",
        )
        .into());
    }
    let request = acceptance(&prepared, &reader, word)?;
    let id = request.as_ref().map(fingerprint).transpose()?;
    if let Some(accepted) = accepted {
        if id.as_deref() != Some(accepted) {
            return Err(RefinementError::Configuration(
                "this proposal is unavailable or changed; review a fresh proposal before accepting",
            )
            .into());
        }
        let request = request.ok_or(RefinementError::Configuration(
            "there is no acceptable timing offer",
        ))?;
        drop(reader);
        super::check(&cancelled, deadline)?;
        return crate::write_json(&crate::live_project::dispatch_short(
            Path::new(path),
            None,
            crate::live_project::ShortOperation::AcceptWordRefinement {
                request: Box::new(request),
            },
        )?);
    }
    let current = prepared.review.capture();
    let output: Value = json!({"protocol":1,"committed":false,"approximate":true,
        "proposal_id":id,"word":word,"revision_id":prepared.expected_revision,
        "analysis_guard":prepared.selection_guard,"context_words":current.context_words(),
        "context_samples":current.window(),"old_word":words.corrected.transcript.words().get(word),
        "offers":prepared.review.offers(),"diagnostics":prepared.review.diagnostics(),
        "runtime":prepared.runtime,"pack_id":models.pack_id,"pack_version":models.pack_version,
        "dictionary_sha256":models.dictionary_sha256,"phones_sha256":models.phones_sha256,
        "acceptance":"Review the approximate edges, then repeat this command with --accept <proposal_id>. It reruns inference and refuses a changed analysis or offer. Support is uncalibrated model evidence, not a correctness probability."});
    crate::write_json(&output)
}
