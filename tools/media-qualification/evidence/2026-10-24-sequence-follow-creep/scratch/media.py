#!/usr/bin/env python3
"""Render and independently verify the project saved by the Sequence-following replay."""
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
groups = [(name, node) for name, node in document['nodes'].items()
          if node['kind']['type'] == 'sequence' and (node.get('framing') or {}).get('value', {}).get('type') == 'follow']
assert len(groups) == 1
owner, group = groups[0]
assert owner != document['root']
assert document['nodes'][document['root']]['kind']['children'] == [owner]
assert len(group['kind']['children']) == 2
framing = group['framing']
assert 'clock' not in framing  # OwnerOutput is the omitted serde default.
assert framing['value']['target'] == 'target-1'
assert framing['value']['scale'] == {'numerator': '11', 'denominator': '10'}
assert framing['value']['fallback']['scale'] == {'numerator': '11', 'denominator': '10'}
assert framing['value']['scale_transition'] == {'factor': {'numerator': '2', 'denominator': '1'}, 'curve': 'linear'}
assert all(not node.get('framing') for name, node in document['nodes'].items() if name != owner)
assert len(document['targets']) == 1
target = document['targets']['target-1']
assert target['region'] == {'center': [580000, 500000]}
assert not target.get('samples', [])
assert len(target['corrections']) == 1
assert target['corrections'][0] == {'at': 80080, 'region': {'center': [680000, 500000]}}
basis_rate = document['presentation_basis']['frame_rate']
audio_samples = 80 * basis_rate['denominator'] * 48000 // basis_rate['numerator']
assert audio_samples == 128128
run('validate', 'project', 'validate', args.project)
run('render', 'render', args.project, '--output', args.output, '--name', 'sequence-follow-creep.mp4', '--expected', revision)
run('verify', 'verify-export', args.project, '--movie', args.output / 'sequence-follow-creep.mp4',
    '--revision', revision, '--every', '1', '--samples', f'0:{audio_samples}',
    '--report', args.output / 'verification.json')
portable = args.output / 'portable.deadpan'
run('copy', 'project', 'copy-portable', args.project, portable)
run('portable-validate', 'project', 'validate', portable)
copied = json.loads(run('portable-dump', 'project', 'dump', portable, '--json'))
assert copied == document
run('checkpoint', 'project', 'checkpoint', args.project)
run('portable-checkpoint', 'project', 'checkpoint', portable)
movie = args.output / 'sequence-follow-creep.mp4'
summary = {'revision': revision, 'portable_document_equal': True,
           'movie_bytes': movie.stat().st_size,
           'movie_sha256': hashlib.sha256(movie.read_bytes()).hexdigest(),
           'commands': len(commands), 'expected_frames': 80, 'expected_audio_samples': audio_samples}
(args.output / 'result.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary))
