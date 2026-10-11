from pathlib import Path
import gzip, hashlib, json, re, subprocess, sys

repo = Path('/Users/michael/Code/deadpan')
root = repo / 'tools/media-qualification/evidence/2026-10-11-loop-lookahead'
def sha(data): return hashlib.sha256(data).hexdigest()
manifest = json.loads((root / 'manifest.json').read_text())
assert len({r['path'] for r in manifest}) == len(manifest)
raw = {}
for row in manifest:
    data = (root / row['path']).read_bytes()
    assert len(data) == row['bytes'] and sha(data) == row['sha256'], row['path']
    unpacked = gzip.decompress(data) if row['path'].endswith('.gz') else data
    assert sha(unpacked) == row['original_sha256'], row['path']
    name = row['path'].removesuffix('.gz')
    raw[name] = unpacked
identity = json.loads(raw['measurements/identity.json'])
for name, expected in identity['source_sha256'].items():
    # Only the Original final-picture hint and its regression test changed
    # after the Edit performance runs. Retain their measured-era source too.
    before = 'before-tail-fix/' + name
    data = raw[before] if before in raw else (repo / name).read_bytes()
    assert sha(data) == expected, name
for name, expected in json.loads(raw['final-source.json']).items():
    data = subprocess.check_output(['git', 'show', 'HEAD:' + name], cwd=repo) if '--committed' in sys.argv else (repo / name).read_bytes()
    assert sha(data) == expected, name
for name, expected in json.loads(raw['fixture-pins.json']).items():
    assert sha((repo / name).read_bytes()) == expected, name
for name, expected in json.loads(raw['library-pins.json']).items():
    assert sha(Path(name).read_bytes()) == expected, name
binary = Path('/tmp/deadpan-loop-lookahead-20261011/measurements/perf')
assert sha(binary.read_bytes()) == identity['binary_sha256']
for name in ['app-loops', 'media', 'app-transport', 'app-worker', 'perf-build-final', 'clippy', 'format-final-2', 'tail-transport', 'tail-worker', 'tail-clippy', 'tail-format-check']:
    check = json.loads(raw[name + '.json'])
    assert check['exit'] == 0, name
    assert sha(raw[name + '.log']) == check['log_sha256'], name
summary = []
for row in json.loads(raw['measurements/runs.json']):
    case = row['case']
    assert row['exit'] == 0 and row['load_before'][0] <= 4, case
    assert sha(raw['measurements/' + case + '.json']) == row['report_sha256']
    assert sha(raw['measurements/' + case + '.log']) == row['log_sha256']
    report = json.loads(raw['measurements/' + case + '.json'])
    pictures, audio = report['pictures'], report['audio']
    assert report['covered_requested_interval'] and report['failure'] is None, case
    assert audio['underruns_starved'] == 0 and audio['device_faults'] == 0, case
    assert len(report['loop_seams']) >= 12, case
    if case.endswith('-on'):
        assert pictures['dropped_skipped'] == 0, case
        assert pictures['lookahead']['swaps'] >= len(report['loop_seams']), case
        assert pictures['lookahead']['failed'] == 0, case
        assert all(s['frame'] == report['start_frame'] for s in report['loop_seams']), case
    else:
        assert pictures['dropped_skipped'] > 0, 'negative control did not expose the seam cost: ' + case
    summary.append(dict(case=case, dropped=pictures['dropped_skipped'], seams=len(report['loop_seams']),
                        slowest_seam_ms=max(s['decode_and_gpu_ms'] for s in report['loop_seams'])))
requirements = (repo / 'docs/REQUIREMENTS.md').read_text()
assert len(re.findall(r'^\| DP-\d+ \|[^\n]+\| Complete \|', requirements, re.M)) == 8
links = 0
for name in ['docs/REQUIREMENTS.md', 'docs/PLAYBACK.md', 'docs/PERFORMANCE.md', 'docs/spec/AGENT_HANDOFF.md', 'docs/qualification/loop-lookahead-2026-10-11.md']:
    page = repo / name
    for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)', page.read_text()):
        if ':' in target or target.startswith('#'): continue
        path = target.split('#', 1)[0].strip('<>')
        if path:
            assert (page.parent / path).exists(), (name, target)
            links += 1
print(json.dumps(dict(files=len(manifest), source_files=len(identity['source_sha256']), links=links, runs=summary), indent=2))
