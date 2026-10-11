from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(__file__).parent
steps = [
    ('format', ['cargo','fmt','--all']),
    ('native-tests', ['cargo','nextest','run','--locked','-p','deadpan-encode','--no-fail-fast']),
    ('render-tests', ['cargo','nextest','run','--locked','-p','deadpan-cli','--lib','--test','encoded_verification','--features','synthetic-worker','-E','test(encoded_render::) | binary(encoded_verification)','--no-fail-fast']),
    ('clippy', ['cargo','clippy','--workspace','--all-targets','--features','deadpan-cli/synthetic-worker','--locked','--','-D','warnings']),
    ('ui-clippy', ['cargo','clippy','-p','deadpan-app','--all-targets','--features','ui-harness','--locked','--','-D','warnings']),
    ('ui-tests', ['cargo','nextest','run','-p','deadpan-app','--features','ui-harness','--locked','--no-fail-fast']),
    ('doc-tests', ['cargo','test','--workspace','--features','deadpan-cli/synthetic-worker','--locked','--doc','--no-fail-fast']),
]
for name, command in steps:
    print('START',name,flush=True)
    result = subprocess.run(['python3',str(root/'run.py'),name,*command])
    if name == 'native-tests':
        paths=[p for p in Path('target/debug/deps').iterdir() if p.is_file() and p.name.startswith('deadpan_encode-') and p.suffix=='' and p.stat().st_mode&0o111]
        binary=max(paths,key=lambda p:p.stat().st_mtime_ns)
        (root/'regression-after-binary.json').write_text(json.dumps(dict(path=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),test_sha256=hashlib.sha256(Path('native/deadpan-encode/src/mux_duration_tests.rs').read_bytes()).hexdigest()),indent=2)+'\n')
    if result.returncode:
        print((root/(name+'.log')).read_text(errors='replace')[-5000:],flush=True)
        sys.exit(result.returncode)
