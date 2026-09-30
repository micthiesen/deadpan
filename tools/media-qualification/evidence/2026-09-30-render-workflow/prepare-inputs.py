import hashlib
import json
import shutil
import sqlite3
from pathlib import Path

root = Path('/tmp/deadpan-render-workflow-EtO3Qu1W')
prior = Path('/tmp/deadpan-publication-recovery-ams9kpds/native-inputs')
destination = root / 'native-inputs'
destination.mkdir()
records = []
for name in ('source', 'generated'):
    source = prior / (name + '.deadpan')
    target = destination / source.name
    def ignore(directory, names):
        if Path(directory) == source:
            return [n for n in names if n.startswith('project.sqlite') or n == '.writer.lock']
        return []
    shutil.copytree(source, target, ignore=ignore)
    with sqlite3.connect(f'file:{source / "project.sqlite"}?mode=ro', uri=True) as src:
        assert src.execute('PRAGMA user_version').fetchone()[0] == 41
        with sqlite3.connect(target / 'project.sqlite') as dst:
            src.backup(dst)
    records.append({'source':str(source), 'target':str(target), 'schema':41,
                    'database_sha256':hashlib.sha256((target/'project.sqlite').read_bytes()).hexdigest(),
                    'method':'immutable objects copied; live database copied through SQLite backup API'})
(root/'native-inputs.json').write_text(json.dumps(records, indent=2)+'\n')
print(json.dumps(records, indent=2))
