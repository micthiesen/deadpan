from pathlib import Path
import gzip
import hashlib
import json
import shutil
import subprocess
import tarfile

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
destination = repo / 'tools/audio-qualification/evidence/2026-10-11-disk-pcm'
destination.mkdir(exist_ok=True)

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def save(name, value):
    (destination / name).write_text(json.dumps(value, indent=2) + '\n')

for path in root.glob('*.log'):
    (destination / (path.name + '.gz')).write_bytes(gzip.compress(path.read_bytes(), mtime=0))
    shutil.copy2(path.with_suffix('.json'), destination / path.with_suffix('.json').name)
for path in root.glob('*-binaries.json'):
    shutil.copy2(path, destination / path.name)
for name in ['baseline-source.json', 'check-evidence.py', 'baseline.py',
             'run.py', 'pin-binaries.py', 'collect.py', 'final-runs.py']:
    shutil.copy2(root / name, destination / name)
modified = subprocess.check_output(['git', 'diff', '--name-only'], cwd=repo, text=True).splitlines()
untracked = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], cwd=repo, text=True).splitlines()
paths = sorted(path for path in set(modified + untracked)
               if path.startswith(('crates/', 'native/')))
paths += ['Cargo.lock', 'rust-toolchain.toml']
save('source-pins.json', {
    'base': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'files': {name: digest(repo / name) for name in paths},
    'note': 'The full suite covers final audio implementation. A final focused run covers the later disk-quota eviction assertion; Clippy and the real export cover the later CLI error mapping and long-export fixture.',
})
save('host.json', {
    'os': subprocess.check_output(['sw_vers'], text=True).strip(),
    'hardware': subprocess.check_output(['sysctl', 'machdep.cpu.brand_string', 'hw.physicalcpu', 'hw.memsize'], text=True).strip(),
    'rust': subprocess.check_output(['rustc', '-vV'], text=True).strip(),
    'qualified_ffmpeg_prefix': '/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',
})
export = json.loads((root / 'export.json').read_text())
(destination / 'export.json.gz').write_bytes(gzip.compress((root / 'export.json').read_bytes(), mtime=0))
movie = Path(export['movie'])
shutil.copytree(movie.parent, destination / 'exports', dirs_exist_ok=True)
fixture_root = Path(export['package']).parent
with tarfile.open(destination / 'fixtures.tar.gz', 'w:gz') as tar:
    tar.add(fixture_root, arcname='fixtures')
save('artifact-pins.json', {path.name: digest(path) for path in (destination / 'exports').iterdir()})
save('review.json', {
    'reviewer': 'main session; no subagents',
    'scope': [
        'Private unlinked descriptor lifetime; no paths, durable assets or external admission tokens.',
        'Exact little-endian finite PCM; bounded positional reads, complete-file publication and poisoned partial writes.',
        'RAM and disk reservations include recursive preparations and request-local pinned projections; only unpinned LRU entries can be evicted.',
        'Cancellation/failure releases reservations and drops unfinished files; complete child caches can survive retries.',
        'Full input dependency admission, canonical DSP schedule and sampler are preserved; no engine-ID or schema change.',
        'Core and worker Preserve budgets agree; non-streamed Hold bounds remain separate.',
        'Long PCM compares an independent decoded-WAV mapping; real four-minute output is rendered and independently decoded.',
    ],
    'resolved': [
        'Corrected an initial patch anchor that changed the return type of read_stage_input; the initial compiler failure was not retained as a separate receipt.',
        'Added poisoned-writer retry checks after reviewing partial writes.',
        'Changed the eviction regression to exhaust disk quota rather than stage count.',
        'The initial export test expected all pictures while using automatic sampling. The final test explicitly selects every picture and retains its report before provenance assertions.',
        'The retained baseline reproduction script pins the already measured base revision instead of resolving a future HEAD.',
    ],
    'remaining': [
        'First demand still prepares complete DSP history within a cooperative deadline; background scheduling/progress remains open.',
        'Files are ephemeral and private; no persistent content-addressed or cross-process cache is claimed.',
        'The resident-frame admission bound excludes native FFT state, source caches and a bounded sampler halo; it is not whole-process RSS.',
        'No live GUI, physical device, listening or new native sanitizer claim. Other export recipes were not rerun for this storage-only milestone.',
    ],
})
save('manifest.json', {str(path.relative_to(destination)): {
    'bytes': path.stat().st_size, 'sha256': digest(path),
} for path in sorted(destination.rglob('*')) if path.is_file() and path.name != 'manifest.json'})
print(destination)
