# Native edited slices review

Base: 41c540aab86af1414bfe73b1378694659bb6763d. Checkout shared, root owns Cargo.
No user/reserved window inspection or native interaction during this milestone yet.

## Reviews

- Independent backend review: no actionable defect in store opaque admission,
  historical media, Arc seals, closure/reopening, cached batch revocation or
  preserved committed warm PCM. No tests run by reviewer.
- Picture worker reviewed root UI and service independently of its own changes.
  Three findings fixed by root:
  1. Rejected newer y in Sounds could leave old pending copy alive. Every
     CopyMoment action now supersedes pending intent before focus guards.
  2. Successful placement receipt consumed Edit selection before verifying the
     refreshed workspace. Clear only with matching visible saved revision.
  3. Coalesced copy success could overwrite saved-refresh warning. Accept the
     register but preserve visible selection/message while refresh is absent.
  Production replay contains explicit delivery-order/simulated receipt checks.
- Root found missing pending-picture invalidation while a refined copied source
  waits for admission. Cancel pending decode, keep last displayed picture.
- Root found old endpoint error text showing during new-source preparation.
  Picture worker now gates error display on current identity, preserving targets.
- Independent picture review found full receipt reconstruction on every warm
  picture. Worker restored existing hot checks and moved full reconstruction to
  exact immutable view cache miss. Added real source/index counter regression;
  reviewer confirmation and focused runtime result pending.

## Retained failures and results

- Backend report/logs in backend/: store40 pass; playback59/60 then corrected
  impulse-range fixture and proposal7/7 pass; strict store/playback Clippy pass.
  Broad warm revocation interpretation corrected before final backend source;
  both established committed warm-PCM tests retained and passed.
- app-compile.log: invalid --lib invocation, app has only binary target.
- app-compile-2.log: three test-only compilation errors fixed by their owners.
- app-tests-1.log: root test used invalid SourceQualificationId argument; fixed
  with required64 lower-hex characters.
- app-tests-2.log:357 passed/2 failed. Both new fixtures corrected: automatic
  basis Undo changed basis, and captured canvas101x61 violates even-size rule.
- app-harness-tests-1.log: five multiline old moment.copied harness adapters
  missed by single-line search. Converted to shared copied.original().
- app-harness-tests-2:396 unit +3 headless =399 passed, no failed or ignored,
  source ed3c51322c77a1b3b71b1e4141d0b21f88770d0b69584fe9b34919b4283168bb.
  This predates hot-admission fix and its additional test.

## Outstanding qualification

Focused worker rerun, rendered placement/Original/delete regressions with actual
image inspection, release performance replay, optional Clippy, one full base
workspace gate after final review. Choose native QA with separate unique bundle
only; never inspect, select, move, resize, close or relaunch
dev.thiesen.deadpan.cursor-qa, and never switch Spaces.
Update REQUIREMENTS, handoff, qualification/evidence and scoped commit/push.
Goal remains active: full Deadpan DP01-24/GatesA-G is far from complete.

## Final review and layout correction

- Final read-only review found visible placement retained success but reported
  only raw refresh error. Corrected to explicit saved/cause/Reopen guidance.
  Both edited visible/fast and existing Original refresh tests now require it.
  Final reviewer confirmed fix and found no additional issues.
- recovery-tests-final:10 passed.
- worker-tests-final:25 passed, including once-per-view full catalog checks.
- visual-place-2:548 checks passed (includes retained-project check); Original16
  and delete97 regressions plus all11,904 routing cases passed.
- Initial release-performance-final:Sound117<120, RoomTone133<140, Gain136<140.
  One diagnostic using the exact binary reproduced all3 with images.
  Compact viewer vertical margins12->8 restores8 points without changing
  control reserves/hit sizes. Reviewer confirmed bounded change.
- layout-final Sound216, RoomTone220, Gain290, slice547 checks passed;
  minimum pictures125,141,144 points. Final images inspected.
- Current source65fa24bf37e2a6c647c092a52d5ebf9196b48605b98e2d4107034a48da52dbe6.
  Corrected release replay, full workspace gate and native QA remain pending.

## Completed native and release verification

- Corrected release replay:3110 checks pass, source65fa24bf..., optional
  generated-picture fixture explicitly skipped. Detailed limits in report.
- Separate native release keys:fixture Hold, Edit[8,25)copy, refine[9,27),
  endpoint slates009/015, Cancel/reopen unchanged register, one18-frame
  insertion[60,78), placement Undo and fixture Undo. Historical last endpoint013
  remains after both Undos.
- SQLite backup comparisons:all20tables identical for copy/cancel; exactly
  one placement revision; full authored restoration after each Undo;16unrelated
  tables unchanged through all7snapshots. Writer lock released, exact QA process
  exited. CUA post-quit observation relaunched empty QA app twice; final quit
  verified by process path without another UI observation. Reserved window never
  inspected or touched. Native visible-screen screenshots clipped at right;
  full layout evidence remains rendered replay captures.
- fmt-final pass1.486s; strict workspace all-target Clippy with ui-harness
  pass620.474s. Workspace tests now running, started03:50:24UTC, pid49862.

## Final integrated gate

- workspace-final completed successfully in 734.258 seconds: 2,766 tests passed,
  none failed or ignored, across 182 reported test groups including doc tests.
- Formatting, strict all-target Clippy with ui-harness, focused worker/recovery
  checks, corrected release replay and isolated native QA are all complete.
- Final source manifest is 65fa24bf37e2a6c647c092a52d5ebf9196b48605b98e2d4107034a48da52dbe6;
  evidence collection rechecks all 1,330 implementation inputs before commit.
- Earlier pending sections record their original checkpoint, not remaining work.
  Remaining product scope is documented in the qualification and requirements.
