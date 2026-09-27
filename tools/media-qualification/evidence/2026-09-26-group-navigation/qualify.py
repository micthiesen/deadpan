import hashlib, json
from pathlib import Path

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-group-navigation-20260926')
gate=json.loads((scratch/'gate-2/report.json').read_text())
prior=json.loads((scratch/'gate-1/report.json').read_text())
source=json.loads((scratch/'gate-2/source-after.json').read_text())
earlier=json.loads((scratch/'gate-1/source-after.json').read_text())
gui=json.loads((scratch/'gui-exit.json').read_text())
assert gate['source_unchanged'] and len(gate['commands'])==7
assert all(row['exit_code']==0 for i,row in enumerate(gate['commands']) if i!=2)
assert gate['commands'][2]['tests']['failed']==1
assert 'PermissionDenied' in (scratch/'gate-2/2.log').read_text()
assert all(hashlib.sha256((repo/p).read_bytes()).hexdigest()==h for p,h in source.items())
changed=[p for p in source if source[p]!=earlier.get(p)]
assert changed and all(p.startswith('crates/deadpan-app/src/preview/') for p in changed)
assert prior['commands'][5]['exit_code']==0

rows=[]
for row in gate['commands']:
    result=f"exit {row['exit_code']}"
    if 'tests' in row:
        t=row['tests']; result+=f"; {t['passed']} passed, {t['failed']} failed, {t['ignored']} ignored"
    rows.append('| `'+ ' '.join(row['command'])+'` | '+result+' |')
checks='''Three independent review lenses covered general correctness, scope and
asynchronous completion, and keyboard ownership. The general review found a
cursor jump when leaving a group after audition passed its parent. The fix moves
cursor/selection resolution into the shared window-free selection transition:
entry clamps to the new group; Backspace and breadcrumbs keep the absolute cursor.
A regression covers heard positions before and after the parent, ordinary return
and entry into an empty group. Follow-up review found no remaining material issue.

Keyboard review requested explicit new-key coverage. The nested scenario now
opens the Hold inspector command with Shift-Tab and Enter and checks Camera
Backspace ownership. Existing CPU help/menu replays now include Enter/Backspace.
These additions are compiled; the GPU replay remains unavailable. The invariant
review reported no material issue. A proposed deeper automatic descent on pause
was not adopted: transport follows the nearest containing ancestor of the browsed
path, as specified and tested, rather than inventing a new sibling scope.

Environment: arm64 macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. Final commands ran serially:

| Command | Result |
| --- | --- |
'''+ '\n'.join(rows)+f'''

All {gate['source_count']} source/config files stayed unchanged through that gate.
The workspace test invocation stopped at the existing sandbox denial in
`deadpan-jobs/tests/artifact.rs:200`: Unix listener creation returned OS 1
PermissionDenied. Later suites did not all execute in that invocation. No test
was disabled and the workspace gate remains non-green.

Before the four preview-only review corrections, the separate locked
store/plan/render invocation passed {prior['commands'][5]['tests']['passed']} tests.
Its source and dependencies remain byte-identical in the final gate. It was not
repeated for those UI changes. Invocation counts overlap and are not distinct-test
totals. Earlier compile failures and an incorrect new test boundary assertion
are retained alongside the corrected runs.

The contributed harness was invoked as:

```sh
cargo run -p deadpan-app --features ui-harness --locked -- --ui-check --scenario nested-pause --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift --output /tmp/deadpan-group-navigation-20260926/ui-nested
```

That attempt preceded the review corrections and exited {gui['exit_code']} at
`egui_kittest` renderer construction with `No adapter found`. Its nested scenario
had zero steps, checks and captures. The separate shortcut audit passed
{gui['shortcut_checked']} routing cases against {gui['shortcut_reservations']}
reservations with no conflicts, matching the current Kestrel source digest.
No final-source GUI pass is claimed. Native startup/shutdown smoke was not repeated
because startup/lifecycle did not change. Release latency, physical key/IME,
VoiceOver and comparison against ImageGen targets remain unverified here.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-26-group-navigation/)
includes exact logs, source identities, incremental diff, review record and the
failed visual report. Local documentation file links were checked separately.
'''
path=repo/'docs/qualification/group-navigation-2026-09-26.md'
text=path.read_text()
anchor='Final integrated checks and review results are recorded after the current gate.'
assert text.count(anchor)==1
path.write_text(text.replace(anchor,checks.strip()))
(scratch/'coverage.json').write_text(json.dumps(dict(
    final_gate=gate, prior_store_plan_render=prior['commands'][5],
    files_changed_since_prior_gate=changed, non_app_sources_unchanged=True,
    sources_match_final_gate=True, gui=gui),indent=2)+'\n')
