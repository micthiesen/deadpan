//! Real-media neutral Ungroup after later label and independent-event edits.
use super::*;
use deadpan_core::SoundClockJournal;

pub const FINAL_FRAMES: u64 = 15;
pub const FINAL_SAMPLES: i64 = 24_024;

/// Derive only the documented neutral structural change, independently of the
/// compiled media plan. Every other authored field must remain value-equivalent.
fn only_wrapper_removed(
    before: &ProjectDocument,
    after: &ProjectDocument,
    top: &NodeId,
    group: &NodeId,
    owner: &NodeId,
) -> Result {
    assert_eq!(before.duration()?, after.duration()?);
    assert!(!after.nodes().contains_key(group));
    let mut nodes = before.nodes().clone();
    let removed = nodes.remove(group).ok_or("neutral wrapper")?;
    assert!(removed.audio_treatments.is_empty());
    assert_eq!(
        removed.audio_edges,
        deadpan_core::AudioEdgePolicies::default()
    );
    assert!(
        matches!(&removed.kind, NodeKind::Sequence { children } if children.as_slice() == std::slice::from_ref(owner))
    );
    let NodeKind::Sequence { children } = &mut nodes.get_mut(top).ok_or("ancestor Top")?.kind
    else {
        return Err("Top is not an ordinary Sequence".into());
    };
    let slot = children
        .iter()
        .position(|child| child == group)
        .ok_or("Top child wrapper")?;
    children[slot] = owner.clone();
    assert_eq!(after.nodes(), &nodes);

    let bindings = before.audio_bindings();
    let after_bindings = after.audio_bindings();
    assert_eq!(after_bindings.timings(), bindings.timings());
    assert_eq!(after_bindings.bindings(), bindings.bindings());
    assert_eq!(after_bindings.gap_bindings(), bindings.gap_bindings());
    let mut clocks = bindings.sound_clocks().clone();
    let mut changed = 0;
    for events in clocks.values_mut() {
        for journal in events.values_mut() {
            assert_eq!(journal.scope(), top);
            let mut references = Vec::new();
            for reference in journal.clocks() {
                let mut groups = reference.introduced_groups().clone();
                assert!(
                    groups.remove(group),
                    "every affected reference still treats G as introduced"
                );
                changed += 1;
                references.push(reference.clone().with_introduced_groups(groups)?);
            }
            *journal = SoundClockJournal::new(journal.scope().clone(), references)?;
        }
    }
    assert!(changed > 0);
    assert_eq!(after_bindings.sound_clocks(), &clocks);
    let mut lineage = before.audio_lineage().clone();
    lineage.remove(group);
    assert_eq!(after.audio_lineage(), &lineage);
    let mut old = serde_json::to_value(before)?;
    let mut new = serde_json::to_value(after)?;
    for value in [&mut old, &mut new] {
        let object = value.as_object_mut().ok_or("document object")?;
        for field in ["revision_id", "nodes", "audio_bindings", "audio_lineage"] {
            object.remove(field);
        }
    }
    assert_eq!(
        new, old,
        "all other authored state, including marks and sound recipes, survives"
    );
    Ok(())
}

pub(super) fn finish(
    mut project: Project,
    directory: &Path,
    owner: NodeId,
    top: NodeId,
    group: NodeId,
    grouped_pcm: &CanonicalPcm,
) -> Result<Fixture> {
    let grouped = project.document()?;
    let sound = SoundId::new("aligned-attached")?;
    let grouped_bindings = grouped.audio_bindings().clone();
    assert!(grouped.nodes()[&group].audio_treatments.is_empty());
    project.apply(|_, _, _| {
        Ok(Command::Rename {
            node: group.clone(),
            label: "Temporary wrapper after later edits".into(),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::Rename {
            node: node("aligned-tail")?,
            label: "Later untouched sibling".into(),
        })
    })?;
    let current = project.document()?;
    let mut later_event = current.beat_sounds()[&owner][&sound].clone();
    later_event.label = "Later edited independent finite event".into();
    later_event.treatments = later_event.treatments.with_trim(GainDb::new(-3000)?)?;
    project.apply(|_, _, _| {
        Ok(Command::SetBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
            event: later_event.clone(),
        })
    })?;
    let before = project.document()?;
    assert_eq!(before.audio_bindings(), &grouped_bindings);
    whole_scope(&before, &owner, &top, &sound)?;
    assert_eq!(before.beat_sounds()[&owner][&sound], later_event);
    assert!(before.nodes()[&group].audio_treatments.is_empty());
    let before_pcm = read_exact(&project.package, &[256])?;
    assert_eq!(
        before_pcm.authored_bits.len(),
        usize::try_from(FINAL_SAMPLES)?
    );
    same_exact(
        &before_pcm,
        &read_exact(&project.package, &[37, 173, 251])?,
        "later edits before neutral Ungroup",
    );
    let event_start = usize::try_from(boundary(9)?)?
        .checked_add(37)
        .ok_or("event start overflow")?;
    let event_end = event_start.checked_add(4096).ok_or("event end overflow")?;
    let later_edit_changed_samples = before_pcm.authored_bits[event_start..event_end]
        .iter()
        .zip(&grouped_pcm.authored_bits[event_start..event_end])
        .filter(|(later, earlier)| later != earlier)
        .count();
    assert!(
        later_edit_changed_samples > 0,
        "the later event trim must affect actual admitted PCM"
    );
    retain(directory, "neutral-ungroup-before-document.json", &before)?;
    retain(directory, "neutral-ungroup-before-pcm.json", &before_pcm)?;
    retain(
        directory,
        "neutral-ungroup-later-edit-control.json",
        &json!({
            "checked_project_samples":[event_start,event_end],
            "changed_samples":later_edit_changed_samples,
            "event_trim_millidecibels":-3000,
            "label_edits":["Temporary wrapper after later edits","Later untouched sibling"],
            "event_off_control":"aligned-event-control.json",
            "outside_interval_silence_claim":false,
        }),
    )?;
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let history_before = store.validate_report(true)?;
    drop(store);
    let operation = json!({"type":"apply","parent":top,"cursor":14,"selected_child":group,
        "program":{"instructions":[{"type":"ungroup"}]}});
    let dry = project.macro_operation(operation.clone(), true)?;
    fs::write(
        directory.join("neutral-ungroup-dry-run.stdout.json"),
        &dry.stdout,
    )?;
    fs::write(
        directory.join("neutral-ungroup-dry-run.stderr.txt"),
        &dry.stderr,
    )?;
    assert!(
        dry.status.success(),
        "neutral Ungroup dry-run: {}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    assert_eq!(
        store.snapshot()?,
        before,
        "dry-run must not author a revision"
    );
    assert_eq!(store.registers()?, bank, "dry-run must not change the bank");
    assert_eq!(
        store.validate_report(true)?,
        history_before,
        "dry-run must not add history"
    );
    drop(store);
    let applied = project.macro_operation(operation, false)?;
    fs::write(
        directory.join("neutral-ungroup-macro.stdout.json"),
        &applied.stdout,
    )?;
    fs::write(
        directory.join("neutral-ungroup-macro.stderr.txt"),
        &applied.stderr,
    )?;
    assert!(
        applied.status.success(),
        "neutral Ungroup after later edits: {}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let trace: Value = serde_json::from_slice(&applied.stdout)?;
    assert_eq!(trace["trace"][0]["resolved_parent"], json!(top));
    assert_eq!(
        trace["trace"][0]["resolved_range"],
        json!({"start":9,"end":13})
    );
    assert_eq!(
        trace["trace"][0]["resolved_selection"],
        json!({"type":"child","node":group})
    );
    assert_eq!(trace["context"]["selected_child"], json!(owner));
    assert_eq!(trace["context"]["cursor"], 9);
    let after = project.document()?;
    only_wrapper_removed(&before, &after, &top, &group, &owner)?;
    assert_eq!(after.duration()?.frames(), i64::try_from(FINAL_FRAMES)?);
    let after_pcm = read_exact(&project.package, &[251, 173, 37])?;
    same_exact(&before_pcm, &after_pcm, "complete neutral Ungroup");
    cold_exact(
        &project.package,
        &before_pcm,
        &[boundary(9)?, boundary(11)?, boundary(13)?, boundary(15)?],
        "neutral Ungroup cold seams",
    )?;
    retain(directory, "neutral-ungroup-after-document.json", &after)?;
    retain(directory, "neutral-ungroup-after-pcm.json", &after_pcm)?;
    let restored = durable_roundtrip(
        &project,
        &before,
        &after,
        &before_pcm,
        &before_pcm,
        "neutral Ungroup after later edits",
    )?;
    only_wrapper_removed(&before, &restored, &top, &group, &owner)?;
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?, restored);
    assert_eq!(store.registers()?, bank);
    let history = store.validate_report(true)?;
    assert_eq!(history.verified_by_receipt, 0);
    assert_eq!(history.replayed + 1, history.revisions);
    drop(store);
    same_exact(
        &before_pcm,
        &read_exact(&project.package, &[173, 251, 37])?,
        "reopened neutral Ungroup complete PCM",
    );
    let portable = directory.join("neutral-ungroup-portable.deadpan");
    let copy = success(&[
        "project",
        "copy-portable",
        project.path(),
        portable.to_str().ok_or("UTF-8 portable package")?,
    ])?;
    let store = ProjectStore::open(&portable, AccessMode::ReadOnly)?;
    same_authored(&store.snapshot()?, &restored)?;
    assert_eq!(store.registers()?, bank);
    let portable_history = store.validate_report(true)?;
    assert_eq!(portable_history.verified_by_receipt, 0);
    assert_eq!(portable_history.replayed + 1, portable_history.revisions);
    drop(store);
    same_exact(
        &before_pcm,
        &read_exact(&portable, &[37, 173, 251])?,
        "portable neutral Ungroup complete PCM",
    );
    cold_exact(
        &portable,
        &before_pcm,
        &[boundary(9)?, boundary(11)?, boundary(13)?, boundary(15)?],
        "portable neutral Ungroup cold seams",
    )?;
    retain(directory, "neutral-ungroup-portable-copy.json", &copy)?;
    retain(
        directory,
        "neutral-ungroup-history.json",
        &json!({
            "before":history_before,"after_reopen":history,"after_portable":portable_history,
            "source":owner,"retained_scope":top,"removed_group":group,
            "final_revision":restored.revision_id(),
            "canonical_samples_compared":FINAL_SAMPLES,"channels":2,
            "authored_limited_and_limiter_gain_bits_equal":true,
            "later_event_edit_changed_samples":later_edit_changed_samples,
            "independent_DSP_claim":false,"listening_claim":false,
        }),
    )?;
    let mut pictures = vec![Expected::Background; 9];
    pictures.extend((27..31).map(original));
    pictures.extend([Expected::Background; 2]);
    assert_eq!(pictures.len(), usize::try_from(FINAL_FRAMES)?);
    let mut fixture = project.finish(
        vec!["Neutral Ungroup inside an old independent sound scope"],
        pictures.into_iter().enumerate().map(|(frame, picture)| (frame as u64, picture)).collect(),
        vec!["Actual qualified cfr-bframes Original [27,31), Top EQ, finite independent Original sound [48000,52096) at +37 samples and event EQ. A five-frame root prefix captures Top before neutral G is introduced. Later group/sibling labels and event trim are committed before public semantic Ungroup promotes the Source. The event-off and later-trim controls prove the independent sound is active. Full authored/limited PCM and limiter gain bits remain identical through Ungroup, durable Undo/Redo, reopen and owned portable copy. All timing tables, recipes and journal relationships remain unchanged except removal of G's introduced marker. The literal 15-picture oracle and complete emitted-file audio window are verified. No independent DSP or listening claim.".into()],
    )?;
    fixture.package = portable;
    Ok(fixture)
}
