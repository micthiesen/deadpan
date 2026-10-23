from pathlib import Path
import os, subprocess, time, json, hashlib, shutil, re

s = Path('/tmp/deadpan-semantic-refresh-duration-reuse-2026-10-23')
repo = Path('/Users/michael/Code/deadpan')
source = s / 'source'
target = s / 'target'
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix', CARGO_TARGET_DIR=str(target))
(s / 'build-owner.json').write_text(json.dumps({'pid':os.getpid(), 'started_unix':time.time(), 'script':str(Path(__file__))}, indent=2)+'\n')
rows = []
tests = []
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
identities = lambda: {str(p.relative_to(source)):sha(p) for p in source.rglob('*.rs')}
inputs = json.loads((s / 'input-identities.json').read_text())

def input_guard():
    assert all(sha(source / p) == v for p,v in inputs.items())

def run(name, cmd, test=False):
    input_guard()
    start = time.monotonic()
    with (s / (name+'.stdout')).open('w') as out, (s / (name+'.stderr')).open('w') as err:
        r = subprocess.run(cmd, cwd=source, env=env, stdout=out, stderr=err)
    row = {'name':name, 'command':cmd, 'cwd':str(source), 'exit':r.returncode, 'seconds':time.monotonic()-start}
    rows.append(row)
    (s / 'build-receipt.json').write_text(json.dumps(rows, indent=2)+'\n')
    print(json.dumps(row), flush=True)
    if r.returncode:
        raise SystemExit(r.returncode)
    input_guard()
    if test:
        results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (s/(name+'.stdout')).read_text())
        assert results and sum(int(x[0]) for x in results) > 0, name
        assert all(x[1:] == ('0','0') for x in results), results
        for index, path in enumerate(re.findall(r'Running .* \(([^\n)]+)\)', (s/(name+'.stderr')).read_text())):
            src = Path(path)
            if not src.is_absolute():
                src = source / src
            dest = s / 'focused-bin' / (name+'-'+str(index))
            dest.parent.mkdir(exist_ok=True)
            shutil.copy2(src, dest)
            tests.append({'step':name, 'source':str(src), 'retained_executed_copy':str(dest), 'sha256':sha(dest), 'test_results':results})
        assert any(t['step'] == name for t in tests), name
        (s/'focused-binaries.json').write_text(json.dumps(tests,indent=2)+'\n')

target.mkdir(exist_ok=False)
run('clone-release-cache', ['cp','-cR',str(repo/'target/release'),str(target/'release')])
run('format', ['cargo','fmt','--all'])
frozen = identities()
baseline = json.loads((s/'original-source-identities.json').read_text())
changed = {p for p in frozen if frozen[p] != baseline.get(p)}
assert changed == {'crates/deadpan-core/src/semantic/planner/selection.rs','crates/deadpan-core/tests/semantic_refresh.rs'}
assert set(frozen) == set(baseline) | {'crates/deadpan-core/tests/semantic_refresh.rs'}
(s/'build-source-identities.json').write_text(json.dumps(frozen,indent=2)+'\n')
(s/'formatted-overlay-audit.json').write_text(json.dumps({'changed':sorted(changed),'nonowned_rust_unchanged':True,'inputs_unchanged':True},indent=2)+'\n')
run('semantic-refresh-tests', ['cargo','test','--locked','--release','-p','deadpan-core','--test','semantic_refresh'], test=True)
assert identities() == frozen
run('semantic-trim-tests', ['cargo','test','--locked','--release','-p','deadpan-core','--test','semantic_trim'], test=True)
assert identities() == frozen
run('command-work-tests', ['cargo','test','--locked','--release','-p','deadpan-core','--test','command_work'], test=True)
assert identities() == frozen
run('release-build', ['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app'])
assert identities() == frozen
bins = {}
for name in ['deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
    src = target / 'release' / name
    dest = s / 'bin' / name
    dest.parent.mkdir(exist_ok=True)
    shutil.copy2(src,dest)
    bins[name] = {'source':str(src),'executed':str(dest),'sha256':sha(dest),'freshly_built':name=='deadpan-app'}
(s/'binaries.json').write_text(json.dumps(bins,indent=2)+'\n')
print(json.dumps({'done':True,'app_sha256':bins['deadpan-app']['sha256']}),flush=True)
