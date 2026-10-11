from pathlib import Path
import hashlib, json, os, shutil, subprocess, time

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

for name in ['app-loops', 'media', 'app-transport', 'app-worker', 'perf-build-final', 'clippy', 'format-final-2']:
    assert json.loads((root / (name + '.json')).read_text())['exit'] == 0, name
out = root / 'measurements'
out.mkdir()
binary = out / 'perf'
shutil.copy2(repo / 'target/release/examples/perf', binary)
pins = json.loads((repo / 'tools/media-qualification/evidence/2026-10-11-hardware-decode/final/library-pins.json').read_text())
for name, expected in pins.items():
    assert digest(Path(name)) == expected, name
changed = subprocess.check_output(['git', 'diff', '--name-only'], cwd=repo, text=True).splitlines()
source = {name: digest(repo / name) for name in changed if name.endswith('.rs')}
identity = dict(base=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
                source_sha256=source, binary_sha256=digest(binary),
                importer_sha256=digest(repo / 'target/release/deadpan-cli'))
(out / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
for name, args in [('system', ['sw_vers']), ('hardware', ['sysctl', 'machdep.cpu.brand_string', 'hw.memsize']),
                   ('power-before', ['pmset', '-g', 'batt']), ('thermal-before', ['pmset', '-g', 'therm']),
                   ('rust', ['rustc', '--version', '--verbose']), ('dependencies', ['otool', '-L', str(binary)])]:
    p = subprocess.run(args, capture_output=True)
    (out / (name + '.txt')).write_bytes(p.stdout + p.stderr)
rows = []
for name, start, end in [('1080', 197, 341), ('4k', 107, 179)]:
    for ahead in ['off', 'on']:
        case = name + '-' + ahead
        began = time.monotonic()
        while os.getloadavg()[0] > 4:
            assert time.monotonic() - began < 240, 'load gate: ' + case
            print('waiting for load below 4', case, os.getloadavg(), flush=True)
            time.sleep(5)
        command = [str(binary), 'playback', str(root / (name + '.deadpan')), '--seconds', '30',
                   '--start-frame', str(start), '--loop-end', str(end), '--pictures', 'original', '--lookahead', ahead]
        load = os.getloadavg()
        began = time.monotonic()
        with (out / (case + '.json')).open('x') as report, (out / (case + '.log')).open('x') as log:
            p = subprocess.run(['/usr/bin/time', '-l', *command], stdout=report, stderr=log, timeout=120)
        row = dict(case=case, command=command, exit=p.returncode, seconds=time.monotonic()-began,
                   load_before=load, load_after=os.getloadavg(), report_sha256=digest(out / (case + '.json')),
                   log_sha256=digest(out / (case + '.log')))
        rows.append(row)
        (out / 'runs.json').write_text(json.dumps(rows, indent=2) + '\n')
        assert p.returncode == 0, row
        result = json.loads((out / (case + '.json')).read_text())
        print(case, json.dumps(dict(dropped=result['pictures']['dropped_skipped'],
                                    picture_ms=result['pictures']['decode_and_gpu_ms'],
                                    seams=result['loop_seams'], lookahead=result['pictures']['lookahead'],
                                    audio=result['audio'], covered=result['covered_requested_interval'])), flush=True)
for name, args in [('power-after', ['pmset', '-g', 'batt']), ('thermal-after', ['pmset', '-g', 'therm'])]:
    p = subprocess.run(args, capture_output=True)
    (out / (name + '.txt')).write_bytes(p.stdout + p.stderr)
