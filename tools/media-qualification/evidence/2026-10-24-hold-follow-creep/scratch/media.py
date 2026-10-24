#!/usr/bin/env python3
"""Render and independently verify the project saved by the Hold-following replay."""
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
holds = [node for node in document['nodes'].values() if node['kind']['type'] == 'hold']
assert len(holds) == 1
held = holds[0]
framing = held['framing']
assert framing['value']['type'] == 'follow'
assert framing['value']['target'] == 'target-2'
assert framing['value']['scale'] == {'numerator': '11', 'denominator': '10'}
assert framing['value']['scale_transition'] == {'factor': {'numerator': '2', 'denominator': '1'}, 'curve': 'linear'}
assert held['kind']['recipe']['picture_context']['canvases']
assert len(document['targets']) == 2
assert document['targets']['target-1']['region'] == {'center': [580000, 500000]}
assert document['targets']['target-2']['region'] == {'center': [680000, 500000]}
assert not document['targets']['target-2'].get('corrections', [])
basis_rate = document['presentation_basis']['frame_rate']
audio_samples = 132 * basis_rate['denominator'] * 48000 // basis_rate['numerator']
assert audio_samples == 211411
run('validate', 'project', 'validate', args.project)
run('render', 'render', args.project, '--output', args.output, '--name', 'hold-follow-creep.mp4', '--expected', revision)
run('verify', 'verify-export', args.project, '--movie', args.output / 'hold-follow-creep.mp4',
    '--revision', revision, '--every', '1', '--samples', f'0:{audio_samples}',
    '--report', args.output / 'verification.json')
portable = args.output / 'portable.deadpan'
run('copy', 'project', 'copy-portable', args.project, portable)
run('portable-validate', 'project', 'validate', portable)
copied = json.loads(run('portable-dump', 'project', 'dump', portable, '--json'))
assert copied == document
run('checkpoint', 'project', 'checkpoint', args.project)
run('portable-checkpoint', 'project', 'checkpoint', portable)
movie = args.output / 'hold-follow-creep.mp4'
summary = {'revision': revision, 'portable_document_equal': True,
           'movie_bytes': movie.stat().st_size,
           'movie_sha256': hashlib.sha256(movie.read_bytes()).hexdigest(),
           'commands': len(commands), 'expected_frames': 132, 'expected_audio_samples': audio_samples}
(args.output / 'result.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary))
