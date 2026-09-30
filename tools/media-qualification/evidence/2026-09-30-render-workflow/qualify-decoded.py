"""Read newly published workflow movies with independent complete-file readers."""
import json
import sys
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = Path('/Users/michael/Code/deadpan')
sys.path.insert(0, str(REPO/'tools/media-qualification/compatible'))
from qualify_project_encode import ProjectEncodeHarness

source = ROOT/'native-report.json'
report = json.loads(source.read_bytes())
assert report['status'] == 'passed'
work = ROOT/'decoded-workflow'
work.mkdir()
harness = ProjectEncodeHarness(work, False, Path('/tmp/deadpan-ui-ffmpeg-build.json'), REPO/'target/debug/deadpan-cli')
harness.report['scope'] = 'two complete coordinator movies compared against freshly captured shared-pipeline I420 and canonical PCM; independent FFmpeg and AVFoundation decoding'
try:
    harness.prepare()
    harness.admitted_files[str(source.resolve())] = harness.artifact(source)['sha256']
    for original in report['cases']:
        assert original['status'] == 'passed' and all(row['passed'] for row in original['checks'])
        case = {key:original[key] for key in ('name', 'contract', 'manifest', 'path')}
        for key in ('picture_reference', 'audio_reference'):
            case[key] = original['direct_inputs'][key]
        try:
            harness.capture_project(case)
        except Exception as error:
            harness.report['cases'][-1]['failure'] = str(error)
            harness.report['cases'][-1]['traceback'] = traceback.format_exc()
    harness.report['result'] = 'passed workflow publications' if len(harness.report['cases']) == 2 and all(c['passed'] for c in harness.report['cases']) else 'failed workflow publications'
except Exception as error:
    harness.report['failure'] = str(error)
    harness.report['traceback'] = traceback.format_exc()
finally:
    harness.finish_admission()
    if harness.process_faults:
        harness.report['result'] = 'failed: process or sanitizer fault'
    (work/'report.json').write_text(json.dumps(harness.report, indent=2)+'\n')
    print(json.dumps({'result':harness.report['result'], 'cases':[{'name':c['name'], 'passed':c['passed'], 'failure':c.get('failure')} for c in harness.report['cases']]}))
raise SystemExit(0 if harness.report['result'] == 'passed workflow publications' else 1)
