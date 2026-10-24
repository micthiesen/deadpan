#!/usr/bin/env python3
"""CLI-derived clipping witness on a portable copy; NEVER native-authored evidence."""
import argparse
import copy
from datetime import datetime, timezone
from fractions import Fraction as F
import hashlib
import json
from pathlib import Path
import stat
import subprocess
import time
import traceback
import uuid

Q = 1 << 32
FRAMES = 80
SAMPLES = 128128


def require(ok, reason):
    if not ok:
        raise AssertionError(reason)


def identity(path):
    h = hashlib.sha256()
    size = 0
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
            size += len(chunk)
    return {'bytes': size, 'sha256': h.hexdigest()}


def ratio(value):
    return F(int(value['numerator']), int(value['denominator']))


def wire(value):
    value = F(value)
    return {'numerator': str(value.numerator), 'denominator': str(value.denominator)}


def grid(value):
    # Python Fraction rounds exact integer ties to even, independently of Rust.
    return F(round(F(value) * Q), Q)


def expected_scale(frame, base):
    start = grid(base)
    end = grid(base * 2)
    progress = grid(F(2 * frame + 1, 2 * FRAMES))
    return grid(start + (end - start) * progress)


def without_revision(value):
    result = copy.deepcopy(value)
    result.pop('revision_id', None)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--project', type=Path, required=True)
    parser.add_argument('--bin', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--approved-script-sha256', required=True)
    args = parser.parse_args()
    script = Path(__file__).resolve()
    script_pin = identity(script)
    require(script_pin['sha256'] == args.approved_script_sha256, 'script changed')
    native = args.project.resolve(strict=True)
    binaries = args.bin.resolve(strict=True)
    output = args.output.resolve()
    require(not output.exists(), 'output must be new')
    require(not output.is_relative_to(native) and not native.is_relative_to(output),
            'output and native project must be separate')
    output.mkdir()
    cli = binaries / 'deadpan-cli'
    binary_pins = {str(p): identity(p) for p in sorted(binaries.iterdir()) if p.is_file()}
    require(str(cli) in binary_pins, 'missing production CLI')
    commands = []

    def save(name, value):
        (output / name).write_text(json.dumps(value, indent=2) + '\n')

    def native_authoritative_pins():
        # SQLite SHM is a volatile reader coordination area. No mutation command,
        # render, checkpoint or history operation ever targets the native source.
        # All persistent regular package files, including SQLite and WAL, are pinned.
        return {str(p.relative_to(native)): identity(p)
                for p in sorted(native.rglob('*'))
                if stat.S_ISREG(p.lstat().st_mode) and p.name != 'project.sqlite-shm'}

    def run(name, *arguments):
        require(all(row['name'] != name for row in commands), 'duplicate command name')
        argv = [str(cli), *map(str, arguments)]
        row = {'name': name, 'argv': argv,
               'started_at': datetime.now(timezone.utc).isoformat()}
        commands.append(row)
        save('commands.json', commands)
        started = time.monotonic()
        outpath, errpath = output / (name + '.stdout'), output / (name + '.stderr')
        with outpath.open('xb') as stdout, errpath.open('xb') as stderr:
            result = subprocess.run(argv, stdout=stdout, stderr=stderr, timeout=240)
        row.update(exit_code=result.returncode, seconds=time.monotonic() - started,
                   stdout=identity(outpath), stderr=identity(errpath))
        save('commands.json', commands)
        require(result.returncode == 0, f'{name} exited {result.returncode}')
        text = outpath.read_text()
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            # render emits one JSON event per line, while inspection verbs
            # emit one pretty-printed JSON document. Retain every raw byte.
            events = [json.loads(line) for line in text.splitlines() if line.strip()]
            require(events and all(isinstance(event, dict) for event in events),
                    f'{name} did not emit JSON events')
            return events

    native_before = native_authoritative_pins()
    save('inputs.json', {
        'label': 'CLI-derived, not native-authored',
        'script': script_pin, 'binaries': binary_pins,
        'native_project': str(native), 'native_authoritative_before': native_before,
        'volatile_file_excluded': 'project.sqlite-shm',
        'environment': {k: v for k, v in __import__('os').environ.items()
                        if k in ('DEADPAN_FFMPEG_PREFIX', 'CARGO_TARGET_DIR',
                                 'DEADPAN_MEDIA_WORKER', 'DEADPAN_REQUIRE_PHYSICAL_MAC')},
    })
    status = 'failed'
    try:
        native_document = run('native-document-before', 'project', 'dump', native, '--json')
        project = output / 'cli-derived-clipped.deadpan'
        run('copy-native', 'project', 'copy-portable', native, project)
        before = run('copy-document-before', 'project', 'dump', project, '--json')
        require(before == native_document, 'portable copy changed native document')
        owners = [(key, node) for key, node in before['nodes'].items()
                  if node['kind']['type'] == 'repeat'
                  and (node.get('framing') or {}).get('value', {}).get('type') == 'follow']
        require(len(owners) == 1, 'expected one followed Repeat')
        owner, repeated = owners[0]
        require(before['nodes'][before['root']]['kind']['children'] == [owner],
                'Repeat must be the sole ordinary root child')
        require(not before.get('overrides') and not before.get('gap_overrides'),
                'unexpected occurrence overrides')
        require(all(not node.get('framing') for key, node in before['nodes'].items()
                    if key != owner), 'unexpected independent child framing')
        framing = repeated['framing']
        require(framing.get('clock', {'type': 'owner_output'}) == {'type': 'owner_output'},
                'unexpected framing clock')
        follow = framing['value']
        target_id = follow['target']
        require(target_id == 'target-1' and len(before['targets']) == 1, 'unexpected target')
        require(ratio(follow['scale']) == F(11, 10), 'unexpected live base')
        require(follow['scale_transition'] == {
            'factor': wire(2), 'curve': 'linear'}, 'unexpected transition')
        fallback = follow['fallback']
        require(ratio(fallback['center_x']) == grid(F(58, 100))
                and ratio(fallback['center_y']) == F(1, 2)
                and ratio(fallback['scale']) == F(11, 10), 'unexpected saved fallback')
        require(ratio(fallback['center_x']) == F(311385129, 536870912),
                'saved native fallback differs from inspected rational')
        basis = before['presentation_basis']
        require((basis['width'], basis['height']) == (320, 180), 'unexpected canvas')
        require(basis['frame_rate'] == {'numerator': 30000, 'denominator': 1001},
                'unexpected frame rate')
        escalation = repeated['kind']['escalation']
        require(ratio(escalation['zoom']['step']) == F(1, 8)
                and escalation['zoom'].get('progression', 'add') == 'add'
                and escalation['zoom'].get('target') is None, 'unexpected escalation')
        gap = repeated['kind']['gap']
        require(gap['duration'] == 4 and gap['audio'] == {'type': 'silence'},
                'unexpected default gap')
        expected_context = {'canvases': [{
            'width': 320, 'height': 180, 'fit': 'fit', 'layers': []}]}
        require(gap['picture_context'] == expected_context,
                'captured gap is not exact identity Fit on current canvas')
        require(gap['video']['type'] == 'freeze'
                and gap['video']['timestamp'] == {
                    'ticks': 23023, 'time_base': {'numerator': 1, 'denominator': 30000}},
                'gap does not freeze Original23')
        target = before['targets'][target_id]
        require(target['region'] == {'center': [580000, 500000]}
                and not target.get('samples'), 'unexpected initial point/samples')
        require(target['corrections'] == [{
            'at': 12012, 'region': {'center': [680000, 500000]}}],
            'unexpected retained correction')
        require(ratio(target['display_aspect']) == F(16, 9), 'unexpected measured aspect')
        require(gap['video']['asset'] == target['asset'], 'gap target asset mismatch')
        plan_before = run('plan-before', 'inspect-plan', project)
        require(plan_before['plan']['metadata']['duration'] == 80, 'unexpected duration')
        audio_before = run('audio-plan-before', 'inspect-plan', project,
                           '--audio-samples', 0, SAMPLES)
        frames_before = [run(f'frame-before-{j:03}', 'inspect-plan', project,
                             '--frame', j)['sample'] for j in range(FRAMES)]

        changed_target = copy.deepcopy(target)
        changed_target['corrections'][0]['region']['center'][0] = 980000
        new_revision = str(uuid.uuid4())
        request = {'protocol': 1, 'project_id': before['project_id'],
                   'expected_revision': before['revision_id'], 'new_revision': new_revision,
                   'command': {'command': 'set_target', 'id': target_id,
                               'target': changed_target}}
        request_path = output / 'set-target-request.json'
        save(request_path.name, request)
        run('set-target', 'command', project, '--json', request_path)
        after = run('copy-document-after', 'project', 'dump', project, '--json')
        expected_document = copy.deepcopy(before)
        expected_document['revision_id'] = new_revision
        expected_document['targets'][target_id] = changed_target
        require(after == expected_document, 'SetTarget changed more than one correction and revision')
        run('validate-derived', 'project', 'validate', project)
        plan_after = run('plan-after', 'inspect-plan', project)
        a, b = copy.deepcopy(plan_before), copy.deepcopy(plan_after)
        a['plan']['metadata'].pop('revision_id')
        b['plan']['metadata'].pop('revision_id')
        require(a == b, 'complete plan inspection changed')
        audio_after = run('audio-plan-after', 'inspect-plan', project,
                          '--audio-samples', 0, SAMPLES)
        require(without_revision(audio_before['audio']) ==
                without_revision(audio_after['audio']), 'complete audio plan changed')

        clipped, observations = [], []
        for j in range(FRAMES):
            sample = run(f'frame-after-{j:03}', 'inspect-plan', project, '--frame', j)['sample']
            original = frames_before[j]
            play, within = divmod(j, 28)
            in_gap = within >= 24
            ordinal = 23 if in_gap else within
            zoom = F(8 + play, 8)
            percent = 58 if ordinal < 12 else 98
            projected = F(1, 2) + (F(percent, 100) - F(1, 2)) * zoom
            visible = 0 <= projected <= 1
            if not visible:
                clipped.append(j)
            pose_layers = [v for v in sample['framing']
                           if v['instance']['node'] == owner and not v['escalation']]
            require(len(pose_layers) == 1, f'outer Follow missing at{j}')
            layer = pose_layers[0]
            require(ratio(layer['local_position']) == F(2*j+1, 2)
                    and ratio(layer['effective_duration']) == 80, f'owner clock at{j}')
            pose = layer['pose']
            expected_x = grid(projected) if visible else grid(ratio(fallback['center_x']))
            expected_y = F(1, 2) if visible else grid(ratio(fallback['center_y']))
            base = ratio(follow['scale']) if visible else ratio(fallback['scale'])
            require(ratio(pose['center_x']) == expected_x
                    and ratio(pose['center_y']) == expected_y
                    and ratio(pose['scale']) == expected_scale(j, base), f'independent pose at{j}')
            esc = [v for v in sample['framing'] if v['escalation']]
            require(len(esc) == (0 if play == 0 else 1), f'escalation layer at{j}')
            if esc:
                require(esc[0]['instance'] == layer['instance']
                        and ratio(esc[0]['pose']['center_x']) == F(1,2)
                        and ratio(esc[0]['pose']['center_y']) == F(1,2)
                        and ratio(esc[0]['pose']['scale']) == zoom, f'centered escalation at{j}')
            require(all(v['pose'] is None for v in sample['framing']
                        if v is not layer and not v['escalation']), f'nonidentity inner layer at{j}')
            picture = sample['picture']
            require(picture['asset'] == target['asset']
                    and picture['point']['time_base'] == {'numerator':1,'denominator':30000},
                    f'provider source identity at{j}')
            if in_gap:
                require(picture['type'] == 'freeze'
                        and ratio(picture['point']['ticks']) == 23023
                        and sample['picture_context'] == expected_context
                        and sample['instance'] == {'node': owner, 'repeats': []}
                        and sample['gap_after']['ordinal'] == play, f'identity captured gap at{j}')
            else:
                require(picture['type'] == 'source'
                        and ratio(picture['point']['ticks']) == F((2*ordinal+1)*1001,2)
                        and sample.get('picture_context') is None
                        and sample.get('gap_after') is None, f'Original reset at{j}')
                scopes = sample['instance']['repeats']
                require(len(scopes) == 1 and scopes[0]['node'] == owner
                        and scopes[0]['iteration']['ordinal'] == play, f'play scope at{j}')
            # Compare every sample field, removing only revision and the one
            # resolved outer pose that SetTarget is authorized to affect.
            x, y = without_revision(original), without_revision(sample)
            for value in (x,y):
                for v in value['framing']:
                    if v['instance']['node'] == owner and not v['escalation']:
                        v.pop('pose')
            require(x == y, f'complete non-outer sample fields changed at{j}')
            observations.append({'Edit': j, 'Original': ordinal, 'play': play, 'gap': in_gap,
                                 'source_percent': percent, 'escalation': wire(zoom),
                                 'projected_before_clip': wire(projected), 'visible': visible,
                                 'expected_center': wire(expected_x), 'actual_pose': pose})
        require(clipped == list(range(40,56)) + list(range(68,80)) and len(clipped) == 28,
                'unexpected clipped frame set')
        save('independent-plan-observations.json', observations)
        run('render', 'render', project, '--output', output,
            '--name', 'cli-derived-clipped-repeat.mp4', '--expected', new_revision)
        movie = output / 'cli-derived-clipped-repeat.mp4'
        report_path = output / 'verification.json'
        run('verify', 'verify-export', project, '--movie', movie,
            '--revision', new_revision, '--every', 1, '--samples', f'0:{SAMPLES}',
            '--report', report_path)
        report = json.loads(report_path.read_text())
        require(report['passed'] and not report['failures']
                and report['movie']['video_frames'] == FRAMES
                and report['movie']['expected_audio_samples'] == SAMPLES
                and report['summary']['pictures_checked'] == FRAMES
                and report['summary']['pictures_failed'] == 0, 'incomplete movie verification')
        require(len(report['pictures']) == FRAMES
                and [v['project_frame'] for v in report['pictures']] == list(range(FRAMES))
                and all(v['passed'] for v in report['pictures']), 'not every picture verified')
        for j, picture in enumerate(report['pictures']):
            within = j % 28
            ordinal = 23 if within >= 24 else within
            require(picture['provenance'] == {
                'kind':'original', 'asset':target['asset'],
                'source_frame':ordinal, 'source_pts':ordinal*1001}, f'encoded provenance at{j}')
        require(len(report['audio']) == 1 and report['audio'][0]['window'] == [0,SAMPLES]
                and report['audio'][0]['passed']
                and report['audio'][0]['uncovered_samples'] == 0
                and report['audio'][0]['offset_status'] == 'verified_zero'
                and report['audio'][0]['measured_offset_samples'] == 0, 'full zero-offset audio verification')

        portable = output / 'cli-derived-history-portable.deadpan'
        run('copy-derived', 'project', 'copy-portable', project, portable)
        copied = run('portable-dump', 'project', 'dump', portable, '--json')
        require(copied == after, 'portable document differs')
        run('portable-undo', 'project', 'undo', portable, '--expected', copied['revision_id'])
        undone = run('portable-undone-dump', 'project', 'dump', portable, '--json')
        require(without_revision(undone) == without_revision(before), 'portable Undo did not restore target')
        run('portable-redo', 'project', 'redo', portable, '--expected', undone['revision_id'])
        redone = run('portable-redone-dump', 'project', 'dump', portable, '--json')
        require(without_revision(redone) == without_revision(after), 'portable Redo changed content')
        run('portable-validate', 'project', 'validate', portable)
        run('portable-checkpoint', 'project', 'checkpoint', portable)
        native_end = run('native-document-after', 'project', 'dump', native, '--json')
        require(native_end == native_document, 'native authored document changed')
        require(native_authoritative_pins() == native_before, 'persistent native package bytes changed')
        require({str(p): identity(p) for p in sorted(binaries.iterdir()) if p.is_file()} ==
                binary_pins and identity(script) == script_pin, 'script/binaries changed during run')
        summary = {
            'label':'CLI-derived correction clipping qualification, NOT native-authored',
            'native_document_and_persistent_package_unchanged': True,
            'target_only_change': {'at':12012, 'from':[680000,500000], 'to':[980000,500000]},
            'saved_fallback_pose': fallback,
            'saved_fallback_center_q32': int(ratio(fallback['center_x'])*Q),
            'frames_checked': FRAMES, 'clipped_frames': clipped,
            'clipped_count': len(clipped), 'audio_samples_verified': SAMPLES,
            'audio_offset_samples': 0, 'audio_plan_equal': True,
            'portable_document_equal': True, 'portable_target_undo_redo_equal': True,
            'revision':new_revision, 'portable_redone_revision':redone['revision_id'],
            'movie':identity(movie), 'commands':len(commands),
            'limitations':['Plan pose oracle is independent; emitted pixel/audio comparison uses shared production verification.',
                           'No separate canonical PCM/gain-bit before/after comparison in this script.',
                           'SQLite SHM is volatile and excluded from persistent native byte equality.'],
        }
        save('result.json', summary)
        status = 'passed'
        print(json.dumps(summary))
    except BaseException as error:
        save('failure.json', {'error':repr(error), 'traceback':traceback.format_exc()})
        raise
    finally:
        save('terminal.json', {
            'status':status, 'finished_at':datetime.now(timezone.utc).isoformat(),
            'native_authoritative_after':native_authoritative_pins(),
            'native_authoritative_equal':native_authoritative_pins() == native_before,
            'script_after':identity(script),
            'binaries_after':{str(p):identity(p) for p in sorted(binaries.iterdir()) if p.is_file()},
            'commands':len(commands),
        })


if __name__ == '__main__':
    main()
