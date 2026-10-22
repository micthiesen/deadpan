//! Transcript-search selection reaches actual verified MP4 export.
//!
//! Words below are synthetic annotation inputs over a qualified real Original.
//! This exercises selection, capture, editing and export, not model accuracy.

use super::*;
use deadpan_analysis::{AnalysedAudio, Transcript, Word};
use deadpan_core::{ProjectDocument, RegisterName, RegisterValue};
use deadpan_store::{AccessMode, ProjectStore, TranscriptKey, single_source::SingleSourceState};
use std::fs;

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

/// Use the same headless semantic path as the app, including its frozen
/// transcript authority and Store publication transaction.
fn macro_operation(
    fixture: &Fixture,
    directory: &Path,
    label: &str,
    operation: Value,
) -> Result<Value> {
    let store = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?;
    let (document, bank) = store.snapshot_with_registers()?;
    drop(store);
    let request = directory.join(format!("transcript-search-{label}.json"));
    fs::write(
        &request,
        serde_json::to_vec_pretty(&json!({
            "protocol":1,
            "project_id":document.project_id(),
            "expected_revision":document.revision_id(),
            "expected_bank_version":bank.version,
            "operation":operation
        }))?,
    )?;
    success(&[
        "macro",
        fixture.package.to_str().ok_or("UTF-8 package")?,
        "--json",
        request.to_str().ok_or("UTF-8 macro request")?,
    ])
}

fn build(directory: &Path) -> Result<Fixture> {
    // This existing public fixture factory calls project create-original,
    // retaining and decoding the full Original through the production path.
    // Unlike shortened range-set fixtures, all 120 pictures remain here.
    let mut fixture = recipes::sdr_source_original(directory, "cfr-bframes", 120)?;
    fixture.name = "transcript-search";
    let store = ProjectStore::open(&fixture.package, AccessMode::ReadWrite)?;
    let before = store.snapshot()?;
    assert_eq!(before.duration()?.frames(), 120);
    assert_eq!(
        serde_json::to_value(before.presentation_basis().frame_rate)?,
        json!({"numerator":30000,"denominator":1001})
    );
    let Some(SingleSourceState::Ready {
        asset,
        baseline_revision,
        qualification,
        ..
    }) = store.single_source_state()?
    else {
        return Err("search fixture requires a ready qualified Original".into());
    };
    assert_eq!(before.revision_id(), &baseline_revision);
    let receipt = store.registered_source(&baseline_revision, &asset)?;
    assert_eq!(receipt.id(), &qualification);
    assert_eq!(
        receipt
            .snapshot()
            .video()
            .ok_or("qualified picture")?
            .index()
            .index()
            .frames()
            .len(),
        120
    );
    let audio = receipt.snapshot().audio().ok_or("qualified audio")?;
    let origin = audio
        .frames()
        .first()
        .ok_or("first audio frame")?
        .valid_start;
    let end = audio.frames().last().ok_or("last audio frame")?.valid_end;
    assert_eq!(
        origin, 0,
        "the literal centisecond oracle starts at source zero"
    );
    assert_eq!(audio.stream().sample_rate, 48_000);
    let duration_cs = u32::try_from(
        (u64::try_from(end - origin)? * 100).div_ceil(u64::from(audio.stream().sample_rate)),
    )?;
    let proposal = Transcript::new(
        AnalysedAudio {
            origin,
            sample_rate: audio.stream().sample_rate,
            duration_cs,
        },
        [(30, 50), (100, 120), (170, 190)]
            .into_iter()
            .enumerate()
            .map(|(segment, (start_cs, end_cs))| {
                Ok(Word {
                    text: "needle".into(),
                    start_cs,
                    end_cs,
                    probability: 0.9,
                    segment: u32::try_from(segment)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    )?;
    let key = TranscriptKey {
        content: receipt.original().content().to_string(),
        audio_stream: audio.stream().stream_index,
        model_sha256: "a".repeat(64),
        language: "en".into(),
        engine: "synthetic-export-annotation".into(),
    };
    store.save_transcript(&key, &proposal)?;
    let frozen = store.transcript_selection(&key)?;
    assert_eq!(frozen.proposal, proposal);
    assert!(frozen.corrections.is_none());
    assert_eq!(store.snapshot()?, before);
    let before_history = store.validate_report(true)?;
    drop(store);
    fs::write(
        directory.join("transcript-search-annotation.json"),
        serde_json::to_vec_pretty(&json!({
            "synthetic_annotation":true,"model_qualification_claim":false,
            "key":key,"proposal":proposal,"selection_guard":frozen.guard,
            "literal_ranges":[[8,15],[29,36],[50,57]],
            "retained_pictures":99,"pasted_pictures":21
        }))?,
    )?;
    macro_operation(
        &fixture,
        directory,
        "save",
        json!({
            "type":"save","register":"m","program":{"instructions":[
                {"type":"select_transcript_search","query":"needle"},
                {"type":"yank","selector":{"type":"visual_selection"},"register":"a"},
                {"type":"cut","selector":{"type":"visual_selection"},"register":"b"},
                {"type":"clear_selection"},
                {"type":"move_frames","forward":true,"count":1000},
                {"type":"paste","register":"b","before":false}
            ]}
        }),
    )?;
    let result = macro_operation(
        &fixture,
        directory,
        "run",
        json!({
            "type":"run","register":"m","parent":before.root(),"cursor":0,"count":1
        }),
    )?;
    let expected_ranges = json!([{"start":8,"end":15},{"start":29,"end":36},{"start":50,"end":57}]);
    let trace = result["trace"].as_array().ok_or("macro trace")?;
    for kind in ["yank", "cut"] {
        let matching = trace
            .iter()
            .filter(|row| row["instruction"]["type"] == kind)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0]["resolved_ranges"], expected_ranges);
        assert_eq!(matching[0]["resolved_nodes"], json!([]));
        assert!(matching[0]["resolved_range"].is_null());
        assert!(matching[0]["resolved_selection"].is_null());
        assert_eq!(matching[0]["capture_set"]["parent"], json!(before.root()));
    }
    let store = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?;
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 120);
    let bank = store.registers()?;
    let mut captures = Vec::new();
    for name in ['a', 'b'] {
        let RegisterValue::EditedSet { set } = bank.entries[&RegisterName::new(name)?].as_ref()
        else {
            return Err("transcript search must capture the complete disjoint union".into());
        };
        assert_eq!(set.revision_id(), before.revision_id());
        assert_eq!(set.scope(), before.root());
        assert_eq!(set.duration().frames(), 21);
        assert_eq!(set.components().len(), 3);
        let captured = store.capture_snapshot_at(set.revision_id())?;
        assert_eq!(captured, before);
        set.validate_capture(&captured)?;
        for (part, pair) in set.components().iter().zip([(8, 15), (29, 36), (50, 57)]) {
            part.validate_capture(&captured)?;
            assert_eq!((part.range().start().0, part.range().end().0), pair);
        }
        captures.push(json!({"register":name.to_string(),"capture":set}));
    }
    let history = store.validate_report(true)?;
    assert_eq!(history.revisions, before_history.revisions + 1);
    assert_eq!(history.verified_by_receipt, 0);
    assert_eq!(history.replayed + 1, history.revisions);
    drop(store);
    let package = fixture.package.to_str().ok_or("UTF-8 package")?;
    success(&[
        "project",
        "undo",
        package,
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let store = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?;
    let undone = store.snapshot()?;
    same_authored(&undone, &before)?;
    assert_eq!(store.registers()?, bank);
    drop(store);
    success(&[
        "project",
        "redo",
        package,
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let store = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?;
    let restored = store.snapshot()?;
    same_authored(&restored, &after)?;
    assert_eq!(store.registers()?, bank);
    let restored_history = store.validate_report(true)?;
    assert_eq!(restored_history.verified_by_receipt, 0);
    assert_eq!(restored_history.replayed + 1, restored_history.revisions);
    drop(store);
    fs::write(
        directory.join("transcript-search-macro.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    fs::write(
        directory.join("transcript-search-captures.json"),
        serde_json::to_vec_pretty(&json!({
            "captures":captures,"before_revision":before.revision_id(),
            "macro_revision":after.revision_id(),"render_revision":restored.revision_id(),
            "history_before_navigation":history,"history_after_redo":restored_history
        }))?,
    )?;
    // This literal oracle is independent of search output and plan inspection.
    // Source [0,120) minus three 7f matches retains 99 pictures, followed by
    // the same three copied windows (21 pictures). Neither hull nor original
    // chronology can satisfy the expected ordering at every output position.
    fixture.expectations = [
        (0, 8),
        (15, 29),
        (36, 50),
        (57, 120),
        (8, 15),
        (29, 36),
        (50, 57),
    ]
    .into_iter()
    .flat_map(|(start, end)| start..end)
    .enumerate()
    .map(|(frame, source_ordinal)| {
        Ok((u64::try_from(frame)?, Expected::Original { source_ordinal }))
    })
    .collect::<Result<Vec<_>>>()?;
    assert_eq!(fixture.expectations.len(), 120);
    fixture.frames = 120;
    fixture.revision = restored.revision_id().to_string();
    fixture.section8_rows = vec!["Discontiguous interval union"];
    fixture.notes = vec!["Qualified full 120-picture cfr-bframes.mp4 Original at 30000/1001. Synthetic needle words at [30,50), [100,120), [170,190) centiseconds select literal Edit [8,15), [29,36), [50,57). One shared host macro selects, yanks, cuts, clears, moves and pastes: 99 retained + 21 copied pictures. One Undo/Redo, reopened historical captures and a portable owned-media copy pass. All 120 emitted pictures are compared to literal independent Original ordinals. Full encoded audio is compared to the canonical preview bus on the origin-based grid, without shifting PCM. This is not model accuracy, independent DSP or listening qualification.".into()];
    // Render the portable copy, exercising retained managed Original ownership
    // and the analysis/capture history without relying on the fixture wrapper.
    let portable = directory.join("transcript-search-portable.deadpan");
    let copied = success(&[
        "project",
        "copy-portable",
        package,
        portable.to_str().ok_or("UTF-8 portable")?,
    ])?;
    assert_eq!(
        copied["portable_copy"]["revision_id"],
        fixture.revision.as_str()
    );
    assert_eq!(copied["portable_copy"]["originals"][0]["from"], "managed");
    let portable_store = ProjectStore::open(&portable, AccessMode::ReadOnly)?;
    same_authored(&portable_store.snapshot()?, &restored)?;
    assert_eq!(portable_store.registers()?, bank);
    assert_eq!(
        portable_store.transcript_selection(&key)?.proposal,
        proposal
    );
    assert!(portable_store.validate_report(true).is_ok());
    drop(portable_store);
    fixture.package = portable;
    Ok(fixture)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn transcript_search_export_preserves_exact_match_union_order_and_complete_audio() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    eprintln!("transcript search Render evidence: {}", root.display());
    let fixture = build(&root.join("fixture"))?;
    check(&fixture)?;
    if skip_without_physical_mac("transcript-search disjoint union encoded Render") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let movie = render(&fixture, &root.join("exports"))?;
    let rendered = started.elapsed();
    let (report, passed) = verify(
        &fixture,
        &movie,
        &fixture.revision,
        &["--every", "1", "--samples", "0:192192"],
    )?;
    check_provenance(&fixture, &report)?;
    assert_eq!(report["summary"]["pictures_checked"], 120);
    assert_eq!(report["summary"]["pictures_failed"], 0);
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    let audio = report["audio"].as_array().ok_or("full canonical audio")?;
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0]["project_samples"], json!([0, 192192]));
    for window in audio {
        assert_eq!(window["flags"], json!([]));
        assert_eq!(window["uncovered_samples"], 0);
        assert_eq!(window["blocks"]["low_snr_blocks"], 0);
        assert_eq!(window["offset_status"], "verified_zero");
        assert_eq!(window["measured_offset_samples"], 0);
    }
    assert!(passed, "{report}");
    let mut row = summary_row(&fixture, &report);
    row["render_seconds"] = json!(rendered.as_secs_f64());
    row["verify_seconds"] = json!((started.elapsed() - rendered).as_secs_f64());
    row["movie"] = json!(movie);
    row["package"] = json!(fixture.package);
    row["revision"] = json!(fixture.revision);
    row["notes"] = json!(fixture.notes);
    fs::write(
        root.join("transcript-search-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::write(
        root.join("transcript-search-summary.json"),
        serde_json::to_vec_pretty(&row)?,
    )?;
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_RESULTS") {
        fs::write(path, serde_json::to_vec_pretty(&row)?)?;
    }
    eprintln!("{row}");
    Ok(())
}
