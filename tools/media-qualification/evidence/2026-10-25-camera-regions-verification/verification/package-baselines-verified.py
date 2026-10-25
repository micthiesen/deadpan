from pathlib import Path
import hashlib
import json
import runpy
import subprocess

s = Path(__file__).resolve().parent
root = Path('/Users/michael/Code/deadpan')
out = s / 'package-baselines-verified'
out.mkdir()
media = runpy.run_path(str(s / 'media.py'))
package = runpy.run_path(str(s / 'package-checks.py'))
project = (s / 'native-regions-verified/projects/regions/Documents/Deadpan/salient-regions.deadpan').resolve()
cli = s / 'bin-verified/deadpan-cli'
manifest_path = root / 'native/deadpan-track/tests/fixtures/salient-regions-fixtures.json'
fixture = root / 'native/deadpan-track/tests/fixtures/salient-regions.mp4'
manifest = json.loads(manifest_path.read_text())
index = manifest['fixtures'][fixture.name]['frames']
identity_report = json.loads((s / 'media-verified/detect-mug-both.stdout').read_text())

def pin(path):
    return {'path': str(path.resolve()), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

before, _, _ = media['authority'](project)
db_before = media['database_bytes'](project)
cases = {}
commands = []
for name, section in zip(['blank', 'mug', 'two'], manifest['sections'], strict=True):
    ordinal = section['start']
    for both in [False, True]:
        mode = 'both' if both else 'regions'
        label = f'{name}-{mode}'
        command = [str(cli), 'detect-regions', str(project), '--asset', identity_report['asset'], '--at', str(index[ordinal]['pts'])]
        if both:
            command.append('--include-faces')
        with (out / f'{label}.stdout').open('xb') as stdout, (out / f'{label}.stderr').open('xb') as stderr:
            result = subprocess.run(command, stdout=stdout, stderr=stderr, check=False)
        commands.append({'label': label, 'argv': command, 'exit_code': result.returncode})
        (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
        result.check_returncode()
        found = json.loads((out / f'{label}.stdout').read_text())
        assert found['picture'] == ordinal and found['pts'] == index[ordinal]['pts']
        assert found['revision'] == before['head']
        assert media['authority'](project)[0] == before
        assert media['database_bytes'](project) == db_before
        cases[label] = pin(out / f'{label}.stdout')

source = pin(s / 'package-verified-source-manifest.json')
config = {
    'source_manifest': source, 'source_manifest_key': 'files',
    'target_directory': str(root / 'target'),
    'ffmpeg_prefix': '/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',
    'project': str(project), 'native_report': pin(s / 'native-regions-verified/report.json'),
    'identities': {key: identity_report[key] for key in ['content', 'asset', 'revision']},
    'asset_argument': True,
    'baseline_provenance': {'source_manifest_sha256': source['sha256'], 'cli': pin(cli), 'worker': pin(s / 'bin-verified/deadpan-track')},
    'baseline_reports': cases, 'fixture': pin(fixture), 'fixture_manifest': pin(manifest_path),
}
config['identities'].update({key: identity_report['stream'][key] for key in ['stream_index', 'rotation_quarter_turns']})
config['identities']['face_request_revision'] = identity_report['faces']['runtime']['request_revision']
measured = package['fixture_input'](config)
native = json.loads((s / 'native-regions-verified/report.json').read_text())
package['native_project_binding'](native, str(project), measured, config['identities'])
for record in cases.values():
    package['assert_report'](json.loads(Path(record['path']).read_text()), config['identities'], measured)
assert media['authority'](project)[0] == before
assert media['database_bytes'](project) == db_before
with (s / 'package-verified-config.json').open('x') as output:
    json.dump(config, output, indent=2)
    output.write('\n')
(out / 'result.json').write_text(json.dumps({'six_read_only_cases': True, 'state_unchanged': True, 'before': before, 'config': pin(s / 'package-verified-config.json'), 'driver': pin(Path(__file__))}, indent=2) + '\n')
print(json.dumps({'six_read_only_cases': True, 'state_unchanged': True, 'config': pin(s / 'package-verified-config.json')}))
