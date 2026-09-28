"""One old encoder dispatch path after the additive plane commands."""
import json
from pathlib import Path
import sys

root = Path('/Users/michael/Code/deadpan/tools/media-qualification/compatible')
sys.path.insert(0, str(root))
from qualify_encoder import EncoderHarness, evaluate_case, source_inventory

scratch = Path(__file__).resolve().parent
work = scratch/'encoder-control'
work.mkdir(exist_ok=False)
harness = EncoderHarness(work, False, Path('/tmp/deadpan-ui-ffmpeg-build.json'))
try:
    harness.prepare()
    case = harness.capture_case('existing-default-60', 'hardware-no-b', 'default', fps=(60, 1))
    evaluate_case(case)
    harness.report['experiment_completed'] = True
    harness.report['result'] = 'passed existing encoder dispatch control' if case.get('scoped_checks_passed') else 'failed existing encoder control'
finally:
    harness.finish_admission()
    harness.report['source_sha256'] = source_inventory()
    harness.report['source_unchanged_during_run'] = harness.report['source_sha256'] == harness.report['source_sha256_at_start']
    if not harness.report['source_unchanged_during_run'] or harness.process_faults:
        harness.report['result'] = 'failed source or process qualification'
    (scratch/'encoder-control.json').write_text(json.dumps(harness.report, indent=2)+'\n')
    print(json.dumps({'result': harness.report['result'], 'checks': sum(len(case.get('checks', [])) for case in harness.report['cases'])}))
sys.exit(0 if harness.report['result'] == 'passed existing encoder dispatch control' else 1)
