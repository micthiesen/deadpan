"""Produce genuine schema-12 SQL using archived, pinned pre-placement Rust."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import sqlite3
import subprocess
import tarfile
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
REVISION = '35b8011e775049af41ef5e12c40fa49e62467cef'
SEED = REPO / 'crates/deadpan-store/tests/fixtures/v11-history.sql'
EXPECTED = REPO / 'crates/deadpan-store/tests/fixtures/v12-history.sql'
HEADER = '''-- Genuine schema-12 project generated and validated with commit
-- 35b8011e775049af41ef5e12c40fa49e62467cef (core schema 7).
-- Starts from v11-history.sql, migrated by a CLI rebuilt from that revision.
-- The same revision's ProjectStore API redoes the inherited edit, applies exact
-- video Duration mappings 28750/1001 and 120000/1001, preserving audio mappings
-- through direct and occurrence commands, adds a mark, and retains an abandoned
-- rename branch plus undo/redo chronology ending with one pending redo.
-- Existing generated requests, attempts, receipts, and originals are preserved.
-- Generated with tools/media-qualification/evidence/2026-09-21-import-timing/
-- fixture/generate.py; this dump comes from SQLite's backup API. Media bytes
-- remain external to this metadata-only migration fixture.
PRAGMA application_id=1146113585;
PRAGMA user_version=12;
'''

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(args, env=None, cwd=None):
    result = subprocess.run([str(arg) for arg in args], check=True, capture_output=True,
                            text=True, env=env, cwd=cwd)
    with (HERE / 'generation-output.txt').open('a') as output:
        output.write(json.dumps([str(arg) for arg in args]) + '\n' + result.stdout + result.stderr + '\n')
    return result.stdout

(HERE / 'generation-output.txt').write_text('')
scratch = Path(tempfile.mkdtemp(prefix='deadpan-schema12-reproduce-', dir='/tmp'))
archive = scratch / 'old-source.tar'
with archive.open('wb') as output:
    subprocess.run(['git', 'archive', REVISION, 'Cargo.toml', 'Cargo.lock',
                    'rust-toolchain.toml', 'crates', 'native'], cwd=REPO, stdout=output, check=True)
source = scratch / 'source'
source.mkdir()
with tarfile.open(archive) as tar:
    tar.extractall(source, filter='data')
example = source / 'crates/deadpan-store/examples/schema12_fixture.rs'
example.parent.mkdir(exist_ok=True)
shutil.copyfile(HERE / 'schema12_fixture.rs', example)
env = os.environ.copy()
env['CARGO_TARGET_DIR'] = '/tmp/deadpan-import-timing-migration/old-target'
run(['cargo', 'build', '--manifest-path', source / 'Cargo.toml', '-p', 'deadpan-cli', '--locked'], env, source)
cli = Path(env['CARGO_TARGET_DIR']) / 'debug/deadpan-cli'
doctor = json.loads(run([cli, 'doctor']))
assert (doctor['document_schema'], doctor['database_schema']) == (7, 12)
(HERE / 'old-binary-doctor.json').write_text(json.dumps(doctor, indent=2) + '\n')
package = scratch / 'fixture.deadpan'
package.mkdir()
(package / 'Snapshots').mkdir()
connection = sqlite3.connect(package / 'project.sqlite')
connection.executescript(SEED.read_text())
connection.close()
run([cli, 'project', 'migrate', package])
run(['cargo', 'run', '--manifest-path', source / 'Cargo.toml', '-p', 'deadpan-store',
     '--example', 'schema12_fixture', '--locked', '--', package], env, source)
validation = run([cli, 'project', 'validate', package])
(HERE / 'old-binary-validation.json').write_text(validation)
connection = sqlite3.connect(package / 'project.sqlite')
snapshot = sqlite3.connect(scratch / 'schema12-before-migration.sqlite')
connection.backup(snapshot)
output = HEADER + '\n'.join(snapshot.iterdump()) + '\n'
reproduced = scratch / 'v12-history-reproduced.sql'
reproduced.write_text(output)
if EXPECTED.exists():
    assert output.encode() == EXPECTED.read_bytes(), 'Fixture must reproduce byte-for-byte'
else:
    EXPECTED.write_text(output)
manifest = {
    'source_revision': REVISION,
    'scratch': str(scratch),
    'archive_sha256': digest(archive),
    'cli': str(cli),
    'cli_sha256': digest(cli),
    'generator_sha256': digest(HERE / 'generate.py'),
    'harness_sha256': digest(HERE / 'schema12_fixture.rs'),
    'seed_fixture': str(SEED.relative_to(REPO)),
    'seed_sha256': digest(SEED),
    'sql_fixture': str(EXPECTED.relative_to(REPO)),
    'sql_sha256': digest(EXPECTED),
    'sql_byte_reproduction': True,
    'revisions': snapshot.execute('SELECT count(*) FROM revisions').fetchone()[0],
    'history_edits': snapshot.execute('SELECT count(*) FROM history').fetchone()[0],
    'original_records': snapshot.execute('SELECT count(*) FROM original_media').fetchone()[0],
    'pending_redo': snapshot.execute('SELECT count(*) FROM redo').fetchone()[0],
}
(HERE / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
