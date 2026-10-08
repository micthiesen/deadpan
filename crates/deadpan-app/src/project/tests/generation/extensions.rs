//! Both definition edges use the normal native job and acceptance lifecycle.
use super::*;
use deadpan_jobs::{ConditioningMode, GenerationPlan};

#[test]
fn automatic_edge_extensions_become_ready_without_editing_and_accept_explicitly() {
    if !synthetic_ready_available() {
        eprintln!("skipped: synthetic Ready tools unavailable");
        return;
    }
    for from_left in [true, false] {
        let fixture = project_with_pause_at(scripted(ready_script()), Some(from_left));
        let before = fixture.workspace.document.clone();
        let started = generation(&fixture.service, start(&fixture, 1));
        assert!(refusal(&started).is_none(), "{:?}", refusal(&started));
        let finished = job_until(&fixture.service, |job| !job.running());
        let state = finished.generation.unwrap();
        let job = state.job.unwrap();
        assert!(
            matches!(job.outcome, Some(Outcome::Ready(_))),
            "{:?}",
            job.outcome
        );
        assert_eq!(
            job.operation,
            Some(if from_left {
                ConditioningMode::ExtendFromLeft
            } else {
                ConditioningMode::ExtendFromRight
            })
        );
        assert_eq!(job.opposite_boundary_present, Some(false));
        assert!(matches!(job.plan, Some(GenerationPlan::Extension(_))));
        assert_eq!(
            *finished.workspace.unwrap().document,
            *before,
            "Ready must preserve the fallback and revision"
        );
        let candidate = state.candidates[&ordinary(&fixture.hold)].clone();
        assert!(matches!(
            candidate.variants[0].receipt.plan(),
            GenerationPlan::Extension(_)
        ));
        let previewed = generation(
            &fixture.service,
            GenerationOperation::Preview {
                presentation: None,
                ticket: 2,
                session: fixture.workspace.session,
                revision: before.revision_id().clone(),
                request: candidate.request.clone(),
                attempt: candidate.selected.clone(),
                draft: 42,
            },
        );
        assert!(refusal(&previewed).is_none(), "{:?}", refusal(&previewed));
        assert_eq!(*previewed.workspace.as_ref().unwrap().document, *before);
        let preview = previewed
            .generation
            .unwrap()
            .preview
            .expect("issued preview");
        let cursor = preview.range().start();
        let accepted = generation(
            &fixture.service,
            GenerationOperation::Accept {
                scoped: None,
                session: fixture.workspace.session,
                revision: before.revision_id().clone(),
                request: candidate.request,
                attempt: candidate.selected,
                hold: fixture.hold.clone(),
                cursor,
                scope: SequenceScope::default(),
            },
        );
        assert!(accepted.error.is_none(), "{:?}", accepted.error);
        let workspace = accepted.workspace.unwrap();
        let NodeKind::Hold { recipe } = &workspace.document.nodes()[&fixture.hold].kind else {
            panic!("Hold remains");
        };
        let HoldVideo::Generated { accepted } = &recipe.video else {
            panic!("explicit acceptance changes pictures");
        };
        assert!(matches!(
            accepted.artifact.sampling,
            deadpan_core::GeneratedSamplingMap::Extension(_)
        ));
        assert_eq!(workspace.plan.duration(), fixture.workspace.plan.duration());

        // Lengthening accepted Extension footage commits its fallback first,
        // then uses the captured operation in the automatic preparation queue.
        let longer = command(
            &fixture.service,
            edit_request(
                &workspace,
                ProjectEdit::HoldDuration {
                    node: fixture.hold.clone(),
                    duration: FrameDuration::new(18).unwrap(),
                },
            ),
        );
        assert!(longer.error.is_none(), "{:?}", longer.error);
        let extended = longer.workspace.unwrap();
        assert!(matches!(&extended.document.nodes()[&fixture.hold].kind,
            NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Freeze { .. })));
        let replaced = job_until(&fixture.service, |job| {
            job.revision == *extended.document.revision_id() && !job.running()
        });
        let replacement = replaced.generation.unwrap().job.unwrap();
        assert!(
            matches!(replacement.outcome, Some(Outcome::Ready(_))),
            "{:?}",
            replacement.outcome
        );
        assert_eq!(replacement.operation, job.operation);
        assert!(matches!(
            replacement.plan,
            Some(GenerationPlan::Extension(_))
        ));
        assert_eq!(replaced.workspace.unwrap().document, extended.document);
    }
}
