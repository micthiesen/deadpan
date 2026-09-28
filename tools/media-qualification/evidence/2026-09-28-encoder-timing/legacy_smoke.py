"""Run the unchanged compatible fixture contract after shared C helper hooks."""
import json
from pathlib import Path
import shlex
import sys

root = Path('/Users/michael/Code/deadpan/tools/media-qualification/compatible')
sys.path.insert(0, str(root))
from qualify_encoder import EncoderHarness, native, source_inventory

work = Path(__file__).parent / 'legacy'
work.mkdir(exist_ok=False)
harness = EncoderHarness(work, False, Path('/tmp/deadpan-ui-ffmpeg-build.json'))
try:
    flags = shlex.split(harness.run(['pkg-config', '--cflags', '--libs', 'libavformat', 'libavcodec', 'libavutil']).stdout)
    binary = work / 'media_probe'
    harness.run(['clang', '-std=c11', '-O2', '-g', '-Wall', '-Wextra', '-Werror',
                 root / 'media_probe.c', '-o', binary, *flags, '-arch', 'arm64', '-mmacosx-version-min=15.0'])
    harness.report['binary_sha256'] = native.digest(binary)
    harness.report['inventory'] = json.loads(harness.run([binary, 'inventory']).stdout)
    source = json.loads(harness.run([binary, 'encode', work / 'legacy-cfr.mp4', 'h264_videotoolbox', 'cfr', 'hardware-no-b']).stdout)
    native.Harness.inspect(harness, 'legacy-cfr', source)
    harness.report['result'] = 'passed original compatible CFR fixture checks'
finally:
    harness.report['source_sha256'] = source_inventory()
    harness.report['source_unchanged_during_run'] = harness.report['source_sha256'] == harness.report['source_sha256_at_start']
    if not harness.report['source_unchanged_during_run'] or harness.process_faults:
        harness.report['result'] = 'failed source or process qualification'
    (work / 'report.json').write_text(json.dumps(harness.report, indent=2) + '\n')
    print(json.dumps({'result': harness.report['result'], 'assertions': len(harness.report['assertions'])}))
if harness.report['result'] != 'passed original compatible CFR fixture checks':
    raise SystemExit(1)
