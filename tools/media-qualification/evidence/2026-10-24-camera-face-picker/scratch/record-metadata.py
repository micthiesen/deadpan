#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path

scratch = Path(__file__).resolve().parent
relative = 'crates/deadpan-app/src/preview/harness/targets.rs'
before = (scratch / 'targets-before-metadata.rs.txt').read_bytes()
after = (Path('/Users/michael/Code/deadpan') / relative).read_bytes()
old = b'Face/person proposals are not implemented; the ,z target punch-in and :zoom target= are covered by the zoom scenario.'
new = b'Detected face proposals and their Camera picker are covered by the faces scenario; target punch-in and :zoom target= are covered by zoom.'
assert before.count(old) == 1
assert before.replace(old, new) == after
assert hashlib.sha256(before).hexdigest() == 'c44f7c18ed877e26d7a5704c83b031a7182f71a5f143e2c95dcb00f82742c5a0'
with (scratch / 'targets-after-metadata.rs.txt').open('xb') as output:
    output.write(after)
row = {'path': relative, 'before_sha256': hashlib.sha256(before).hexdigest(),
       'after_sha256': hashlib.sha256(after).hexdigest(),
       'exactly_one_string_replacement': True, 'functional_behavior_changed': False,
       'new_wording_executed_in_retained_replays': False,
       'note': 'The targets reports retain the old skip wording; face coverage is in the separate faces report.'}
with (scratch / 'post-run-metadata-correction.json').open('x') as output:
    json.dump(row, output, indent=2)
    output.write('\n')
print(json.dumps(row))
