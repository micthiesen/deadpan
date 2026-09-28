"""Re-observe video metadata without re-encoding or replacing failed evidence."""
import copy
import json
from pathlib import Path
import sys

root = Path('/Users/michael/Code/deadpan/tools/media-qualification/compatible')
sys.path.insert(0, str(root))
from qualify_encoder import EncoderHarness, evaluate_case, native, source_inventory

scratch = Path(__file__).parent
original_path = scratch / 'matrix.json'
original = json.loads(original_path.read_text())
work = scratch / 'video-followup'
work.mkdir(exist_ok=False)
harness = EncoderHarness(work, False, Path('/tmp/deadpan-ui-ffmpeg-build.json'))
harness.report['original_observations'] = {'path': str(original_path), 'sha256': native.digest(original_path),
    'scope': 'Reuse original encoded bytes, packet/audio/box observations; only video metadata is re-decoded. Reevaluate with corrected oracle.'}
try:
    harness.prepare()
    assert harness.report['native'] == original['native']
    for old in original['cases']:
        case = copy.deepcopy(old)
        harness.report['cases'].append(case)
        for artifact in case['artifacts'].values():
            assert native.digest(Path(artifact['path'])) == artifact['sha256']
        if 'video' not in case:
            continue
        for audio in case['audio'].values():
            assert native.digest(Path(audio['pcm']['path'])) == audio['pcm']['sha256']
        path = case['artifacts']['mp4']['path']
        case['video'] = json.loads(harness.run([harness.binary, 'video', path]).stdout)
        case['status'] = 'captured, acceptance not evaluated'
        evaluate_case(case)
    harness.report['result'] = 'video re-observation complete; export remains unqualified'
finally:
    harness.finish_admission()
    harness.report['source_sha256'] = source_inventory()
    harness.report['source_unchanged_during_run'] = harness.report['source_sha256'] == harness.report['source_sha256_at_start']
    if not harness.report['source_unchanged_during_run'] or harness.process_faults:
        harness.report['result'] = 'failed source or process qualification'
    (scratch / 'video-followup.json').write_text(json.dumps(harness.report, indent=2) + '\n')
    print(json.dumps({'result': harness.report['result'], 'cases': [
        {'name': case['name'], 'status': case['status'], 'required_failures': [check['label'] for check in case['checks']
         if not check['passed'] and not check.get('diagnostic',False)]} for case in harness.report['cases']]}))
if harness.report['result'] != 'video re-observation complete; export remains unqualified':
    raise SystemExit(1)
