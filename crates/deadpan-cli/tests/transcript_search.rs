#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Real decoded media, synthetic transcript annotations, shared public host API.
use deadpan_analysis::{AnalysedAudio, Transcript, Word};
use deadpan_cli::macros::{self, Request};
use deadpan_core::*;
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_store::{
    AccessMode, ProjectStore, TranscriptKey,
    original_media::{OriginalMediaLimits, OriginalOwnership},
    single_source::SingleSourceInitialization,
    source_registration::PreparedSourceRegistration,
};
use serde_json::json;
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn create(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("guard.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("guard")?,
        revision("initial"),
        node("root"),
    )?;
    Ok((
        path.clone(),
        ProjectStore::create_single_source(&path, &document)?,
    ))
}

fn ready(parent: &Path) -> Result<(PathBuf, ProjectStore, TranscriptKey, Transcript)> {
    let (path, mut store) = create(parent)?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/offset-bframes.mp4")
        .canonicalize()?;
    let limits = OriginalMediaLimits::new(2_000_000, Duration::from_secs(10))?;
    let record = store
        .retain_original(&fixture, OriginalOwnership::Managed, limits, &active())?
        .record;
    let mut original =
        store
            .original_import_handle()?
            .snapshot_original(&record, limits, &active())?;
    let input = VerifiedSourceInput::copy_verified(
        &mut original,
        SourceContentIdentity::new(record.sha256(), record.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active(),
    )?;
    let video = SourceSession::open_input(
        input.clone(),
        asset("decode"),
        SourceSessionLimits::default(),
        &active(),
    )?;
    let audio = AudioSession::open_input(input, 1, AudioSessionLimits::default(), &active())?;
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio))?;
    let source = PreparedSourceRegistration::from_decoded(original, &decoded, &active())?;
    let measured = source.receipt().snapshot().audio().unwrap();
    let key = TranscriptKey {
        content: source.receipt().original().content().to_string(),
        audio_stream: measured.stream().stream_index,
        model_sha256: "a".repeat(64),
        language: "en".into(),
        engine: "guard-test".into(),
    };
    let first = measured.frames().first().unwrap().valid_start;
    let last = measured.frames().last().unwrap().valid_end;
    let duration_cs = u32::try_from(
        (u64::try_from(last - first)? * 100).div_ceil(u64::from(measured.stream().sample_rate)),
    )?;
    let proposal = Transcript::new(
        AnalysedAudio {
            origin: first,
            sample_rate: measured.stream().sample_rate,
            duration_cs,
        },
        vec![
            Word {
                text: "hello".into(),
                start_cs: 25,
                end_cs: 35,
                probability: 0.9,
                segment: 0,
            },
            Word {
                text: "world".into(),
                start_cs: 45,
                end_cs: 55,
                probability: 0.8,
                segment: 0,
            },
        ],
    )?;
    store.initialize_prepared_source(
        &SingleSourceInitialization {
            expected_revision: revision("initial"),
            new_revision: revision("baseline"),
            new_asset_id: asset("original"),
            node: node("full-original"),
            label: "The original".into(),
        },
        &source,
        &active(),
    )?;
    store.save_transcript(&key, &proposal)?;
    Ok((path, store, key, proposal))
}

fn request(store: &ProjectStore, instructions: serde_json::Value, id: &str) -> Result<Request> {
    let document = store.snapshot()?;
    Ok(Request::from_json(&serde_json::to_vec(&json!({
        "protocol":1,"project_id":document.project_id(),"expected_revision":document.revision_id(),
        "expected_bank_version":store.register_version()?,"operation":{"type":"apply",
        "program":{"instructions":instructions},"parent":document.root(),"cursor":0,
        "new_revision":id}
    }))?)?)
}

#[test]
fn complete_corrected_phrase_shared_host_preview_cut_copy_reopen_and_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = ready(scratch.path())?;
    let before = store.snapshot()?;
    let request = request(
        &store,
        json!([
            {"type":"select_transcript_search","query":"HELLO wor"},
            {"type":"yank_selection","register":"a"},
            {"type":"cut_selection","register":"b"}
        ]),
        "search-cut",
    )?;
    let prepared = macros::prepare(&store, &request)?;
    let plan = prepared.plan.as_ref().unwrap();
    assert!(plan.transcript_guard.is_some());
    assert!(plan.request.is_some());
    let selected = match plan.context.visual_selection.as_ref() {
        None => match prepared
            .final_bank
            .entries
            .get(&RegisterName::new('a')?)
            .unwrap()
            .as_ref()
        {
            RegisterValue::EditedSet { set } => set.clone(),
            other => panic!("expected a complete phrase union, got {other:?}"),
        },
        other => panic!("cut must clear selection: {other:?}"),
    };
    assert_eq!(selected.components().len(), 1);
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.register_version()?, 0);
    let removed = before.duration()?.frames() - plan.document.duration()?.frames();
    assert!(removed > 0);
    let result = macros::commit(&mut store, &prepared)?;
    assert_eq!(result.committed_revision, Some(revision("search-cut")));
    assert_eq!(
        store.snapshot()?.duration()?.frames(),
        before.duration()?.frames() - removed
    );
    let bank = store.registers()?;
    let RegisterValue::EditedSet { set: cut } = bank.entries[&RegisterName::new('b')?].as_ref()
    else {
        panic!("cut union missing")
    };
    assert_eq!(selected.duration(), cut.duration());
    assert_eq!(
        selected
            .components()
            .iter()
            .map(|part| part.range())
            .collect::<Vec<_>>(),
        cut.components()
            .iter()
            .map(|part| part.range())
            .collect::<Vec<_>>()
    );
    let mut changed = proposal.words().to_vec();
    changed[0].text = "different".into();
    store.save_transcript(&key, &Transcript::new(proposal.audio(), changed)?)?;
    store.undo(&revision("search-cut"), revision("search-undo"))?;
    assert_eq!(store.snapshot()?.duration()?, before.duration()?);
    store.redo(&revision("search-undo"), revision("search-redo"))?;
    assert_eq!(store.registers()?, bank);
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    reopened.validate()?;
    assert_eq!(
        reopened.snapshot()?.duration()?.frames(),
        before.duration()?.frames() - removed
    );
    Ok(())
}

#[test]
fn analysis_replacement_after_prepare_refuses_selection_yank_and_cut_without_writes() -> Result {
    for (kind, instructions) in [
        (
            "select",
            json!([{ "type":"select_transcript_search","query":"hello world" }]),
        ),
        (
            "yank",
            json!([{ "type":"yank","selector":{"type":"transcript_search","query":"hello world"},"register":"a" }]),
        ),
        (
            "cut",
            json!([{ "type":"cut","selector":{"type":"transcript_search","query":"hello world"},"register":"a" }]),
        ),
    ] {
        let scratch = tempfile::tempdir()?;
        let (_, mut store, key, proposal) = ready(scratch.path())?;
        let prepared = macros::prepare(&store, &request(&store, instructions, kind)?)?;
        let mut changed = proposal.words().to_vec();
        changed[0].text = "hullo".into();
        store.save_transcript(&key, &Transcript::new(proposal.audio(), changed)?)?;
        let before = store.snapshot()?;
        let bank = store.registers()?;
        let history = store.history_availability()?;
        assert!(macros::commit(&mut store, &prepared).is_err(), "{kind}");
        assert_eq!(store.snapshot()?, before);
        assert_eq!(store.registers()?, bank);
        assert_eq!(store.history_availability()?, history);
    }
    Ok(())
}

#[test]
fn native_pinned_key_survives_another_key_and_missing_phrase_never_falls_back() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, store, key, proposal) = ready(scratch.path())?;
    let pinned = store.transcript_selection(&key)?.guard;
    let mut other = key.clone();
    other.model_sha256 = "0".repeat(64);
    let mut changed = proposal.words().to_vec();
    changed[0].text = "hullo".into();
    store.save_transcript(&other, &Transcript::new(proposal.audio(), changed)?)?;
    let document = store.snapshot()?;
    let bank = store.registers()?;
    let context = SemanticContext {
        parent: document.root().clone(),
        cursor: ProjectFrame(0),
        selected_child: None,
        visual_selection: None,
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
    };
    let program = SemanticProgram::new(vec![SemanticInstruction::SelectTranscriptSearch {
        query: SemanticTranscriptQuery::new("hello world")?,
    }])?;
    let plan = macros::plan_program_with_transcript_guard(
        &store,
        &document,
        &bank,
        &context,
        &program,
        revision("pinned"),
        Some(&pinned),
    )?;
    assert_eq!(plan.transcript_guard, Some(pinned));
    assert!(matches!(
        plan.context.visual_selection,
        Some(SemanticVisualSelection::TranscriptSearch { .. })
    ));
    let absent = request(
        &store,
        json!([{ "type":"cut","selector":{"type":"transcript_search","query":"never found"},"register":"a" }]),
        "absent",
    )?;
    assert!(macros::prepare(&store, &absent).is_err());
    assert_eq!(store.snapshot()?, document);
    assert_eq!(store.registers()?, bank);
    Ok(())
}
