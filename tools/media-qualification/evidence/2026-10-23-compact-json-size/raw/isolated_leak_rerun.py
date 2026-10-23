from pathlib import Path
import subprocess, json, time, hashlib, os
q = Path('/tmp/deadpan-compact-json-size-qualification-2026-10-23')
phase = 'candidate-ui'
needle = 'project::tests::render::unchanged_preview_cannot_reuse_an_earlier_commit_receipt'
def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def guard():
    sources = json.loads((q / (phase+'-source-identities.json')).read_text())
    names = set(subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines())
    assert names == set(sources)
    assert all(sha(path) == expected for path, expected in sources.items())
    assert all(sha(path) == expected for path, expected in json.loads((q / (phase+'-inputs.json')).read_text()).items())
guard()
suites = json.loads((q / (phase+'-list.json')).read_text())['rust-suites']
matches = [suite for suite in suites.values() if needle in suite['testcases']]
assert len(matches) == 1
binary = matches[0]['binary-path']
before = sha(binary)
command = ['cargo','nextest','run','--archive-file',str(q/(phase+'.tar.zst')),'--extract-to',str(q/(phase+'-extracted')),'--extract-overwrite','--workspace-remap','/Users/michael/Code/deadpan','-E','test('+needle+')','--test-threads','1','--no-fail-fast']
environment = dict(os.environ,
    CARGO_PROFILE_DEV_OPT_LEVEL='2',
    DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',
    DEADPAN_REQUIRE_PHYSICAL_MAC='1',
    DEADPAN_REQUIRE_SYNTHETIC_WORKER='1',
    DEADPAN_SCOPE_PROBE_EVIDENCE=str(q / 'candidate-ui-isolated-leak-evidence'))
started = time.monotonic()
log = q / 'candidate-ui-isolated-leak-rerun.log'
with log.open('w') as out:
    result = subprocess.run(command,env=environment,stdout=out,stderr=subprocess.STDOUT)
text = log.read_text()
assert result.returncode == 0 and '1 test run: 1 passed' in text, text[-2500:]
assert sha(binary) == before
guard()
row = {'phase':phase,'test':needle,'command':command,'exit':result.returncode,'seconds':time.monotonic()-started,'tests':1,'passed':1,'leaky':'LEAK [' in text,'binary':binary,'sha256':before,'source_binary_guards':True,'original_required_hardware_and_synthetic_environment_preserved':True,'independent_evidence_path':environment['DEADPAN_SCOPE_PROBE_EVIDENCE']}
(q / 'isolated-leak-reruns.json').write_text(json.dumps([row],indent=2)+'\n')
print(json.dumps(row),flush=True)
