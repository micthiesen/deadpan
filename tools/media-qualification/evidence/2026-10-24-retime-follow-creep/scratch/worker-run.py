#!/usr/bin/env python3
import hashlib, json, os, re, subprocess, sys, time
from datetime import datetime, timezone
from pathlib import Path
ROOT = Path('/Users/michael/Code/deadpan')
SCRATCH = Path(__file__).resolve().parent
OWNED = [
 'crates/deadpan-core/src/framing_zoom.rs',
 'crates/deadpan-core/src/framing_zoom/tests.rs',
 'crates/deadpan-core/src/semantic/planner/framing.rs',
 'crates/deadpan-core/src/semantic/planner/tests/framing.rs',
 'crates/deadpan-cli/tests/project_commands/macros/framing.rs',
 'crates/deadpan-cli/tests/project_commands/macros/framing/sequence_follow_creep.rs',
 'crates/deadpan-cli/tests/project_commands/macros/framing/repeat_follow_creep.rs',
 'crates/deadpan-cli/tests/project_commands/macros/framing/retime_follow_creep.rs',
]
def pins(names):
 return {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in names}
def basis():
 return [
 'crates/deadpan-core/src/framing.rs',
 'crates/deadpan-core/src/framing/numeric.rs',
 'crates/deadpan-core/src/time.rs',
 'crates/deadpan-core/src/source_index.rs',
 'crates/deadpan-plan/src/plan.rs',
 'crates/deadpan-plan/src/picture.rs',
 'crates/deadpan-plan/src/follow_context.rs',
 'crates/deadpan-cli/src/framing.rs',
 'crates/deadpan-cli/src/macros.rs']
def global_pins():
 return {name: (hashlib.sha256((ROOT/name).read_bytes()).hexdigest() if (ROOT/name).exists() else None)
         for name in json.loads((SCRATCH/'source-files.json').read_text())}
label, *command = sys.argv[1:]
receipt = SCRATCH / (label + '.json')
assert not receipt.exists()
record = {'command': command, 'cwd': str(ROOT), 'started_at': datetime.now(timezone.utc).isoformat(),
 'environment': {k:os.environ[k] for k in ['CARGO_TARGET_DIR','DEADPAN_FFMPEG_PREFIX']},
 'owned_before': pins(OWNED), 'basis_before': pins(basis()),
 'global_source_before': global_pins(),
 'basis_scope': 'Nine explicitly named Core numeric/time/index, Plan mapping/projection and CLI framing/macro source files; not a whole-product basis'}
copies = SCRATCH / (label + '-source')
copies.mkdir()
for name in OWNED:
 destination = copies / (name + '.txt')
 destination.parent.mkdir(parents=True, exist_ok=True)
 destination.write_bytes((ROOT / name).read_bytes())
record['source_copies'] = str(copies)
receipt.write_text(json.dumps(record,indent=2)+'\n')
start = time.monotonic()
with (SCRATCH / (label+'.stdout')).open('xb') as out, (SCRATCH / (label+'.stderr')).open('xb') as err:
 child = subprocess.Popen(command, cwd=ROOT, stdout=out, stderr=err)
 record['pid'] = child.pid
 receipt.write_text(json.dumps(record,indent=2)+'\n')
 code = child.wait()
record.update(exit_code=code, elapsed_seconds=time.monotonic()-start,
 finished_at=datetime.now(timezone.utc).isoformat(), owned_after=pins(OWNED),basis_after=pins(basis()))
record['global_source_after'] = global_pins()
record['global_source_unchanged'] = record['global_source_before'] == record['global_source_after']
record['global_missing_before'] = [p for p,v in record['global_source_before'].items() if v is None]
record['global_missing_after'] = [p for p,v in record['global_source_after'].items() if v is None]
record['global_complete_before'] = not record['global_missing_before']
record['global_complete_after'] = not record['global_missing_after']
record['owned_unchanged'] = record['owned_before'] == record['owned_after']
record['basis_unchanged'] = record['basis_before'] == record['basis_after']
stderr = (SCRATCH / (label+'.stderr')).read_text()
record['executed_binary_pins'] = {p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
 for p in re.findall(r'Running [^\n]*\((/[^)]+)\)',stderr) if Path(p).exists()}
record['stream_pins'] = {suffix: hashlib.sha256((SCRATCH/(label+suffix)).read_bytes()).hexdigest()
 for suffix in ['.stdout','.stderr']}
receipt.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps({k:record[k] for k in ['exit_code','elapsed_seconds','owned_unchanged','basis_unchanged','global_source_unchanged']}),flush=True)
sys.exit(code)
