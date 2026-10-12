from pathlib import Path
import hashlib, json, shutil, subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
binary = root / 'bin'
binary.mkdir(exist_ok=True)
for name in ['binary-pins.json', 'source-pins.json']:
    current = root/name
    prior = root/('initial-'+name)
    if current.exists() and not prior.exists():
        shutil.copy2(current, prior)
for old in Path('/tmp/deadpan-gain-waveform-20261011/bin-release').iterdir():
    source = repo/'target/release'/old.name if old.name in ['deadpan-app', 'deadpan-cli'] else old
    shutil.copy2(source, binary/old.name)
(root/'binary-pins.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in binary.iterdir()},indent=2)+'\n')
changed = subprocess.check_output(['git', 'diff', '--name-only'], cwd=repo, text=True).splitlines()
new = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], cwd=repo, text=True).splitlines()
files = sorted(p for p in set(changed + new) if p.startswith('crates/deadpan-app/') or p == 'docs/COMMANDS.md')
(root/'source-pins.json').write_text(json.dumps(dict(base_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(), files={p:hashlib.sha256((repo/p).read_bytes()).hexdigest() for p in files}),indent=2)+'\n')
