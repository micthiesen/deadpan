from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import tarfile

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
baseline = root / 'baseline'
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
baseline.mkdir()
archive = root / 'baseline.tar'
with archive.open('wb') as output:
    subprocess.run(['git', 'archive', revision], cwd=repo, stdout=output, check=True)
with tarfile.open(archive) as tar:
    tar.extractall(baseline, filter='data')
core = 'crates/deadpan-core/tests/retime_edits.rs'
audio = 'crates/deadpan-audio/tests/stages.rs'
shutil.copy2(repo / core, baseline / core)
current = (repo / 'crates/deadpan-audio/tests/stages/streaming.rs').read_text()
plan = current[current.index('fn long_plan()'):current.index('fn reference()')]
witness = '''
mod streamed_witness {
use super::*;
const INPUT: u32 = 704 * 8192;
const OUTPUT: u32 = INPUT / 2;
''' + plan + '''
#[test]
fn streamed_input_supports_two_minutes_before_a_late_first_read() {
    let mut renderer = StageAudio::new(long_plan());
    let mut provider = FixtureProvider::new();
    let result = renderer.read(&mut provider, AudioSample(i64::from(OUTPUT - 1)),
        1, Duration::from_secs(60), &AtomicBool::new(false));
    assert!(result.is_ok(), "{result:?}");
}
}
'''
with (baseline / audio).open('a') as output:
    output.write(witness)
(root / 'baseline-audio-overlay.rs').write_text(witness)
target = '/tmp/deadpan-group-mix-20261011/old-target'
(root / 'baseline-source.json').write_text(json.dumps({
    'revision': revision,
    'archive_sha256': hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest(),
    'test_overlays': {path: hashlib.sha256((baseline / path).read_bytes()).hexdigest()
                      for path in [core, audio]},
    'target': target,
}, indent=2) + '\n')
env = dict(os.environ, CARGO_TARGET_DIR=target)
result = subprocess.run(['cargo', 'test', '--locked', '--no-fail-fast',
    '-p', 'deadpan-core', '--test', 'retime_edits', '-p', 'deadpan-audio',
    '--test', 'stages', 'streamed_'], cwd=baseline, env=env)
raise SystemExit(result.returncode)
