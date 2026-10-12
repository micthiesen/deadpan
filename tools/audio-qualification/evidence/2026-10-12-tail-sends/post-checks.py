import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
checks=[
('doc-tests','cargo test --locked --doc -p deadpan-audio -p deadpan-core -p deadpan-plan -p deadpan-playback'),
('clippy-final','cargo clippy --locked --workspace --all-targets --features ui-harness -- -D warnings'),
('fmt','cargo fmt --all --check'),('whitespace','git diff --check'),
('release-final','cargo build --locked --release -p deadpan-app --features ui-harness')]
for name,command in checks:
 print('Starting '+name,flush=True)
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command.split()])
 if r.returncode:sys.exit(r.returncode)
sys.exit(subprocess.run([sys.executable,str(s/'replays.py')]).returncode)
