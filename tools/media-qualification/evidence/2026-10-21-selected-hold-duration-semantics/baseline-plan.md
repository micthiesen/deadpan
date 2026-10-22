# Selected several-play Hold duration: smallest native baseline

Scratch plan/code sketch only. No repository edits, Cargo, formatting, Git or
runtime claims. Execute against the completed mixed milestone after commit.

Proposed child: preview/scoped/model/tests/selected_duration.rs
Registration in model/tests.rs: mod selected_duration;
Test: native_selected_hold_duration_offers_entry_after_zero_net_compound_control

## Exact public fixture setup

Copy/extract the public qualified setup in model/tests/mixed_duration.rs::exercise
through its reopened Workspace, immediately before cursor=ProjectFrame(19).
Do not call exercise(false): it also executes its own baseline and returns no
Workspace. Keep the existing tests unchanged if extracting a test-only helper.

That setup uses current public APIs:
- ProjectService::start with tempfile ProjectLibrary.
- ProjectRequest::CreateFromSource offset-bframes.mp4, Managed ownership.
- Wait for ImportStage::Complete, reopen, SingleSourceState::Ready.
- ProjectRequest::PrepareRoomTone, actual receipt, Original ordinals1..4.
- Holds use actual Freeze asset/span.start() plus returned qualified RoomTone.
- Close service, ProjectStore::open ReadWrite, delete initial Source, insert
  lead3 / shared-outer / tail3 using Command::Insert; reopen through service.
- Reuse model/tests.rs::{repeat,sequence,overrides,frames,node,revision} and the
  mixed_duration.rs local command/seed_command helpers. Do not mutate Workspace
  snapshots, construct fake media or add a production API.

The existing fixture is already precisely the needed 3x2 structure:
  root lead3, shared-outer, tail3
  shared-outer iterations3, literal child shared-inner
    outer play2 owns [before2,pause7,after2] =11
  shared-inner iterations2, literal shared-body [before2,pause2,after2] =6
    inner play1 owns inner-owned [before2,pause4,after2] =8
  no gaps/Retime/root sounds/beat sounds.
Repeat allocation is its name, ordinals0..N-1. Root3+14*2+11+3=45.

## Independent counterexample and expected OLD failure

Select outer Default / inner plays1,2, then set both Hold lengths to3.
Default Hold2->3 contributes +2 across outer0,1; owned Hold4->3 contributes -2.
Root returns45, but both recipes/internal boundaries change. First leaf root47,
second leaf root45. The outer1 primary start moves19->20->19. Exact node
pools4 then0, marks0 for both. Distinct revisions.

Before starts: selected default Holds5/19; owned Holds11/25; outer2 starts31,
tail starts42. After both: default starts5/19, owned starts12/26, outer2/tail
unchanged. This catches root-delta-only no-op detection and shared-ID mistakes.

Old final assertion: State::hold_parameter(workspace) should return
Some("hold-duration 2f"), but current model.rs::hold_parameter_in explicitly
returns None whenever Level.also or mapped_also is nonempty. Existing separate
hold_duration_available() should already be TRUE; assert that as a prefix.
2f is primary current-value prefill, not a claim members share a duration.

## Public API witness body sketch

Call the following body with the reopened qualified Workspace while the fixture
TempDir and service are alive. Current APIs suffice; no proposed batch types.
The scalar mixed command is already present in the OLD completed milestone.

    fn selected_duration_witness(workspace: &Workspace) {
        use deadpan_core::{
            Command, CommandRequest, LeafEdit, OccurrenceIdentities,
            ResolvedStep, ResolvedTransaction, RepeatEditBranch,
            prepare_scoped_hold_duration, apply_with_result,
        };
        let before = &*workspace.document;
        before.validate().unwrap();
        assert_eq!(workspace.plan.duration().frames(), 45);
        assert!(before.sounds().is_empty() && before.beat_sounds().is_empty());
        assert!(before.nodes().values().all(|n| !matches!(n.kind, NodeKind::Retime{..})));
        let cursor = ProjectFrame(19);
        let mut state = State::new(
            workspace, SequenceScope::default(), node("shared-outer"), cursor,
        ).unwrap();
        assert!(state.enter(workspace).unwrap()); // outer Default
        state.switch_plays(workspace, &[1,2]).unwrap();
        assert!(state.enter(workspace).unwrap());
        state.select(workspace,1).unwrap();
        let captured = state.target(workspace,cursor).unwrap();
        captured.validate(workspace).unwrap();
        assert_eq!(captured.target.node,node("shared-pause"));
        assert_eq!(captured.also.len(),1);
        assert_eq!(captured.also[0].node,node("inner-pause"));
        assert_eq!(captured.target.repeats.len(),2);
        assert_eq!(captured.target.repeats[0].branch,RepeatEditBranch::Default);
        assert_eq!(captured.also[0].repeats[0],captured.target.repeats[0]);
        assert_ne!(captured.target.repeats[1],captured.also[0].repeats[1]);
        for t in std::iter::once(&captured.target).chain(&captured.also) {
            t.validate(before).unwrap();
            assert!(matches!(t.repeats[1].branch,RepeatEditBranch::Play{..}));
        }
        let shown = captured.presentation.as_ref().unwrap();
        assert!(captured.target.matches_instance(before,shown).unwrap());
        assert_eq!(state.presentation(workspace).unwrap().unwrap().frames,19..21);
        assert!(workspace.plan.picture(cursor).unwrap().framing.iter()
            .any(|layer| &layer.instance==shown));

        // Derive other presentation from explicit stable Play and the captured
        // outer representative, independently of any sampled root cursor.
        let mut other_shown = shown.clone();
        other_shown.node = captured.also[0].node.clone();
        other_shown.repeats[1].iteration = match &captured.also[0].repeats[1].branch {
            RepeatEditBranch::Play{iteration} => iteration.clone(),
            RepeatEditBranch::Default => panic!("selected Play required"),
        };
        assert!(captured.also[0].matches_instance(before,&other_shown).unwrap());

        let make_request = |doc:&ProjectDocument, target:ScopedNodeTarget,
                            allocation:&str, expected_nodes:usize| {
            let needs=doc.scoped_hold_duration_requirements(&target,frames(3)).unwrap();
            assert_eq!((needs.nodes,needs.marks),(expected_nodes,0));
            CommandRequest {
                project_id:doc.project_id().clone(),
                expected_revision:doc.revision_id().clone(),
                new_revision:revision(allocation),
                command:Command::SetScopedHoldDuration {
                    target,duration:frames(3),
                    identities:OccurrenceIdentities {
                        nodes:(0..needs.nodes).map(|n|
                            node(&format!("{allocation}-node-{n}"))).collect(),
                        marks:vec![],
                    },
                },
            }
        };
        let first=make_request(before,captured.target.clone(),"selected-first",4);
        let p1=prepare_scoped_hold_duration(before,&first,Some(shown)).unwrap();
        assert_eq!(p1.document.duration().unwrap().frames(),47);
        assert_eq!(p1.boundary.as_ref().unwrap().frame,ProjectFrame(20));
        let other=p1.map_target(before,&captured.also[0]).unwrap();
        let other_shown=p1.map_instance(before,&other_shown).unwrap();
        assert_eq!(other.node,node("inner-pause"));
        let second=make_request(&p1.document,other,"selected-second",0);
        let p2=prepare_scoped_hold_duration(&p1.document,&second,Some(&other_shown)).unwrap();
        assert_eq!(p2.document.duration().unwrap().frames(),45);
        assert_eq!(p2.boundary.as_ref().unwrap().frame,ProjectFrame(26));
        for owner in ["outer-owned-two","outer-owned-two-before",
                      "outer-owned-two-pause","outer-owned-two-after","root-tail"] {
            assert_eq!(p2.document.nodes()[&node(owner)],before.nodes()[&node(owner)]);
        }
        let requested=CommandRequest {
            project_id:before.project_id().clone(),
            expected_revision:before.revision_id().clone(),
            new_revision:revision("selected-final"),
            command:Command::Compound {
                transaction:ResolvedTransaction::new(0,BTreeMap::new(),vec![
                    ResolvedStep::Edit{edit:LeafEdit::new(first.new_revision,first.command).unwrap()},
                    ResolvedStep::Edit{edit:LeafEdit::new(second.new_revision,second.command).unwrap()},
                ]).unwrap(),
            },
        };
        let (transaction,after)=apply_with_result(before,&requested).unwrap();
        assert_eq!(transaction.duration_delta,0);
        assert!(!transaction.changed_ids.is_empty());
        assert_eq!(after.nodes(),p2.document.nodes());
        assert_eq!(after.audio_bindings(),p2.document.audio_bindings());
        assert_eq!(transaction.inverse.apply(&after).unwrap(),*before);
        assert_eq!(ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),after);

        // No published service mutation occurred during the independent control.
        assert_eq!(*workspace.document,*before);
        // Generic one-play framing and scalar mixed authority remain restricted.
        assert!(captured.semantic_framing(workspace).is_err());
        assert!(captured.semantic_mixed_hold_duration(workspace).is_err());

        assert!(state.hold_duration_available()); // expected OLD positive prefix
        assert_eq!(state.hold_parameter(workspace).as_deref(),
                   Some("hold-duration 2f")); // ONLY intended OLD failure
    }

## Evidence requirements

All qualified setup, exact selected authority/presentation,4/0 pools,
45->47->45, changed recipes/owned peers, full actual Compound, inverse/JSON
prefixes must pass before the final public native assertion. Capture source
identities, executed binary SHA, before/request/control-after JSON and runtime
log in scratch. A build/setup failure or unrelated semantic guard is not this
baseline. No runtime claim is made here.

This establishes discoverability/entry suppression, not a service mutation
failure. After implementation, follow it with actual atomic Apply, mapped_also,
recording/dot/counts, durable store and independent PCM/export qualification.
