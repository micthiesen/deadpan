#!/usr/bin/env python3
"""Render and independently verify the actual native-authored Retime-following replay project."""
import argparse
import hashlib
import json
from fractions import Fraction
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
root_kind = document['nodes'][document['root']]['kind']
assert root_kind['type'] == 'sequence' and len(root_kind['children']) == 1
owner = root_kind['children'][0]
retime = document['nodes'][owner]
kind = retime['kind']
assert kind == {
    'type': 'retime', 'child': kind['child'], 'duration': 120,
    'mapping': {'start': 0, 'end': 80}, 'pitch': 'follow_speed'}
# RetimePurpose::Edit is omitted; tape pitch is PitchPolicy::FollowSpeed.
assert not document.get('overrides', {})
assert not document.get('gap_overrides', {})
group = document['nodes'][kind['child']]['kind']
assert group['type'] == 'sequence' and len(group['children']) == 2
assert not any(node['kind']['type'] == 'repeat' for node in document['nodes'].values())


def duration(name):
    node = document['nodes'][name]['kind']
    if node['type'] == 'source':
        return node['source']['duration']
    if node['type'] == 'hold':
        return node['recipe']['duration']
    if node['type'] == 'retime':
        return node['duration']
    assert node['type'] == 'sequence'
    return sum(duration(child) for child in node['children'])


assert [duration(child) for child in group['children']] == [40, 40]
assert duration(kind['child']) == 80
assert duration(owner) == duration(document['root']) == 120
assert Fraction(kind['mapping']['end'] - kind['mapping']['start'],
                kind['duration']) == Fraction(2, 3)
framing = retime['framing']
assert 'clock' not in framing  # OwnerOutput is the omitted serde default.
assert framing['value']['type'] == 'follow'
assert framing['value']['target'] == 'target-1'
assert framing['value']['scale'] == {'numerator': '11', 'denominator': '10'}
assert framing['value']['fallback'] == {
    'center_x': {'numerator': '311385129', 'denominator': '536870912'},
    'center_y': {'numerator': '1', 'denominator': '2'},
    'scale': {'numerator': '11', 'denominator': '10'}}
assert framing['value']['scale_transition'] == {
    'factor': {'numerator': '2', 'denominator': '1'}, 'curve': 'linear'}
assert all(not node.get('framing') for name, node in document['nodes'].items()
           if name != owner)
assert len(document['targets']) == 1
target = document['targets']['target-1']
assert target['region'] == {'center': [580000, 500000]}
assert not target.get('samples', [])
assert target['corrections'] == [
    {'at': 80080, 'region': {'center': [680000, 500000]}}]
assert target['display_aspect'] == {'numerator': '16', 'denominator': '9'}
clock = {'numerator': 1, 'denominator': 30000}
assert target['span'] == {
    'start': {'ticks': 0, 'time_base': clock},
    'end': {'ticks': 120120, 'time_base': clock}}
basis = document['presentation_basis']
assert (basis['width'], basis['height']) == (320, 180)
basis_rate = basis['frame_rate']
assert basis_rate == {'numerator': 30000, 'denominator': 1001}


def fraction(value):
    return Fraction(int(value['numerator']), int(value['denominator']))


def original_bounds(name, start, end):
    # Native delete retains transparent unity-rate partition wrappers. Follow
    # their actual maps; do not infer Original coordinates from child order.
    node = document['nodes'][name]['kind']
    if node['type'] == 'retime':
        assert node['purpose'] == 'partition'
        assert node['pitch'] == 'preserve'
        mapping = node['mapping']
        rate = Fraction(mapping['end'] - mapping['start'], node['duration'])
        assert rate == 1
        return original_bounds(node['child'],
                               mapping['start'] + start * rate,
                               mapping['start'] + end * rate)
    assert node['type'] == 'source'
    source = node['source']
    video = source['video']
    assert video['type'] == 'stream' and video['asset'] == target['asset']
    span = video['span']
    assert span['start']['time_base'] == span['end']['time_base'] == clock
    mapping = source['video_mapping']
    if mapping['type'] == 'fit_beat':
        placement, frames = Fraction(0), Fraction(source['duration'])
    else:
        assert mapping['type'] in ('duration', 'placement', 'selected_placement')
        placement = fraction(mapping['start']) if 'start' in mapping else Fraction(0)
        frames = fraction(mapping['frames'])
    # The fixture's measured CFR tick step is1001 in its 1/30000 clock.
    origin = Fraction(span['start']['ticks'], 1001)
    length = Fraction(span['end']['ticks'] - span['start']['ticks'], 1001)
    return (origin + (start - placement) * length / frames,
            origin + (end - placement) * length / frames)


original_ranges = [original_bounds(child, Fraction(0), Fraction(40))
                   for child in group['children']]
assert original_ranges == [(Fraction(0), Fraction(40)),
                           (Fraction(80), Fraction(120))]
audio_samples = 120 * basis_rate['denominator'] * 48000 // basis_rate['numerator']
assert audio_samples == 192192
run('validate', 'project', 'validate', args.project)
run('render', 'render', args.project, '--output', args.output, '--name', 'retime-follow-creep.mp4', '--expected', revision)
run('verify', 'verify-export', args.project, '--movie', args.output / 'retime-follow-creep.mp4',
    '--revision', revision, '--every', '1', '--samples', f'0:{audio_samples}',
    '--report', args.output / 'verification.json')
portable = args.output / 'portable.deadpan'
run('copy', 'project', 'copy-portable', args.project, portable)
run('portable-validate', 'project', 'validate', portable)
copied = json.loads(run('portable-dump', 'project', 'dump', portable, '--json'))
assert copied == document
run('checkpoint', 'project', 'checkpoint', args.project)
run('portable-checkpoint', 'project', 'checkpoint', portable)
movie = args.output / 'retime-follow-creep.mp4'
assert len(commands) == 9
summary = {
    'fixture': 'actual native-authored Retime-following replay project',
    'fixture_origin': 'native replay project; this driver constructs no document',
    'derived_fixture': False,
    'project': str(args.project), 'owner': owner, 'child_group': kind['child'],
    'revision': revision, 'portable_document_equal': True,
    'movie_bytes': movie.stat().st_size,
    'movie_sha256': hashlib.sha256(movie.read_bytes()).hexdigest(),
    'commands': len(commands), 'expected_frames': 120,
    'expected_audio_samples': audio_samples,
    'expected_input_frames': 80, 'expected_input_speed': '2/3',
    'expected_pitch_wire': 'follow_speed',
    'asserted_original_ranges': [[str(a), str(b)] for a, b in original_ranges],
    'expected_mapped_cut_edit': 60, 'target_correction_pts': 80080}
(args.output / 'result.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary))
