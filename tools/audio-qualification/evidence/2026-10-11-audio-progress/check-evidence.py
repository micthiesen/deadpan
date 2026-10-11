from pathlib import Path
import gzip
import hashlib
import json
import re

root = Path(__file__).resolve().parent
repo = root.parents[3]

def read(name):
    path = root/name
    return json.loads(gzip.decompress(path.read_bytes()) if path.suffix=='.gz' else path.read_text())

def log(name):
    return gzip.decompress((root/(name+'.log.gz')).read_bytes()).decode()

def totals(name):
    rows = [tuple(map(int, row)) for row in re.findall(
        r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log(name))]
    assert rows and all(failed==ignored==0 for _,failed,ignored in rows), name
    return sum(passed for passed,_,_ in rows)

for name, meta in read('manifest.json').items():
    data = (root/name).read_bytes()
    assert len(data)==meta['bytes'] and hashlib.sha256(data).hexdigest()==meta['sha256'], name
for name, digest in read('source-pins.json')['files'].items():
    assert hashlib.sha256((repo/name).read_bytes()).hexdigest()==digest, name
for path in root.glob('*.log.gz'):
    receipt = read(path.name.removesuffix('.log.gz')+'.json')
    assert hashlib.sha256(gzip.decompress(path.read_bytes())).hexdigest()==receipt['log_sha256'], path
for name in ['check-final','audio-playback','transport','clippy','format-check','replays',
             'format-final','transport-final','controller-final','input-admission-final',
             'clippy-final','format-check-final','sound-replay-final',
             'layout-format','layout-clippy','layout-format-check','sound-layout-replay']:
    assert read(name+'.json')['exit']==0, name
assert read('check.json')['exit']!=0 and 'missing field `preparation`' in log('check')
assert read('progress-tests.json')['exit']!=0 and 'audio stream clock stopped advancing' in log('progress-tests')
assert totals('transport')==0, 'wrong-target command must not become test evidence'
assert totals('audio-playback')==576
assert totals('transport-final')==21
assert totals('controller-final')==totals('input-admission-final')==1
assert 'preparation_activity_is_request_bound_and_never_moves_the_playback_clock ... ok' in log('transport-final')
assert 'preparation_progress_is_live_without_device_activation_and_supersession_stays_responsive ... ok' in log('controller-final')
assert 'shortened_streamed_output_still_admits_unused_input_dependencies ... ok' in log('input-admission-final')
summary=read('replay-summary.json')
assert len(summary)==6
for entry in summary:
    report=read(entry['run']+'-'+entry['scenario']+'.json.gz')
    assert not report['failed'] and not entry['failed']
    assert report['metadata']['binary_sha256']==entry['binary_sha256']
    checks=[check for s in report['scenarios'] for check in s['checks']]
    assert len(checks)==entry['checks'] and all(check['passed'] for check in checks)
    for s in report['scenarios']:
        assert not s['failed']
        # The report's skipped list describes explicitly unmeasured OS/device
        # behavior. Actual scenario assertions must all pass above.
        if s['name'] != 'kestrel-shortcuts':
            assert len(s['skipped'])==3 and 'audio device output' in s['skipped'][-1]
    for selected in entry['selected_images']:
        assert (root/selected['file']).is_file()
assert sum(e['checks'] for e in summary if e['run']=='replays')==963
intermediate=next(e for e in summary if e['run']=='sound-replay-final')
assert intermediate['checks']==59 and len(intermediate['selected_images'])==2
final=next(e for e in summary if e['run']=='sound-layout-replay')
assert final['checks']==61 and len(final['selected_images'])==2
for name in ['docs/PLAYBACK.md','docs/AUDIO_STAGE_PREPARATION.md','docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md']:
    assert 'audio-progress-2026-10-11.md' in (repo/name).read_text(), name
print(json.dumps({'passed':True,'audio_playback_tests':576,'transport_tests':21,
                  'final_focused_tests':2,'initial_replay_checks':963,'final_sound_checks':61}))
