from pathlib import Path
import difflib
import hashlib
import json
root = Path('/Users/michael/Code/deadpan')
baseline = Path('/tmp/deadpan-moment-paste-20260926/before')
paths = [
    'crates/deadpan-core/src/document.rs', 'crates/deadpan-core/src/lib.rs',
    'crates/deadpan-core/src/legacy_v26.rs',
    'crates/deadpan-store/src/source_registration.rs', 'crates/deadpan-store/src/schema.rs',
    'crates/deadpan-store/src/migration.rs', 'crates/deadpan-store/src/validation.rs',
    'crates/deadpan-cli/src/doctor.rs', 'crates/deadpan-cli/tests/project_commands.rs',
    'crates/deadpan-store/tests/prepared_source_registration.rs',
    'crates/deadpan-store/tests/prepared_source_registration/moment.rs',
    'crates/deadpan-store/tests/migration.rs',
    'crates/deadpan-store/tests/migration/sequence_insert.rs',
    'crates/deadpan-store/tests/migration/moment_splice.rs',
    'crates/deadpan-store/tests/fixtures/produce-v32-moment-splice-history.py',
    'crates/deadpan-store/tests/fixtures/v32-moment-splice-history.sql',
    'crates/deadpan-store/tests/fixtures/v32-moment-splice-history.commands.json',
    'crates/deadpan-store/tests/fixtures/v32-moment-splice-history.provenance.json',
]
manifest = {}
diff = []
for path in paths:
    file = root / path
    prior = baseline / path
    manifest[path] = hashlib.sha256(file.read_bytes()).hexdigest()
    if path.endswith('.rs'):
        diff.extend(difflib.unified_diff(prior.read_text().splitlines(True) if prior.exists() else [],
            file.read_text().splitlines(True), fromfile='before/' + path, tofile=path))
out = Path('/tmp/deadpan-moment-paste-20260926/store')
(out / 'owned-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
(out / 'incremental.diff').write_text(''.join(diff))
provenance = json.loads((root / paths[-1]).read_text())
for suffix, key in [('sql', 'sql_sha256'), ('commands.json', 'producer_log_sha256')]:
    data = (root / ('crates/deadpan-store/tests/fixtures/v32-moment-splice-history.' + suffix)).read_bytes()
    assert hashlib.sha256(data).hexdigest() == provenance[key]
assert manifest[paths[-4]] == provenance['producer_script_sha256']
print(json.dumps(dict(paths=len(paths), fixture_hashes='verified', incremental_diff=str(out / 'incremental.diff'))))
