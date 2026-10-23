from pathlib import Path
import difflib
import hashlib
import json

root = Path('/Users/michael/Code/deadpan')
out = Path('/tmp/deadpan-ui-pause-profile-2026-10-23')
paths = [
    'crates/deadpan-app/src/project/service.rs',
    'crates/deadpan-app/src/project/service/macros.rs',
]
original = {p: (root / p).read_text() for p in paths}
proposed = dict(original)
def replace(path, old, new):
    text = proposed[path]
    if text.count(old) != 1:
        raise RuntimeError(f'{path}: expected exactly one anchor, got {text.count(old)}')
    proposed[path] = text.replace(old, new, 1)

service, macros = paths
replace(service, 'mod macros;\n', 'mod macro_profile;\nmod macros;\n')
replace(service, '''            Ok(request) => {
                if service.dispatch_request(request) {
                    #[cfg(test)]
                    service.shared.request_completed(crate::project::Lane::User);
                    service.shared.busy.store(false, Ordering::Release);
                }
                service.invalidate_changed_splice();
                service.reconcile_slip();
                service.reconcile_trim();
                service.publish();
            }
''', '''            Ok(request) => {
                let profiling_macro = matches!(&request, ProjectRequest::Macro(_))
                    && macro_profile::begin();
                {
                    let _total =
                        macro_profile::Span::start(macro_profile::Phase::ServiceTotal);
                    let completed = service.dispatch_request(request);
                    let _publication = macro_profile::Span::start(
                        macro_profile::Phase::PostDispatchPublication,
                    );
                    if completed {
                        #[cfg(test)]
                        service.shared.request_completed(crate::project::Lane::User);
                        service.shared.busy.store(false, Ordering::Release);
                    }
                    service.invalidate_changed_splice();
                    service.reconcile_slip();
                    service.reconcile_trim();
                    service.publish();
                }
                if profiling_macro {
                    macro_profile::finish();
                }
            }
''')
replace(macros, '''    fn perform_macro(&mut self, operation: &Operation) -> Result<Receipt> {
        self.observe_semantic();
        let id = operation.id();
        let (document, stored) = self.check_macro_context(id)?;
''', '''    fn perform_macro(&mut self, operation: &Operation) -> Result<Receipt> {
        let _total = macro_profile::Span::start(macro_profile::Phase::PerformMacroTotal);
        let context_profile = macro_profile::Span::start(macro_profile::Phase::ContextValidation);
        self.observe_semantic();
        let id = operation.id();
        let (document, stored) = self.check_macro_context(id)?;
        drop(context_profile);
''')
replace(macros, '''                let picture_plan = RenderPlan::compile(&document).map_err(display)?;
''', '''                let pre_edit_profile =
                    macro_profile::Span::start(macro_profile::Phase::PreEditPlanScope);
                let picture_plan = RenderPlan::compile(&document).map_err(display)?;
''')
replace(macros, '''                let invocation = SemanticProgram::new(vec![instruction]).map_err(display)?;
                let plan = deadpan_cli::macros::plan_program_with_transcript_guard(
''', '''                let invocation = SemanticProgram::new(vec![instruction]).map_err(display)?;
                drop(pre_edit_profile);
                let plan_profile =
                    macro_profile::Span::start(macro_profile::Phase::SemanticProgramPlan);
                let plan = deadpan_cli::macros::plan_program_with_transcript_guard(
''')
replace(macros, '''                    id.transcript_guard.as_deref(),
                )
                .map_err(display)?;
                if let Some(consumed) = &plan.transcript_guard {
''', '''                    id.transcript_guard.as_deref(),
                )
                .map_err(display)?;
                drop(plan_profile);
                let guard_profile =
                    macro_profile::Span::start(macro_profile::Phase::TranscriptGuard);
                if let Some(consumed) = &plan.transcript_guard {
''')
replace(macros, '''                let final_picture_plan = RenderPlan::compile(&plan.document).map_err(display)?;
''', '''                drop(guard_profile);
                let final_profile =
                    macro_profile::Span::start(macro_profile::Phase::FinalPlanScopeReceipt);
                let final_picture_plan = RenderPlan::compile(&plan.document).map_err(display)?;
''')
replace(macros, '''                let Some(_) = &plan.request else {
''', '''                drop(final_profile);
                let Some(_) = &plan.request else {
''')
replace(macros, '''                let preview = plan
                    .preview(self.store.as_ref().ok_or("Open a project first")?)
''', '''                let preview_profile =
                    macro_profile::Span::start(macro_profile::Phase::PreviewRuntimeBank);
                let preview = plan
                    .preview(self.store.as_ref().ok_or("Open a project first")?)
''')
replace(macros, '''                let outcome = plan.commit_compound(self.writer()?).map_err(display)?;
                if let Some(saved) = &outcome.committed {
''', '''                drop(preview_profile);
                let commit_profile =
                    macro_profile::Span::start(macro_profile::Phase::CommitCompound);
                let outcome = plan.commit_compound(self.writer()?).map_err(display)?;
                drop(commit_profile);
                let receipt_profile =
                    macro_profile::Span::start(macro_profile::Phase::PostCommitReceipt);
                if let Some(saved) = &outcome.committed {
''')
replace(macros, '''                match self.refresh() {
                    Ok(()) => {
''', '''                drop(receipt_profile);
                let refresh_profile =
                    macro_profile::Span::start(macro_profile::Phase::ServiceRefresh);
                let refresh_result = self.refresh();
                drop(refresh_profile);
                let _post_refresh_profile =
                    macro_profile::Span::start(macro_profile::Phase::PostRefreshReceipt);
                match refresh_result {
                    Ok(()) => {
''')

helper = '''//! Temporary isolated-build profiling. Never print project or content data.
//! Child phases are disjoint; ServiceTotal and PerformMacroTotal are inclusive.

use std::cell::RefCell;
use std::time::Instant;

#[derive(Clone, Copy)]
pub(super) enum Phase {
    ServiceTotal,
    PerformMacroTotal,
    ContextValidation,
    PreEditPlanScope,
    SemanticProgramPlan,
    TranscriptGuard,
    FinalPlanScopeReceipt,
    PreviewRuntimeBank,
    CommitCompound,
    PostCommitReceipt,
    ServiceRefresh,
    PostRefreshReceipt,
    PostDispatchPublication,
}

const PHASES: [(&str, &str); 13] = [
    ("service_total", "inclusive_total"),
    ("perform_macro_total", "nested_inclusive_total"),
    ("context_validation", "disjoint_child"),
    ("pre_edit_plan_scope", "disjoint_child"),
    ("semantic_program_plan", "disjoint_child"),
    ("transcript_guard", "disjoint_child"),
    ("final_plan_scope_receipt", "disjoint_child"),
    ("preview_runtime_bank", "disjoint_child"),
    ("commit_compound", "disjoint_child"),
    ("post_commit_receipt", "disjoint_child"),
    ("service_refresh", "disjoint_child"),
    ("post_refresh_receipt", "disjoint_child"),
    ("post_dispatch_publication", "disjoint_service_child"),
];

#[derive(Default)]
struct Measurement {
    nanoseconds: u128,
    count: u64,
}

struct Profile {
    phases: [Measurement; PHASES.len()],
}

thread_local! {
    static ACTIVE: RefCell<Option<Profile>> = const { RefCell::new(None) };
}

pub(super) fn begin() -> bool {
    if std::env::var_os("DEADPAN_UI_PAUSE_PROFILE").as_deref()
        != Some(std::ffi::OsStr::new("1"))
    {
        return false;
    }
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if active.is_some() {
            return false;
        }
        *active = Some(Profile {
            phases: std::array::from_fn(|_| Measurement::default()),
        });
        true
    })
}

pub(super) struct Span {
    phase: Phase,
    started: Option<Instant>,
}

impl Span {
    pub(super) fn start(phase: Phase) -> Self {
        let started = ACTIVE.with(|active| active.borrow().is_some().then(Instant::now));
        Self { phase, started }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some(started) = self.started else {
            return;
        };
        let elapsed = started.elapsed().as_nanos();
        ACTIVE.with(|active| {
            if let Some(profile) = active.borrow_mut().as_mut() {
                let measurement = &mut profile.phases[self.phase as usize];
                measurement.nanoseconds = measurement.nanoseconds.saturating_add(elapsed);
                measurement.count = measurement.count.saturating_add(1);
            }
        });
    }
}

pub(super) fn finish() {
    let profile = ACTIVE.with(|active| active.borrow_mut().take());
    let Some(profile) = profile else {
        return;
    };
    // Output occurs only after the measured service route and publication.
    eprintln!("DEADPAN_UI_PAUSE_PROFILE BEGIN count=1");
    for ((phase, kind), measurement) in PHASES.into_iter().zip(profile.phases) {
        eprintln!(
            "DEADPAN_UI_PAUSE_PROFILE PHASE phase={phase} kind={kind} elapsed_ns={} count={}",
            measurement.nanoseconds, measurement.count,
        );
    }
    eprintln!("DEADPAN_UI_PAUSE_PROFILE END count=1");
}
'''
new_path = 'crates/deadpan-app/src/project/service/macro_profile.rs'
proposed[new_path] = helper
patches = []
manifest = {'algorithm': 'sha256', 'repo': str(root), 'files': []}
for path, text in proposed.items():
    before = original.get(path, '')
    patches.extend(difflib.unified_diff(
        before.splitlines(keepends=True), text.splitlines(keepends=True),
        fromfile=('a/' + path) if path in original else '/dev/null', tofile='b/' + path))
    target = out / (path.replace('/', '__') + '.txt')
    target.write_text(text)
    manifest['files'].append({
        'path': path,
        'original_sha256': hashlib.sha256(before.encode()).hexdigest() if path in original else None,
        'original_bytes': len(before.encode()) if path in original else None,
        'proposed_sha256': hashlib.sha256(text.encode()).hexdigest(),
        'proposed_bytes': len(text.encode()),
        'proposed_contents': target.name,
    })
(out / 'instrumentation.patch').write_text(''.join(patches))
(out / 'source-hashes.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
