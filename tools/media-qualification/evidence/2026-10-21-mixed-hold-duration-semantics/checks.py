from pathlib import Path
import json,re
s=Path('/tmp/deadpan-mixed-hold-duration-next-2026-10-21')
rows=[]
for name,source in [
 ('focused.log','final-source-before-boxing'),
 ('normal.log','final-source-before-boxing'),
 ('ui.log','final-source-before-boxing'),
 ('final-regression.log','final-source-before-dedup'),
 ('final-ui.log','final-source-before-dedup'),
 ('harness-ui-tests.log','final-source-first'),
 ('quiet-tests.log','final-source-before-dedup'),
 ('encoded.log','final-source')]:
 p=s/name
 if not p.exists(): continue
 log=p.read_text(); summaries=[line.strip() for line in log.splitlines() if 'Summary [' in line]
 rows.append({'log':name,'source_snapshot':source,'summary':summaries,'leak_markers':[line.strip() for line in log.splitlines() if line.strip().startswith('LEAK')]})
out={'base':'bcbf8493cdf709a2321e616cb8b1b6b3edb3ddd0','tests':rows,'strict_checks':{'command_reference':'commands_reference_is_current passes final regression; update-mode intentional rewrite failure retained','format':'harness-format.log','changed_packages_and_harness':'harness-lint.log,exit0','explicit_ui_clippy':'final-ui-lint.log,exit0','release_app':'harness-release-app.log,exit0'},'source_note':'The normal/UI/regression source snapshots precede only the documented subsequent fixes. Representation boxing is covered by759 regressions and197UI; later changes only share the replay fixture, fix canonical test-command spelling and provision scenario evidence capacity. Complete final native visual/performance replays exercise that final54-file source.','quiet_audio':'quiet-audio.json,one test PASS0.08s with bounded output EOF'}
(s/'checks.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out,indent=2))
