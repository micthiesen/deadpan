#!/usr/bin/env python3
"""Render and independently verify the actual native-authored Repeat-following replay project."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--project', type=Path, required=True)
parser.add_argument('--bin', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir()
cli = args.bin / 'deadpan-cli'
commands = []


def run(name, *arguments):
    command = [str(cli), *map(str, arguments)]
    started = time.monotonic()
    row = {'name': name, 'command': command}
    commands.append(row)
    (args.output / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
    with (args.output / f'{name}.stdout').open('xb') as stdout:
        with (args.output / f'{name}.stderr').open('xb') as stderr:
            result = subprocess.run(command, stdout=stdout, stderr=stderr)
    row.update({'exit_code': result.returncode, 'seconds': time.monotonic() - started})
    (args.output / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
    result.check_returncode()
    return (args.output / f'{name}.stdout').read_text()


document = json.loads(run('dump', 'project', 'dump', args.project, '--json'))
revision = document['revision_id']
repeats = [(name, node) for name, node in document['nodes'].items()
           if node['kind']['type'] == 'repeat']
assert len(repeats) == 1
owner, repeat = repeats[0]
assert owner != document['root']
assert document['nodes'][document['root']]['kind'] == {'type': 'sequence', 'children': [owner]}
assert not document.get('overrides', {})
assert not document.get('gap_overrides', {})
kind = repeat['kind']
runs = kind['iterations']['runs']
assert len(runs) == 1
assert runs[0]['first'] == 0 and runs[0]['count'] == 3
assert isinstance(runs[0]['allocation'], str) and runs[0]['allocation']
gap = kind['gap']
assert gap['duration'] == 4
assert gap['audio'] == {'type': 'silence'}
assert gap['video']['type'] == 'freeze'
assert gap['video']['timestamp'] == {
    'ticks': 23023, 'time_base': {'numerator': 1, 'denominator': 30000}}
basis = document['presentation_basis']
assert (basis['width'], basis['height']) == (320, 180)
assert gap['picture_context'] == {'canvases': [{
    'width': basis['width'], 'height': basis['height'], 'fit': 'fit', 'layers': []}]}
# Unity gain, additive progression and absent target are omitted serde defaults.
assert kind['escalation'] == {'zoom': {'step': {'numerator': '1', 'denominator': '8'}}}


def duration(name):
    node = document['nodes'][name]['kind']
    if node['type'] == 'source':
        return node['source']['duration']
    if node['type'] == 'hold':
        return node['recipe']['duration']
    if node['type'] == 'retime':
        return node['duration']
    if node['type'] == 'sequence':
        return sum(duration(child) for child in node['children'])
    assert node['type'] == 'repeat'
    plays = sum(run['count'] for run in node['iterations']['runs'])
    return duration(node['child']) * plays + (node.get('gap') or {}).get('duration', 0) * (plays - 1)


assert duration(kind['child']) == 24
assert duration(owner) == duration(document['root']) == 80
assert [[28 * play, 28 * play + 24] for play in range(3)] == [[0, 24], [28, 52], [56, 80]]
framing = repeat['framing']
assert 'clock' not in framing  # OwnerOutput is the omitted serde default.
assert framing['value']['type'] == 'follow'
assert framing['value']['target'] == 'target-1'
assert framing['value']['scale'] == {'numerator': '11', 'denominator': '10'}
assert framing['value']['fallback'] == {
    'center_x': {'numerator': '311385129', 'denominator': '536870912'},
    'center_y': {'numerator': '1', 'denominator': '2'},
    'scale': {'numerator': '11', 'denominator': '10'}}
assert framing['value']['scale_transition'] == {'factor': {'numerator': '2', 'denominator': '1'}, 'curve': 'linear'}
assert all(not node.get('framing') for name, node in document['nodes'].items() if name != owner)
assert len(document['targets']) == 1
target = document['targets']['target-1']
assert target['region'] == {'center': [580000, 500000]}
assert not target.get('samples', [])
assert target['corrections'] == [{'at': 12012, 'region': {'center': [680000, 500000]}}]
assert target['display_aspect'] == {'numerator': '16', 'denominator': '9'}
assert target['span'] == {
    'start': {'ticks': 0, 'time_base': {'numerator': 1, 'denominator': 30000}},
    'end': {'ticks': 120120, 'time_base': {'numerator': 1, 'denominator': 30000}}}
assert target['asset'] == gap['video']['asset']
assert all(node['kind']['source']['video'].get('asset') == target['asset']
           for node in document['nodes'].values() if node['kind']['type'] == 'source')
basis_rate = basis['frame_rate']
assert basis_rate == {'numerator': 30000, 'denominator': 1001}
audio_samples = 80 * basis_rate['denominator'] * 48000 // basis_rate['numerator']
assert audio_samples == 128128
run('validate', 'project', 'validate', args.project)
run('render', 'render', args.project, '--output', args.output, '--name', 'repeat-follow-creep.mp4', '--expected', revision)
run('verify', 'verify-export', args.project, '--movie', args.output / 'repeat-follow-creep.mp4',
    '--revision', revision, '--every', '1', '--samples', f'0:{audio_samples}',
    '--report', args.output / 'verification.json')
portable = args.output / 'portable.deadpan'
run('copy', 'project', 'copy-portable', args.project, portable)
run('portable-validate', 'project', 'validate', portable)
copied = json.loads(run('portable-dump', 'project', 'dump', portable, '--json'))
assert copied == document
run('checkpoint', 'project', 'checkpoint', args.project)
run('portable-checkpoint', 'project', 'checkpoint', portable)
movie = args.output / 'repeat-follow-creep.mp4'
assert len(commands) == 9
summary = {'fixture': 'actual native-authored Repeat-following replay project',
           'project': str(args.project), 'owner': owner,
           'revision': revision, 'portable_document_equal': True,
           'movie_bytes': movie.stat().st_size,
           'movie_sha256': hashlib.sha256(movie.read_bytes()).hexdigest(),
           'commands': len(commands), 'expected_frames': 80, 'expected_audio_samples': audio_samples,
           'expected_play_ranges': [[0, 24], [28, 52], [56, 80]],
           'expected_gap_ranges': [[24, 28], [52, 56]],
           'target_correction_pts': 12012}
(args.output / 'result.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary))
