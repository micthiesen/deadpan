"""Create real core32 gain-migration history through the preserved old CLI."""
from pathlib import Path
import hashlib, json, os, shutil, sqlite3, subprocess

root = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-authored-gain-289_gcj6')
package = scratch/'gain-history-3.deadpan'
package.mkdir()
(package/'Snapshots').mkdir()
originals = package/'Media/Originals'
originals.mkdir(parents=True)
shutil.copy2(root/'native/deadpan-source/tests/fixtures/offset-bframes.mp4', originals/'blake3-2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f')
database = sqlite3.connect(package/'project.sqlite')
database.executescript((root/'crates/deadpan-store/tests/fixtures/v37-hold-audio-history.sql').read_text())
database.close()
cli = scratch/'old-deadpan-cli'
log = []
def run(*args):
    result = subprocess.run([str(cli), *map(str,args)], env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix'), text=True, capture_output=True)
    log.append({'args':list(map(str,args)), 'code':result.returncode, 'stdout':result.stdout, 'stderr':result.stderr})
    (scratch/'fixture-generation-3.json').write_text(json.dumps(log,indent=2)+'\n')
    if result.returncode: raise RuntimeError(log[-1])
    return json.loads(result.stdout)
doctor = run('doctor')
assert doctor['database_schema'] == 38 and doctor['document_schema'] == 32
run('project','migrate',package)
def dump(): return run('project','dump',package,'--json')
def command(revision, body):
    d=dump()
    request={'protocol':1,'project_id':d['project_id'],'expected_revision':d['revision_id'],'new_revision':revision,'command':body}
    path=scratch/(revision+'.json'); path.write_text(json.dumps(request,indent=2)+'\n')
    return run('command',package,'--json',path)
def undo():
    return run('project','undo',package,'--expected',dump()['revision_id'])
d=dump()
audio={'type':'room_tone','source':{'asset':'camera','span':d['assets']['camera']['audio']}}
hold='core31-first-pause-hold'
command('core32-hold-room-tone', {'command':'set_hold_audio','node':hold,'audio':audio})
for sound in sorted(dump().get('sounds', {})):
    command('core32-remove-'+sound, {'command':'delete_sound','id':sound})
command('core32-hold-repeat', {'command':'wrap_repeat','node':hold,'id':'gain-history-repeat','plays':3,'gap':None})
d=dump()
print(json.dumps(d['nodes']['gain-history-repeat'],indent=2))
iteration={'allocation':'core32-hold-repeat','ordinal':1}
command('core32-isolated-silence', {'command':'edit_occurrence','instance':{'node':hold,'repeats':[{'node':'gain-history-repeat','iteration':iteration}]},'edit':{'type':'set_hold_audio','audio':{'type':'silence'}},'identities':{'nodes':['gain-history-isolated-hold'],'marks':[]}})
undo()
command('core32-abandoned-tail', {'command':'set_hold_audio','node':hold,'audio':{'type':'tail','source':audio['source'],'maximum':1}})
undo()
command('core32-pending-silence', {'command':'set_hold_audio','node':hold,'audio':{'type':'silence'}})
undo()
run('project','validate',package)
run('project','checkpoint',package)
database=sqlite3.connect(package/'project.sqlite')
assert database.execute('pragma user_version').fetchone()[0] == 38
assert database.execute("select distinct json_extract(document,'$.schema_version') from revisions").fetchall() == [(32,)]
header = '\n'.join([
    '-- Authentic database schema 38 / core schema 32 history.',
    '-- Migrated v37-hold-audio-history.sql through verified CLI at 5fd8b23,',
    '-- then authored direct Hold audio, Repeat occurrence isolation, branches and pending redo.',
    '-- CLI SHA-256: '+hashlib.file_digest(cli.open('rb'),'sha256').hexdigest(),
    '-- Captured 2026-09-28. No modern snapshot relabeling or authored SQL modifications.',
    'PRAGMA application_id=1146113585;', 'PRAGMA user_version=38;', ''])
target=root/'crates/deadpan-store/tests/fixtures/v38-gain-history.sql'
target.write_text(header+'\n'.join(database.iterdump())+'\n')
print(json.dumps({'fixture':str(target),'sha256':hashlib.file_digest(target.open('rb'),'sha256').hexdigest(),'revisions':database.execute('select count(*) from revisions').fetchone()[0],'history':database.execute('select count(*) from history').fetchone()[0]}))
