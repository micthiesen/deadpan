"""Retain debug cold-admission timing from the already-built test artifact."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

scratch = Path(__file__).parent
artifacts = []
for line in (scratch / (sys.argv[1] + '.log')).read_text().splitlines():
    try:
        row = json.loads(line)
    except json.JSONDecodeError:
        continue
    if row.get('reason') == 'compiler-artifact' and row.get('executable') and row['profile']['test']:
        if Path(row['manifest_path']).parent.name == 'deadpan-playback' and row['target']['kind'] == ['lib']:
            artifacts.append(row)
assert len(artifacts) == 1, artifacts
artifact = artifacts[0]
executable = Path(artifact['executable'])
record = {'executable': str(executable), 'sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
    'cwd': str(Path(artifact['manifest_path']).parent), 'features': artifact['features'],
    'command': ['--exact', 'tests::waveform::real_definition_peaks_match_canonical_pcm_and_each_request_readmits_evidence', '--nocapture']}
print(json.dumps(record), flush=True)
result = subprocess.run([str(executable), *record['command']], cwd=record['cwd'], env=os.environ)
record['exit_code'] = result.returncode
(scratch / (sys.argv[1] + '-timing-inventory.json')).write_text(json.dumps(record, indent=2) + '\n')
raise SystemExit(result.returncode)
