from pathlib import Path
import hashlib, json, os, re, subprocess, sys, time

root = Path(__file__).parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows = []
checks = [
    ('generation-model-pack-isolated', ['cargo', 'test', '--locked', '-p', 'deadpan-models', '--lib', 'unexpected_resume_offsets_keep_the_partial_and_installs_are_exclusive']),
    ('generation-models-followup', ['cargo', 'test', '--locked', '-p', 'deadpan-models', '--lib']),
    ('generation-relevance-followup', ['cargo', 'test', '--locked', '-p', 'deadpan-cli', '--lib', 'generation_context']),
    ('generation-conditioning-followup', ['cargo', 'test', '--locked', '-p', 'deadpan-cli', '--lib', 'generation::conditioning']),
    ('generation-real-conditioning-followup', ['cargo', 'test', '--locked', '-p', 'deadpan-cli', '--test', 'bridge_conditioning']),
    ('generation-python', ['python3', '-m', 'unittest', 'discover', '-s', 'tools/model-qualification/tests', '-p', 'test_*.py']),
]
for name, command in checks[int(sys.argv[1]) if len(sys.argv) > 1 else 0:]:
    if len(sys.argv) > 2:
        name += '-' + sys.argv[2]
    print('START', name, flush=True)
    start = time.monotonic()
    logpath = root/(name+'.log')
    with logpath.open('w') as log:
        result = subprocess.run(command, cwd='/Users/michael/Code/deadpan', env=env, stdout=log, stderr=subprocess.STDOUT)
    log = logpath.read_text()
    counts = re.findall(r'test result: ok\. (\d+) passed', log) if command[0]=='cargo' else re.findall(r'Ran (\d+) tests', log)
    passed = sum(map(int, counts))
    rows.append(dict(name=name, command=command, exit=result.returncode, passed=passed, seconds=time.monotonic()-start, sha256=hashlib.sha256(logpath.read_bytes()).hexdigest()))
    (root/('generation-followup-tests' + ('-' + sys.argv[2] if len(sys.argv) > 2 else '') + '.json')).write_text(json.dumps(rows, indent=2)+'\n')
    print('FINISH', name, result.returncode, 'passed', passed, flush=True)
    if result.returncode or passed==0:
        print(log[-16000:])
        raise SystemExit(result.returncode or 1)
