from pathlib import Path
import hashlib,json,subprocess,time
scratch=Path('/tmp/deadpan-selected-hold-duration-next-2026-10-21')
row=json.loads((scratch/'focused-binaries.json').read_text())['deadpan-core']
binary=Path(row['retained'])
assert hashlib.sha256(binary.read_bytes()).hexdigest()==row['sha256']
name='semantic::planner::tests::shared_hold_duration::selected_hold_duration::selected_hold_duration_keeps_partial_physical_clocks_and_mark_copy_loss'
start=time.monotonic()
result=subprocess.run([str(binary),'--exact',name,'--test-threads=1','--nocapture'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
elapsed=time.monotonic()-start
(scratch/'quiet-core.log').write_bytes(result.stdout+result.stderr)
record={'binary':str(binary),'sha256':row['sha256'],'test':name,'exit':result.returncode,'seconds':elapsed,'stdout_and_stderr_reached_eof':True,'timeout_seconds':30,'note':'Isolated exact retained binary after focused Nextest completed. Concurrent-run captured-output marker retained; its cause is not established.'}
(scratch/'quiet-core.json').write_text(json.dumps(record,indent=2)+'\n')
assert result.returncode==0,result.stderr.decode()
assert b'1 passed; 0 failed' in result.stdout,result.stdout.decode()
print(json.dumps(record,indent=2))
