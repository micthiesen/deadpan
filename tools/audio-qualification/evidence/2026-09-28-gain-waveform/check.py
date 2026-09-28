"""Serial waveform verification on stable workspace feature graphs."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
runner = Path('/tmp/deadpan-sound-allowances-h1t6i96x/run.py')
stage, prefix = sys.argv[1:3]
if stage == 'focus':
    commands = [
        ('format', ['cargo','fmt','--all']),
        ('format-check', ['cargo','fmt','--all','--','--check']),
        ('clippy', ['cargo','clippy','--workspace','--all-targets','--features','deadpan-app/ui-harness','--locked','--','-D','warnings']),
        ('tests', ['cargo','test','--workspace','--features','deadpan-app/ui-harness','--locked','--no-fail-fast','waveform']),
    ]
elif stage == 'base':
    commands = [
        ('clippy', ['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings']),
        ('tests', ['cargo','test','--workspace','--locked','--no-fail-fast']),
    ]
elif stage == 'app-base':
    commands = [
        ('format-check', ['cargo','fmt','--all','--','--check']),
        ('clippy', ['cargo','clippy','--workspace','--all-targets','--locked','--','-D','warnings']),
        ('artifacts', ['cargo','test','--workspace','--locked','--no-run','--message-format=json']),
        ('app-tests', ['python3',str(root/'feature_tests.py'),prefix+'-artifacts','base']),
    ]
elif stage == 'feature':
    commands = [
        ('clippy', ['cargo','clippy','--workspace','--all-targets','--features','deadpan-app/ui-harness','--locked','--','-D','warnings']),
        ('artifacts', ['cargo','test','--workspace','--features','deadpan-app/ui-harness','--locked','--no-run','--message-format=json']),
        ('app-tests', ['python3',str(root/'feature_tests.py'),prefix+'-artifacts']),
    ]
elif stage in ('visual', 'visual-followup'):
    commands = []
    if stage == 'visual-followup':
        commands.extend([
            ('format-check', ['cargo','fmt','--all','--','--check']),
            ('clippy', ['cargo','clippy','--workspace','--all-targets','--features','deadpan-app/ui-harness','--locked','--','-D','warnings']),
        ])
    commands.append(('build', ['cargo','build','--workspace','--features','deadpan-app/ui-harness','--locked']))
    for scenario in sys.argv[3:] or ['gain','room-tone']:
        name = 'visual-'+scenario
        commands.append((name,['target/debug/deadpan-app','--ui-check','--scenario',scenario,'--retain-projects','--output',str(root/(prefix+'-'+name)),'--kestrel-source','/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift']))
elif stage == 'timing':
    commands = [('cold-timing', ['python3',str(root/'waveform_timing.py'),sys.argv[3]])]
elif stage == 'release':
    commands = [
        ('build', ['cargo','build','--workspace','--release','--features','deadpan-app/ui-harness','--locked']),
        ('performance',['target/release/deadpan-app','--ui-check','--mode','performance','--output',str(root/(prefix+'-performance')),'--kestrel-source','/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift']),
    ]
else:
    raise ValueError(stage)
failed = False
for suffix, command in commands:
    name = prefix+'-'+suffix
    assert not (root/(name+'.json')).exists(), name
    print(json.dumps({'starting':name,'command':command}),flush=True)
    result = subprocess.run(['python3',str(runner),name,*command],
        env=dict(os.environ,DEADPAN_CHECK_OUTPUT=str(root)),stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    report = json.loads((root/(name+'.json')).read_text())
    print(json.dumps({key:report[key] for key in ('exit_code','seconds','source_manifest_sha256')}),flush=True)
    if result.returncode:
        print(result.stdout[-16000:],flush=True)
        if stage in ('visual', 'visual-followup') and suffix.startswith('visual-'):
            failed = True
            continue
        raise SystemExit(result.returncode)
raise SystemExit(1 if failed else 0)
