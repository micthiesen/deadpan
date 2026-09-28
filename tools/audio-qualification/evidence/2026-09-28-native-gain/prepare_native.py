"""Bundle the exact verified binary and record the retained private fixture."""
import hashlib
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

root = Path(__file__).parent
repository = Path('/Users/michael/Code/deadpan')
artifact_name, replay_name = sys.argv[1:]
project = root / replay_name / 'projects/gain/Documents/Deadpan/cfr-bframes.deadpan'
assert (project / 'project.sqlite').is_file()
binary = repository / 'target/debug/deadpan-app'
inventory = json.loads((root / (artifact_name + '-app-inventory.json')).read_text())
expected = next(row['sha256'] for row in inventory if row['target'] == 'deadpan-app' and not row['test'])
assert hashlib.sha256(binary.read_bytes()).hexdigest() == expected
native = root / 'native'
native.mkdir(exist_ok=False)
bundle = native / 'Deadpan Gain Review.app'
contents = bundle / 'Contents'
(contents / 'MacOS').mkdir(parents=True)
bundled = contents / 'MacOS/deadpan-app'
shutil.copy2(binary, bundled)
assert hashlib.sha256(bundled.read_bytes()).hexdigest() == expected
info = {
    'CFBundleDisplayName': 'Deadpan Gain Review',
    'CFBundleExecutable': 'deadpan-app',
    'CFBundleIdentifier': 'dev.deadpan.gain-review-20260928',
    'CFBundleName': 'Deadpan Gain Review',
    'CFBundlePackageType': 'APPL',
    'CFBundleVersion': '0.1.0',
    'NSHighResolutionCapable': True,
}
with (contents / 'Info.plist').open('wb') as target:
    plistlib.dump(info, target)
dump = subprocess.check_output([str(binary), '--headless', 'project', 'dump', str(project), '--json'])
(native / 'project-before.json').write_bytes(dump)
record = {'bundle': str(bundle), 'project': str(project), 'binary_sha256': expected,
          'bundle_sha256': hashlib.sha256(bundled.read_bytes()).hexdigest(),
          'project_dump_sha256': hashlib.sha256(dump).hexdigest()}
(native / 'build.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record, indent=2))
