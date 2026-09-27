import json
from pathlib import Path

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-nested-sequence-20260926')
gate=json.loads((scratch/'gate-1/report.json').read_text())
gui=json.loads((scratch/'gui-exit.json').read_text())
assert gate['source_unchanged'] and len(gate['commands']) == 8
assert all(c['exit_code'] == 0 for i,c in enumerate(gate['commands']) if i != 2)
assert gate['commands'][2]['tests']['failed'] == 1
assert 'PermissionDenied' in (scratch/'gate-1/2.log').read_text()
rows=[]
for item in gate['commands']:
    result=f"exit {item['exit_code']}"
    if 'tests' in item:
        t=item['tests']; result+=f"; {t['passed']} passed, {t['failed']} failed, {t['ignored']} ignored"
    rows.append('| `'+ ' '.join(item['command']) + '` | '+result+' |')
verification='''Environment: arm64 macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg
prefix `/tmp/deadpan-ui-ffmpeg/prefix`. Commands ran serially. Invocation counts
overlap and do not represent distinct tests.

| Command | Result |
| --- | --- |
'''+ '\n'.join(rows)+f'''

All {gate['source_count']} source/config files stayed unchanged through the gate.
The workspace invocation stopped at the existing sandbox denial in
`deadpan-jobs/tests/artifact.rs:200`: Unix listener creation returned OS 1
PermissionDenied. Later suites did not all execute in that invocation. The
separate store/plan/render and harness-enabled app commands cover their selected
suites. No test was disabled. The full workspace gate remains non-green.

Focused checks passed: 23 core insertion tests, 9 decoded-audio insertion tests,
92 migration tests, 17 CLI command tests and 6 native pause service tests.
The additional nested re-split PCM test ran in the final workspace invocation.
An earlier CLI test attempt stopped at a test assertion type mismatch, corrected
before the passing run. Harness-enabled strict lint also passed independently
before the final gate.

The visual command was attempted once for the new native completion behavior:
`cargo run --locked -p deadpan-app --features ui-harness -- --ui-check --output
/tmp/deadpan-nested-sequence-20260926/gui --scenario nested-pause`.
'''
assert gui['exit_code'] != 0
assert gui['scenario_checks'] == 0 and gui['scenario_steps'] == 0
assert gui['screenshots'] == 0 and gui['shortcut_passed']
assert gui['shortcut_conflicts'] == []
assert any('No adapter found' in finding['message'] for finding in gui['scenario_findings'])
verification+=f'''
It exited {gui['exit_code']} because Metal adapter creation failed before app
construction (`No adapter found`). The nested scenario ran zero steps and zero
assertions and produced no UI captures. The separate shortcut audit passed
{gui['shortcut_checked']} routing cases against {gui['shortcut_reservations']}
reservations with no conflicts. That audit and the compiled scenario do not
qualify the new keyboard presentation or aesthetics.

Native startup/shutdown smoke and release latency replay were not repeated:
startup/lifecycle did not change, and Metal startup prevents this environment's
new visual replay. Physical display, VoiceOver and native IME delivery remain
unverified. No new performance claim is made.
'''
p=repo/'docs/qualification/nested-sequence-2026-09-26.md'
text=p.read_text();old='Final gate and GUI replay results will be recorded here after the current runs.'
assert text.count(old)==1
p.write_text(text.replace(old,verification.strip()))
