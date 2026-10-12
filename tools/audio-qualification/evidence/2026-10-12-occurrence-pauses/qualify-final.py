import hashlib
import json
import pathlib
import subprocess

scratch = pathlib.Path(__file__).resolve().parent
repo = pathlib.Path('/Users/michael/Code/deadpan')
release = repo / 'target/release/deadpan-cli'
debug = scratch / 'qualified-debug-cli'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
identity = sha(release)
fixture = json.loads((scratch / 'release-export-result.json').read_text())
package = pathlib.Path(fixture['package'])
calls = []

def call(binary, *arguments):
    command = [str(binary), *map(str, arguments)]
    result = subprocess.run(command, capture_output=True, text=True)
    calls.append(dict(command=command, exit=result.returncode, stdout=result.stdout, stderr=result.stderr))
    assert result.returncode == 0, result.stderr
    return result.stdout

before = call(release, 'project', 'dump', package, '--json')
document = json.loads(before)
windows = []
for frame in [0, 2, 3, 6, 8, 12]:
    # Independent round-to-even frame boundary at 30000/1001 fps.
    quotient, remainder = divmod(frame * 8008, 5)
    start = quotient + (remainder > 2) + 512
    actual = json.loads(call(release, 'inspect-audio', package, '--samples', start, start + 256, '--authored-bus'))['audio']['samples']
    expected = json.loads(call(debug, 'inspect-audio', package, '--samples', start, start + 256, '--authored-bus'))['audio']['samples']
    assert actual == expected
    assert len(actual) == 256
    if frame in [2, 6]:
        assert all(value == 0.0 for sample in actual for value in sample)
    else:
        assert any(abs(value) > 0.001 for sample in actual for value in sample)
    windows.append(dict(frame=frame, start=start, samples=actual))

movie = pathlib.Path(fixture['movie'])
report = json.loads(call(release, 'verify-export', package, '--movie', movie, '--revision', document['revision_id'], '--every', '1'))
assert report['passed']
assert report['summary']['pictures_checked'] == 15
assert report['summary']['nonzero_offsets'] == 0
assert report['summary']['offset_verified_windows'] >= 1
assert call(release, 'project', 'dump', package, '--json') == before
assert sha(release) == identity
(scratch / 'final-result.json').write_text(json.dumps(dict(release_binary_sha256=identity, debug_binary_sha256=sha(debug), package=str(package), movie=str(movie), movie_sha256=sha(movie), revision=document['revision_id'], report=report, windows=windows, calls=calls, pcm_bit_identical=True, authored_document_unchanged=True), indent=2) + '\n')
print(json.dumps(report['summary']))
