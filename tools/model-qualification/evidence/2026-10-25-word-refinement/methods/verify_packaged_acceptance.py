#!/usr/bin/env python3
"""One real packaged proposal/accept/Undo round trip with an isolated home."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

p = argparse.ArgumentParser()
p.add_argument('--cli', type=Path, required=True)
p.add_argument('--source', type=Path, required=True)
p.add_argument('--models', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
a.output.mkdir(exist_ok=False)
home = a.output / 'home'
home.mkdir()
env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'TMPDIR': '/tmp'}
records = []
script_sha = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def run(name, args, success=True):
    command = [str(a.cli)] + [str(v) for v in args]
    start = time.monotonic()
    result = subprocess.run(command, capture_output=True, env=env, timeout=180)
    (a.output / (name + '.stdout')).write_bytes(result.stdout)
    (a.output / (name + '.stderr')).write_bytes(result.stderr)
    records.append({'name': name, 'command': command, 'exit_code': result.returncode,
                    'seconds': time.monotonic() - start, 'expected_success': success})
    (a.output / 'runs.json').write_text(json.dumps({
        'script_sha256': script_sha, 'environment': env, 'records': records}, indent=2) + '\n')
    assert (result.returncode == 0) == success, (name, result.stderr)
    return result.stdout


project = a.output / 'Accepted.deadpan'
run('copy', ['project', 'copy-portable', a.source, project])
before_doc = json.loads(run('document-before', ['project', 'dump', project, '--json']))
before = json.loads(run('transcript-before', ['transcript', project]))
proposed = json.loads(run('proposal', ['refine-words', project, '--word', '0',
                                     '--models-root', a.models]))
assert proposed['committed'] is False and proposed['proposal_id']
run('accept', ['refine-words', project, '--word', '0', '--models-root', a.models,
               '--accept', proposed['proposal_id']])
accepted = json.loads(run('transcript-accepted', ['transcript', project]))
accepted_doc = json.loads(run('document-accepted', ['project', 'dump', project, '--json']))
assert accepted['corrections_version'] == before['corrections_version'] + 1
old = before['transcripts'][0]
new = accepted['transcripts'][0]
assert old['corrected_words'] == [4] and new['corrected_words'] == [0, 4]
assert old['transcript']['words'][1:] == new['transcript']['words'][1:]
assert old['transcript']['words'][0]['text'] == new['transcript']['words'][0]['text']
bounds = proposed['offers'][0]['centisecond_bounds']
assert (new['transcript']['words'][0]['start_cs'], new['transcript']['words'][0]['end_cs']) == (
    bounds['start_cs'], bounds['end_cs'])
assert before_doc == accepted_doc
undo = a.output / 'undo.json'
undo.write_text(json.dumps({'protocol': 1, 'expected_version': accepted['corrections_version'],
                            'change': {'type': 'undo'}}) + '\n')
run('undo', ['corrections', project, '--json', undo])
undone = json.loads(run('transcript-undone', ['transcript', project]))
undone_doc = json.loads(run('document-undone', ['project', 'dump', project, '--json']))
assert undone['transcripts'] == before['transcripts']
assert before_doc == undone_doc
run('pinned-refusal', ['refine-words', project, '--word', '4', '--models-root', a.models], False)
(a.output / 'verification.json').write_text(json.dumps({
    'passed': True, 'proposal_id': proposed['proposal_id'], 'bounds': bounds,
    'correction_versions': [before['corrections_version'], accepted['corrections_version'],
                            undone['corrections_version']],
    'pinned_neighbor_unchanged': True, 'other_words_unchanged': True,
    'authored_document_unchanged': True, 'one_undo_restores_all_words': True,
    'revision': before_doc['revision_id'], 'scope': 'Workflow, not phonetic accuracy.'
}, indent=2) + '\n')
print('Packaged proposal, explicit acceptance and one Undo verified.')
