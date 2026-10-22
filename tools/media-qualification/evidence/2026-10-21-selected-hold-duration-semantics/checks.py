from pathlib import Path
import json
scratch=Path('/tmp/deadpan-selected-hold-duration-next-2026-10-21')
rows=[]
for name,source in [
    ('baseline.log','baseline-source'),
    ('focused-first-runtime.log','second-runtime-source'),
    ('focused.log','third-runtime-source'),
    ('regression.log','third-runtime-source'),
    ('ui.log','third-runtime-source'),
    ('post-capture-app.log','fourth-runtime-source'),
    ('post-capture-ui.log','fourth-runtime-source'),
    ('encoded.log','final-source'),
]:
    path=scratch/name
    if not path.exists():
        continue
    lines=path.read_text().splitlines()
    rows.append({'log':name,'source_snapshot':source,'summaries':[line.strip() for line in lines if 'Summary [' in line],'leak_markers':[line.strip() for line in lines if line.strip().startswith('LEAK')]})
record={
    'base':'f384a85810f51be4d52ebfbc86b24071ef111268',
    'tests':rows,
    'quiet_core':'quiet-core.json; exact retained binary passes with output EOF',
    'strict_checks':{
        'format':'post-navigation-format.log',
        'changed_packages':'post-capture-lint.log',
        'ui_clippy':'post-navigation-lint.log',
        'release_app':'final-release-app.log',
    },
    'source_note':'The 3446-test broader and first198 UI results precede only the final Source/Hold macro-capture kind guard. That app-only fix has196 normal app and198 UI follow-ups, strict checks and final native replays. Later changes repair native replay setup/oracles: supported selected-duration entry, re-entering the Hold after play-set changes, mapped IDs after reopen, copied default Sequence shape, fresh versus pre-owned Hold counts, and explicit play-set reselection after navigation revokes mapped continuation. Final UI Clippy and release replays exercise them. Failed compiler/setup/native reports remain separate; an intentional generated-command-reference rewrite exits with an assertion before the ordinary passing check.',
}
(scratch/'checks.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
