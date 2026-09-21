"""Reproduce schema-10 fixture from pinned old source plus preserved old CLI."""
from pathlib import Path
import hashlib, json, os, shutil, sqlite3, subprocess, sys, tarfile, tempfile
REPO = Path('/Users/michael/Code/deadpan')
REVISION = '82f76aaf86e24d4fb3aec822ada0a638d19ddf25'
CLI = Path('/tmp/deadpan-schema10-fixture-cli')
HERE = Path(__file__).parent
AUDIO = REPO / 'native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav'
SEED = REPO / 'crates/deadpan-store/tests/fixtures/v9-history.sql'
EXPECTED = REPO / 'crates/deadpan-store/tests/fixtures/v10-history.sql'
HEADER = '''-- Genuine schema-10 project generated and validated with commit
-- 82f76aaf86e24d4fb3aec822ada0a638d19ddf25 (core schema 5).
-- Starts from v9-history.sql, migrated by the preserved schema-10 CLI.
-- The same revision's ProjectStore::commit_reconciled appended an original A/V
-- asset, two Source nodes with offsets -137 / 2401, source-clock and local marks,
-- and a rename. undo_reconciled / redo_reconciled / undo_reconciled retain a redo.
-- The prior generated Hold acceptance, reversion, six-object admission and
-- operational request/attempt records remain present throughout this chronology.
-- The schema-10 CLI retained pcm-stereo-48000.wav through the real managed-original
-- API, then validated the complete package. This SQL comes from a SQLite backup.
-- Media bytes are intentionally external to this metadata migration fixture.
PRAGMA application_id=1146113585;
PRAGMA user_version=10;
'''
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def run(args, env=None):
    result = subprocess.run([str(arg) for arg in args], check=True, capture_output=True, text=True, env=env)
    with (HERE/'generation-output.txt').open('a') as output:
        output.write(json.dumps([str(arg) for arg in args])+'\n'+result.stdout+result.stderr+'\n')
    return result.stdout
(HERE/'generation-output.txt').write_text('')
scratch = Path(tempfile.mkdtemp(prefix='deadpan-schema10-reproduce-', dir='/tmp'))
archive = scratch/'old-source.tar'
with archive.open('wb') as output:
    subprocess.run(['git','archive',REVISION,'Cargo.toml','Cargo.lock','rust-toolchain.toml','crates','native'], cwd=REPO, stdout=output, check=True)
source = scratch/'source'; source.mkdir()
with tarfile.open(archive) as tar: tar.extractall(source, filter='data')
example=source/'crates/deadpan-store/examples/schema10_fixture.rs';example.parent.mkdir(exist_ok=True);shutil.copyfile(HERE/'schema10_fixture.rs', example)
package=scratch/'fixture.deadpan';package.mkdir();(package/'Snapshots').mkdir()
conn=sqlite3.connect(package/'project.sqlite');conn.executescript(SEED.read_text());conn.close()
run([CLI,'project','migrate',package])
env=os.environ.copy();env['CARGO_TARGET_DIR']='/tmp/deadpan-schema10-target'
run(['cargo','run','--manifest-path',source/'Cargo.toml','-p','deadpan-store','--example','schema10_fixture','--locked','--',package],env)
(package/'Media').mkdir(mode=0o700);(package/'Media/Originals').mkdir(mode=0o700)
run([CLI,'project','retain-original',package,AUDIO])
validation=run([CLI,'project','validate',package]);(HERE/'old-binary-validation.json').write_text(validation)
conn=sqlite3.connect(package/'project.sqlite');snapshot=sqlite3.connect(HERE/'schema10-before-migration.sqlite');conn.backup(snapshot)
output=HEADER+'\n'.join(snapshot.iterdump())+'\n';(HERE/'v10-history-reproduced.sql').write_text(output)
assert output.encode()==EXPECTED.read_bytes(), 'SQL output must reproduce committed fixture byte-for-byte'
manifest={
 'source_revision': REVISION, 'scratch':str(scratch), 'archive_sha256':digest(archive),
 'cli':str(CLI),'cli_sha256':digest(CLI),
 'generator_sha256':digest(HERE/'generate.py'),'harness_sha256':digest(HERE/'schema10_fixture.rs'),
 'original_fixture':str(AUDIO),'original_sha256':digest(AUDIO),
 'seed_fixture':str(SEED),'seed_sha256':digest(SEED),
 'sql_fixture':str(EXPECTED),'sql_sha256':digest(EXPECTED),'sql_byte_reproduction':True,
 'source_files_sha256': {str(path.relative_to(source)):digest(path) for directory in ['crates/deadpan-core/src','crates/deadpan-store/src'] for path in sorted((source/directory).rglob('*.rs'))},
 'revisions':snapshot.execute('SELECT count(*) FROM revisions').fetchone()[0],
 'history_edits':snapshot.execute('SELECT count(*) FROM history').fetchone()[0],
 'original_records':snapshot.execute('SELECT count(*) FROM original_media').fetchone()[0],
}
(HERE/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({key:value for key,value in manifest.items() if key!='source_files_sha256'},indent=2))
