from pathlib import Path
import json,hashlib,subprocess
s=Path('/tmp/deadpan-mixed-hold-duration-next-2026-10-21')
suite=json.loads((s/'final-list.json').read_text())['rust-suites']['deadpan-audio::audio_definition']
binary=Path(suite['binary-path'])
before=hashlib.sha256(binary.read_bytes()).hexdigest()
name='mix::scoped_mix_sums_two_decoded_rates_and_keeps_exhaustion_and_gates_per_voice'
result=subprocess.run([str(binary),'--exact',name,'--test-threads=1','--nocapture'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
(s/'quiet-audio.log').write_bytes(result.stdout+result.stderr)
after=hashlib.sha256(binary.read_bytes()).hexdigest()
assert before==after
(s/'quiet-audio.json').write_text(json.dumps({'binary':str(binary),'sha256':before,'test':name,'exit':result.returncode,'stdout_and_stderr_reached_eof':True,'timeout_seconds':30,'note':'Isolated direct execution; no other Deadpan tests run concurrently. Final native timing remains separate.'},indent=2)+'\n')
assert result.returncode==0,result.stderr.decode()
assert b'1 passed; 0 failed' in result.stdout,result.stdout.decode()
print(result.stdout.decode())
