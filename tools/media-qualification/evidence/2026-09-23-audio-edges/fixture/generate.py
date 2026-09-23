"""Reproduce schema 15 twice with the pinned pre-edge CLI and store API."""
from pathlib import Path
import gzip
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
REVISION = 'd1348a5dbf2a4979a54af823b4a4f5ca82ed44e1'
SEED = REPO / 'crates/deadpan-store/tests/fixtures/v14-history.sql'
EXPECTED = REPO / 'crates/deadpan-store/tests/fixtures/v15-history.sql'
HEADER = '''-- Genuine schema-15 project generated and validated with commit
-- d1348a5dbf2a4979a54af823b4a4f5ca82ed44e1 (core schema 10).
-- Starts from v14-history.sql, migrated by that revision's rebuilt CLI.
-- The old host API imports a primary source, adopts measured geometry, then
-- edits the canvas and leaves a pending redo. Earlier branches, qualifications,
-- original ownership and operational generation rows remain in the fixture.
-- Generated twice by tools/media-qualification/evidence/
-- 2026-09-23-audio-edges/fixture/generate.py, byte-for-byte equal.
-- SQLite backup captures the metadata; managed media bytes are not embedded.
PRAGMA application_id=1146113585;
PRAGMA user_version=15;
'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


scratch = Path(tempfile.mkdtemp(prefix='deadpan-schema15-reproduce-', dir='/tmp'))
log = HERE / f'generation-{scratch.name}.txt'


def run(args, env=None, cwd=None):
    result = subprocess.run([str(arg) for arg in args], capture_output=True,
                            text=True, env=env, cwd=cwd)
    with log.open('a') as output:
        output.write(json.dumps([str(arg) for arg in args]) + '\n'
                     + result.stdout + result.stderr
                     + f'\nexit_code={result.returncode}\n')
    result.check_returncode()
    return result.stdout


archive = scratch / 'old-source.tar'
with archive.open('wb') as output:
    subprocess.run(['git', 'archive', REVISION, 'Cargo.toml', 'Cargo.lock',
                    'rust-toolchain.toml', 'crates', 'native'], cwd=REPO,
                   stdout=output, check=True)
source = scratch / 'source'
source.mkdir()
with tarfile.open(archive) as tar:
    tar.extractall(source, filter='data')
example = source / 'crates/deadpan-store/examples/schema15_fixture.rs'
example.parent.mkdir(exist_ok=True)
shutil.copyfile(HERE / 'schema15_fixture.rs', example)
env = os.environ.copy()
env['CARGO_TARGET_DIR'] = '/tmp/deadpan-audio-edge-migration/old-target'
run(['cargo', 'build', '--manifest-path', source / 'Cargo.toml', '-p',
     'deadpan-cli', '--locked'], env, source)
cli = Path(env['CARGO_TARGET_DIR']) / 'debug/deadpan-cli'
doctor = json.loads(run([cli, 'doctor']))
assert (doctor['document_schema'], doctor['database_schema']) == (10, 15)
(HERE / 'old-binary-doctor.json').write_text(json.dumps(doctor, indent=2) + '\n')
reproductions = []
for number in range(2):
    package = scratch / f'fixture-{number}.deadpan'
    package.mkdir()
    for relative in ['Snapshots', 'Media/Originals', 'Media/Generated']:
        (package / relative).mkdir(parents=True)
    with sqlite3.connect(package / 'project.sqlite') as connection:
        connection.executescript(SEED.read_text())
    run([cli, 'project', 'migrate', package])
    run(['cargo', 'run', '--manifest-path', source / 'Cargo.toml', '-p',
         'deadpan-store', '--example', 'schema15_fixture', '--locked', '--',
         package], env, source)
    validation = run([cli, 'project', 'validate', package])
    (HERE / f'old-binary-validation-{number}.json').write_text(validation)
    with sqlite3.connect(package / 'project.sqlite') as connection:
        with sqlite3.connect(scratch / f'schema15-backup-{number}.sqlite') as snapshot:
            connection.backup(snapshot)
            output = HEADER + '\n'.join(snapshot.iterdump()) + '\n'
            reproduced = scratch / f'v15-history-reproduced-{number}.sql'
            reproduced.write_text(output)
            reproductions.append(digest(reproduced))
            if EXPECTED.exists():
                assert output.encode() == EXPECTED.read_bytes(), 'Fixture must reproduce byte-for-byte'
            else:
                EXPECTED.write_text(output)
            counts = {
                table: snapshot.execute(f'SELECT count(*) FROM {table}').fetchone()[0]
                for table in ['revisions', 'history', 'original_media', 'source_qualifications',
                              'redo', 'generation_requests', 'generation_attempts',
                              'generation_bundle_receipts']
            }
assert len(set(reproductions)) == 1
compressed_log = log.with_suffix(log.suffix + '.gz')
compressed_log.write_bytes(gzip.compress(log.read_bytes(), mtime=0))
log.unlink()
manifest = {
    'source_revision': REVISION,
    'scratch': str(scratch),
    'archive_sha256': digest(archive),
    'cli': str(cli),
    'cli_sha256': digest(cli),
    'generator_sha256': digest(HERE / 'generate.py'),
    'harness_sha256': digest(HERE / 'schema15_fixture.rs'),
    'ffmpeg_prefix': env.get('DEADPAN_FFMPEG_PREFIX'),
    'seed_fixture': str(SEED.relative_to(REPO)),
    'seed_sha256': digest(SEED),
    'sql_fixture': str(EXPECTED.relative_to(REPO)),
    'sql_sha256': digest(EXPECTED),
    'sql_byte_reproduction': True,
    'reproduced_sql_sha256': reproductions,
    'source_fixture_sha256': {
        name: digest(source / 'native/deadpan-source/tests/fixtures' / name)
        for name in ['cfr-bframes.mp4', 'vfr.mp4']
    },
    'counts': counts,
    'log': compressed_log.name,
    'evidence_sha256': {
        path.name: digest(path)
        for path in [HERE / 'old-binary-doctor.json',
                     HERE / 'old-binary-validation-0.json',
                     HERE / 'old-binary-validation-1.json', compressed_log]
    },
}
(HERE / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
