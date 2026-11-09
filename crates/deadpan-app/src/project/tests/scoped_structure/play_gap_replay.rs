//! A shared-gap length change from the gap row replays in another play: a
//! saved Macro run and dot each give only the newly chosen play its own gap,
//! and one Undo restores each run.

use std::collections::BTreeMap;

use super::*;

fn gap_length(frames: u32) -> SemanticInstruction {
    SemanticInstruction::SetPlayGap {
        change: deadpan_core::PlayGapChange::Length {
            length: PauseLength::Frames {
                frames: count(frames),
            },
        },
    }
}

/// The length of each play's own gap pause, by play ordinal. Plays that
/// still use the shared default gap are absent.
fn own_gaps(document: &ProjectDocument) -> BTreeMap<u32, i64> {
    let Some(plays) = document.gap_overrides().get(&node("rep")) else {
        return BTreeMap::new();
    };
    plays
        .iter()
        .map(|(iteration, gap)| {
            let NodeKind::Hold { recipe } = &document.nodes()[gap].kind else {
                panic!("an owned gap is a pause");
            };
            (iteration.ordinal, recipe.duration.frames())
        })
        .collect()
}

/// The shared gap and the Repeat's body are untouched.
fn assert_shared(document: &ProjectDocument, before: &ProjectDocument) {
    assert_eq!(document.nodes()[&node("rep")], before.nodes()[&node("rep")]);
    assert_eq!(
        document.nodes()[&node("body")],
        before.nodes()[&node("body")]
    );
    assert!(document.overrides().is_empty());
}

/// One Undo restores `expected` exactly, apart from its revision identity.
fn assert_undoes_to(
    service: &ProjectService,
    document: &ProjectDocument,
    expected: &ProjectDocument,
) -> ProjectDocument {
    let restored = undo(service, document);
    assert_eq!(restored.nodes(), expected.nodes());
    assert_eq!(restored.gap_overrides(), expected.gap_overrides());
    assert_eq!(restored.audio_bindings(), expected.audio_bindings());
    assert_eq!(restored.duration().unwrap(), expected.duration().unwrap());
    restored
}

fn document(update: &ProjectUpdate) -> ProjectDocument {
    (*update.workspace.as_ref().unwrap().document).clone()
}

#[test]
fn a_recorded_play_gap_length_runs_in_play_three_as_its_own_gap_with_one_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open_gapped_plays(&scratch.path().join("gap-macro.deadpan"), 4);
    let before = (*opened.document).clone();
    // 4 plays of 20 frames, three shared 4-frame gaps, then z.
    assert_eq!(before.duration().unwrap().frames(), 102);
    // Recording on play 2's gap row performs the edit, then saves exactly
    // the instruction that succeeded.
    let recorded = structure(
        &service,
        &target(&opened, "body", &[("rep", 1)], 24),
        gap_length(10),
    );
    saved(&recorded);
    let first = document(&recorded);
    assert_eq!(own_gaps(&first), BTreeMap::from([(1, 10)]));
    let workspace = recorded.workspace.as_ref().unwrap();
    let save = command(
        &service,
        ProjectRequest::Macro(Operation::Save {
            id: Id {
                session: workspace.session,
                project: first.project_id().clone(),
                revision: first.revision_id().clone(),
                bank_version: 0,
                request: REQUEST.fetch_add(1, Ordering::Relaxed),
                transcript_guard: None,
            },
            register: 'g',
            program: Arc::new(deadpan_core::SemanticProgram::new(vec![gap_length(10)]).unwrap()),
        }),
    );
    saved(&save);
    let bank = save.registers.as_ref().unwrap().version;
    // Saving adds no history.
    assert_eq!(document(&save).revision_id(), first.revision_id());
    // Play 3 now starts at 20 + 4 + 20 + 10 = 54.
    let workspace = save.workspace.as_ref().unwrap();
    let chosen = target(workspace, "body", &[("rep", 2)], 54);
    let run = command(
        &service,
        ProjectRequest::Macro(Operation::Run {
            id: Id {
                session: chosen.session,
                project: chosen.project.clone(),
                revision: chosen.revision.clone(),
                bank_version: bank,
                request: REQUEST.fetch_add(1, Ordering::Relaxed),
                transcript_guard: None,
            },
            register: 'g',
            count: 1,
            scope: chosen.scope.clone(),
            context: scoped_context(&chosen),
        }),
    );
    saved(&run);
    let ran = document(&run);
    // Only play 3's gap changed; plays 1 and 4 keep nothing of their own.
    assert_eq!(own_gaps(&ran), BTreeMap::from([(1, 10), (2, 10)]));
    assert_eq!(
        ran.gap_overrides()[&node("rep")].get(&play("rep", 1)),
        first.gap_overrides()[&node("rep")].get(&play("rep", 1))
    );
    assert_shared(&ran, &before);
    assert_eq!(ran.duration().unwrap().frames(), 114);
    // The run continues on play 3's new gap at its first frame (54 + 20).
    let committed = run.committed.as_ref().unwrap();
    assert_eq!(
        Some(&committed.scoped.as_ref().unwrap().target.node),
        ran.gap_overrides()[&node("rep")].get(&play("rep", 2))
    );
    assert_eq!(committed.cursor, Some(ProjectFrame(74)));
    // One Undo per run: the run, then the recorded edit.
    let recorded = assert_undoes_to(&service, &ran, &first);
    assert_undoes_to(&service, &recorded, &before);
}

#[test]
fn dot_repeats_a_play_gap_length_in_the_play_chosen_now_with_one_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open_gapped_plays(&scratch.path().join("gap-dot.deadpan"), 4);
    let before = (*opened.document).clone();
    let edited = structure(
        &service,
        &target(&opened, "body", &[("rep", 1)], 24),
        gap_length(10),
    );
    saved(&edited);
    let first = document(&edited);
    let semantic = edited.semantic.as_ref().unwrap();
    assert_eq!(
        semantic.edit.as_ref().unwrap().operation,
        crate::project::semantic::RepeatableEdit::Parameter(gap_length(10))
    );
    // Dot on play 3 (54..74) repeats the change there.
    let workspace = edited.workspace.as_ref().unwrap();
    let mut operation = apply(
        &target(workspace, "body", &[("rep", 2)], 54),
        gap_length(10),
    );
    let Operation::Apply { repeat_version, .. } = &mut operation else {
        unreachable!()
    };
    *repeat_version = Some(semantic.version);
    let dotted = command(&service, ProjectRequest::Macro(operation));
    saved(&dotted);
    let repeated = document(&dotted);
    assert_eq!(own_gaps(&repeated), BTreeMap::from([(1, 10), (2, 10)]));
    assert_shared(&repeated, &before);
    assert_eq!(repeated.duration().unwrap().frames(), 114);
    // Dot again on play 1 (0..20): plays 1 to 3 now own 10-frame gaps and
    // the last play still has none.
    let workspace = dotted.workspace.as_ref().unwrap();
    let version = dotted.semantic.as_ref().unwrap().version;
    let mut operation = apply(&target(workspace, "body", &[("rep", 0)], 0), gap_length(10));
    let Operation::Apply { repeat_version, .. } = &mut operation else {
        unreachable!()
    };
    *repeat_version = Some(version);
    let again = command(&service, ProjectRequest::Macro(operation));
    saved(&again);
    let third = document(&again);
    assert_eq!(
        own_gaps(&third),
        BTreeMap::from([(0, 10), (1, 10), (2, 10)])
    );
    assert_shared(&third, &before);
    // Each dot is one Undo, as is the original edit.
    let undone = assert_undoes_to(&service, &third, &repeated);
    let undone = assert_undoes_to(&service, &undone, &first);
    assert_undoes_to(&service, &undone, &before);
}
