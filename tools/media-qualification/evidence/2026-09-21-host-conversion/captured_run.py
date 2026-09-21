import hashlib, json, os, shutil, struct, subprocess, sys, tempfile, time
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
worker = Path(sys.argv[1]).resolve()
label = sys.argv[2]
directory = Path(tempfile.mkdtemp(prefix=f'deadpan-host-conversion-{label}-', dir='/tmp'))
print(directory, flush=True)
bin_dir = directory / 'bin'
bin_dir.mkdir()
shutil.copy2(worker, bin_dir / 'deadpan-media-worker')
shutil.copy2(repo / 'target/debug/examples/convert_generated', bin_dir / 'convert_generated')
env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ['PATH'])
capture = Path('/tmp/deadpan-supervised-mlx-20260921-attributed')
raw_dir = Path('/private/tmp/deadpan-ffv1-final3-normal')
source_paths = sorted((repo / 'crates/deadpan-media/src').glob('*.rs')) + sorted((repo / 'native/deadpan-media-worker/src').glob('*')) + [repo / 'native/deadpan-media-worker/build.rs', repo / 'Cargo.lock']

def sha(path):
    return hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()

report = {'scope': 'host converter qualification, not Ready bundle or authored acceptance',
          'label': label, 'directory': str(directory), 'source_sha256': {str(path.relative_to(repo)): sha(path) for path in source_paths},
          'runner_sha256': sha(Path(__file__)),
          'binaries': {str(path): sha(path) for path in bin_dir.iterdir()},
          'host': subprocess.check_output(['sw_vers'], text=True),
          'machine': subprocess.check_output(['sysctl', '-n', 'hw.model'], text=True).strip(),
          'processor': subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip(),
          'memory_bytes': int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True)),
          'compiler': subprocess.check_output(['clang', '--version'], text=True),
          'libraries': {str(path): sha(path) for path in Path('/tmp/deadpan-media-compatible-xyhilms4/prefix/lib').glob('*.dylib') if not path.is_symlink()},
          'worker_linkage': subprocess.check_output(['otool', '-L', bin_dir / 'deadpan-media-worker'], text=True),
          'cases': []}
for name, filename, raw_name, count, num, den, expected in [
    ('sampled', 'host/candidate.snapshot.mp4', 'candidate-snapshot.rgb', 30, 30000, 1001, 'd7fcf04e2bbe213d0352153443eb77c4a1534855ab8c19c0ff5dde4f2ddf85d9'),
    ('native', 'worker/outputs/native.mp4', 'native-25-sequence.rgb', 25, 24, 1, 'c9d34268df14d105bb4f3799e9bfc9b946ad153de43e6ee4a73a44bb7833be33'),
]:
    source = capture / filename
    assert sha(source) == expected
    raw = raw_dir / raw_name
    pixel_hash = hashlib.sha256()
    with raw.open('rb') as input_file:
        header = input_file.read(64)
        assert header[:8] == b'DPFVRGB1'
        width, height, frames = struct.unpack_from('<III', header, 12)
        assert (width, height, frames) == (768, 320, count)
        for _ in range(count):
            assert len(input_file.read(16)) == 16
            pixels = input_file.read(width * height * 3)
            assert len(pixels) == width * height * 3
            pixel_hash.update(pixels)
        assert not input_file.read(1)
    request = {'protocol': 1, 'video': {'width': width, 'height': height, 'frames': count, 'rate_num': num, 'rate_den': den},
               'input_byte_length': source.stat().st_size,
               'limits': {'max_input_bytes': 64*1024*1024, 'max_output_bytes': 64*1024*1024,
                          'max_scratch_bytes': 64*1024*1024, 'timeout_ms': 120000}}
    request_path = directory / f'{name}-request.json'
    request_path.write_text(json.dumps(request, indent=2) + '\n')
    output = directory / f'{name}.mkv'
    command = ['convert_generated', str(bin_dir / 'deadpan-media-worker'), str(source), str(request_path), expected, str(output)]
    started = time.monotonic()
    result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=150)
    (directory / f'{name}.stdout').write_text(result.stdout)
    (directory / f'{name}.stderr').write_text(result.stderr)
    case = {'command': command, 'exit_code': result.returncode, 'seconds': time.monotonic()-started,
            'input_sha256': expected, 'independent_raw_fixture_sha256': sha(raw),
            'independent_rgb_sha256': pixel_hash.hexdigest()}
    if result.returncode == 0:
        converted = json.loads(result.stdout)
        assert converted['report']['input_rgb_sha256'] == pixel_hash.hexdigest()
        assert converted['report']['output_rgb_sha256'] == pixel_hash.hexdigest()
        assert converted['report']['output_bytes'] == output.stat().st_size
        case['conversion'] = converted
        case['output_sha256'] = sha(output)
    else:
        case['failure'] = result.stderr
    report['cases'].append(case)
    (directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'case': name, 'exit_code': result.returncode, 'seconds': case['seconds']}), flush=True)
    if result.returncode:
        raise SystemExit(result.stderr)
print('passed independent captured RGB comparison', flush=True)
