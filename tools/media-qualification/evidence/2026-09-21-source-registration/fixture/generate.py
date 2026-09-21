"""Produce schema-13 SQL twice with the archived, pinned pre-registration Rust."""
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
REVISION = 'ed5c8eccebb8bb77451284f95b99b4ed33c49c93'
SEED = REPO / 'crates/deadpan-store/tests/fixtures/v12-history.sql'
EXPECTED = REPO / 'crates/deadpan-store/tests/fixtures/v13-history.sql'
HEADER = '''-- Genuine schema-13 project generated and validated with commit
-- ed5c8eccebb8bb77451284f95b99b4ed33c49c93 (core schema 8).
-- Starts from v12-history.sql, migrated by a CLI rebuilt from that revision.
-- That revision's ProjectStore API redoes the inherited edit and adds exact
-- signed audio/video Placement mappings with direct and occurrence commands,
-- a local mark, and an abandoned rename branch. Undo/redo chronology ends
-- with one pending redo. Existing originals and generation rows are preserved.
-- Generated twice with tools/media-qualification/evidence/
-- 2026-09-21-source-registration/fixture/generate.py, byte-for-byte equal.
-- Each dump uses SQLite's backup API. External media is not in this fixture.
PRAGMA application_id=1146113585;
PRAGMA user_version=13;
'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, env=None, cwd=None):
    result = subprocess.run([str(arg) for arg in args], check=True,
                            capture_output=True, text=True, env=env, cwd=cwd)
    with (HERE / 'generation-output.txt').open('a') as output:
        output.write(json.dumps([str(arg) for arg in args]) + '\n'
                     + result.stdout + result.stderr + '\n')
    return result.stdout


(HERE / 'generation-output.txt').write_text('')
scratch = Path(tempfile.mkdtemp(prefix='deadpan-schema13-reproduce-', dir='/tmp'))
archive = scratch / 'old-source.tar'
with archive.open('wb') as output:
    subprocess.run(['git', 'archive', REVISION, 'Cargo.toml', 'Cargo.lock',
                    'rust-toolchain.toml', 'crates', 'native'], cwd=REPO,
                   stdout=output, check=True)
source = scratch / 'source'
source.mkdir()
with tarfile.open(archive) as tar:
    tar.extractall(source, filter='data')
example = source / 'crates/deadpan-store/examples/schema13_fixture.rs'
example.parent.mkdir(exist_ok=True)
shutil.copyfile(HERE / 'schema13_fixture.rs', example)
env = os.environ.copy()
env['CARGO_TARGET_DIR'] = '/tmp/deadpan-source-registration-migration/old-target'
run(['cargo', 'build', '--manifest-path', source / 'Cargo.toml', '-p',
     'deadpan-cli', '--locked'], env, source)
cli = Path(env['CARGO_TARGET_DIR']) / 'debug/deadpan-cli'
doctor = json.loads(run([cli, 'doctor']))
assert (doctor['document_schema'], doctor['database_schema']) == (8, 13)
(HERE / 'old-binary-doctor.json').write_text(json.dumps(doctor, indent=2) + '\n')
reproductions = []
for number in range(2):
    package = scratch / f'fixture-{number}.deadpan'
    package.mkdir()
    (package / 'Snapshots').mkdir()
    with sqlite3.connect(package / 'project.sqlite') as connection:
        connection.executescript(SEED.read_text())
    run([cli, 'project', 'migrate', package])
    run(['cargo', 'run', '--manifest-path', source / 'Cargo.toml', '-p',
         'deadpan-store', '--example', 'schema13_fixture', '--locked', '--',
         package], env, source)
    validation = run([cli, 'project', 'validate', package])
    (HERE / 'old-binary-validation.json').write_text(validation)
    with sqlite3.connect(package / 'project.sqlite') as connection:
        with sqlite3.connect(scratch / f'schema13-backup-{number}.sqlite') as snapshot:
            connection.backup(snapshot)
            output = HEADER + '\n'.join(snapshot.iterdump()) + '\n'
            reproduced = scratch / f'v13-history-reproduced-{number}.sql'
            reproduced.write_text(output)
            reproductions.append(digest(reproduced))
            if EXPECTED.exists():
                assert output.encode() == EXPECTED.read_bytes(), 'Fixture must reproduce byte-for-byte'
            else:
                EXPECTED.write_text(output)
            counts = {
                table: snapshot.execute(f'SELECT count(*) FROM {table}').fetchone()[0]
                for table in ['revisions', 'history', 'original_media', 'redo',
                              'generation_requests', 'generation_attempts',
                              'generation_bundle_receipts']
            }
assert len(set(reproductions)) == 1
manifest = {
    'source_revision': REVISION,
    'scratch': str(scratch),
    'archive_sha256': digest(archive),
    'cli': str(cli),
    'cli_sha256': digest(cli),
    'generator_sha256': digest(HERE / 'generate.py'),
    'harness_sha256': digest(HERE / 'schema13_fixture.rs'),
    'seed_fixture': str(SEED.relative_to(REPO)),
    'seed_sha256': digest(SEED),
    'sql_fixture': str(EXPECTED.relative_to(REPO)),
    'sql_sha256': digest(EXPECTED),
    'sql_byte_reproduction': True,
    'reproduced_sql_sha256': reproductions,
    'counts': counts,
}
(HERE / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
