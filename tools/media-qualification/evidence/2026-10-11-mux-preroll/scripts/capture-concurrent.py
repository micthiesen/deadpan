from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import hashlib
import json
import subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
binaries = [p for p in (repo / 'target/debug/deps').iterdir()
            if p.is_file() and p.name.startswith('deadpan_cli-') and p.suffix == '' and p.stat().st_mode & 0o111]
binary = max(binaries, key=lambda p: p.stat().st_mtime_ns)
name = 'encoded_render::verification::inspect::hdr_tests::hdr_pq_and_hlg_main10_files_pass_complete_inspection'
assert name in subprocess.check_output([str(binary), '--list'], text=True)
(root / 'concurrent-binary.json').write_text(json.dumps(dict(path=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
    test_source_sha256=hashlib.sha256((repo / 'crates/deadpan-cli/src/encoded_render/verification/inspect/hdr_tests.rs').read_bytes()).hexdigest()), indent=2)+'\n')
def run(index):
    return subprocess.run(['python3', str(root / 'run.py'), f'concurrent-{index}', str(binary), '--exact', name, '--nocapture']).returncode
for batch in range(2):
    with ThreadPoolExecutor(max_workers=4) as executor:
        outcomes = list(executor.map(run, range(batch * 4, batch * 4 + 4)))
    if any(outcomes):
        print('Failures retained by the test; inspect concurrent logs', flush=True)
        break
else:
    raise RuntimeError('No failure in eight concurrent trials')
