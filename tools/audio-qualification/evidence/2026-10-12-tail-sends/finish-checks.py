import subprocess,sys
runner='/tmp/deadpan-tail-sends-20261012/run.py'
checks=[
('store-tests','cargo test --locked -p deadpan-store'),
('app-tests2','cargo test --locked -p deadpan-app --bin deadpan-app --features ui-harness'),
('clippy','cargo clippy --locked --workspace --all-targets --features ui-harness -- -D warnings'),
('fmt','cargo fmt --all --check'),
('whitespace','git diff --check'),
]
for name,command in checks:
 print('Starting '+name,flush=True)
 result=subprocess.run([sys.executable,runner,name,*command.split()])
 if result.returncode:sys.exit(result.returncode)
