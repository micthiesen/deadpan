"""Bounded evidence collector. Root alone may run this after final checks.

Invoke with --confirmed-final only after writing runtime-results.json:
{
  "passed": true, "verified_by": "root", "source_identities": "final-source-identities.json",
  "runs": {
    "focused-plan": {"kind":"nextest","log":"focused-plan.log","passed":4,
      "exit_code":0,"command":"ACTUAL COMMAND","binaries":["final-plan"]},
    "focused-pcm": {"kind":"nextest","log":"focused-pcm.log","passed":8,
      "exit_code":0,"command":"ACTUAL COMMAND","binaries":["final-pcm"]},
    "broad": {"kind":"nextest","covers":["broad-plan","broad-audio","broad-playback"],
      "log":"broad.log","passed":ACTUAL,"exit_code":0,
      "command":"ACTUAL COMMAND","binaries":["EACH EXECUTED BINARY"]},
    "encoded": {"kind":"nextest","log":"encoded.log","passed":1,
      "exit_code":0,"command":"ACTUAL COMMAND","binaries":["encoded-cli","encoded-test"]},
    "lint": {"kind":"command","log":"lint.log","exit_code":0,
      "command":"ACTUAL COMMAND","binaries":[]}
  },
  "binaries": {"NAME":{"path":"PRIVATE RETAINED EXECUTABLE","sha256":"ACTUAL HASH"}}
}
Separate broad runs may each cover their own named role instead. Every nextest
count is checked against its actual final summary. Source identities are a map
of repository-relative paths to final SHA-256 values. A replaced executable
must use its retained private copy. No raw media, package, or executable enters
the evidence directory. This script has not run merely because it exists.
"""
from pathlib import Path
import gzip
import hashlib
import json
import re
import sys

REPO = Path('/Users/michael/Code/deadpan')
SCRATCH = Path('/tmp/deadpan-fractional-owner-support-2026-10-21')
OUTPUT = REPO / 'tools/media-qualification/evidence/2026-10-21-fractional-owner-support'
BASE = '25646268'
BASELINES = {
    'original': ('old-pcm-test', 'a660e05818178706644d3413387a934e13209ad6486367965f469353ca1ee8c2',
                 'pcm/baseline-output-sound.rs', 'c390fdaaf50962dea7966634a884176b2d9fff2af1f71ed9d08af44bdb134d6e'),
    'child-gain': ('old-child-gain-test', '2df57c18ac8177b58b25b40014f9d6cf29dc337a5cdf2307bd07af40efd649e5',
                   'pcm/baseline-negative-clock-output-sound.rs', '68c3b8ab2d39d4a283927234885a479425722ddaa25de13824831ba5840b4b3d'),
    'plan': ('old-plan-test', '7624f5a97fead15f6e02282855245ccd64dc9bd82dd3b9fe68f703f031a4c2b5',
             'old-plan-tests.rs', 'f51e39f4422e0461d92ccdd8847875f8a777205f44cb2c525f2efcab9db78f66'),
}
MEDIA = {
    'video': ('native/deadpan-source/tests/fixtures/cfr-bframes.mp4',
              '5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918'),
    'event': ('native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav',
              'b95b3debd2c83cf2590fddfaa98276c23c3b383b720fd96ac52c2c00c668318b'),
}


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(65536), b''):
            digest.update(block)
    return digest.hexdigest()


def read(name):
    return json.loads((SCRATCH / name).read_text())


def final_runtime():
    runtime = read('runtime-results.json')
    assert runtime['passed'] is True and runtime['verified_by'] == 'root'
    roles = set()
    referenced = set()
    for name, run in runtime['runs'].items():
        roles.update(run.get('covers', [name]))
        assert run['exit_code'] == 0 and run['command'].strip(), name
        log = (SCRATCH / run['log']).read_text()
        if run['kind'] == 'nextest':
            assert isinstance(run['passed'], int) and run['passed'] > 0, name
            summary = next(line for line in reversed(log.splitlines()) if 'Summary [' in line)
            counts = re.search(r'\b(\d+) tests? run: (\d+) passed\b', summary)
            assert counts and int(counts[1]) == int(counts[2]) == run['passed'], (name, summary)
            assert ' failed' not in summary, (name, summary)
            assert run['binaries'], (name, 'executed identities required')
        else:
            assert run['kind'] == 'command', name
        referenced.update(run['binaries'])
    required = {'focused-plan', 'focused-pcm', 'broad-plan', 'broad-audio', 'broad-playback', 'encoded', 'lint'}
    assert required <= roles, ('unfinished required checks', required - roles)
    assert {'encoded-cli', 'encoded-test'} <= referenced
    for name in referenced:
        identity = runtime['binaries'][name]
        assert re.fullmatch('[a-f0-9]{64}', identity['sha256']), name
        assert sha(identity['path']) == identity['sha256'], (name, 'executed binary changed; use retained copy')
    sources = read(runtime['source_identities'])
    assert sources
    for path, expected in sources.items():
        assert sha(REPO / path) == expected, ('final source changed', path)
    return runtime, sources


def validate_baselines():
    for name, (binary, binary_hash, source, source_hash) in BASELINES.items():
        assert sha(SCRATCH / binary) == binary_hash, name
        assert sha(SCRATCH / source) == source_hash, name
    first = read('baseline-identities.json')
    assert first['base_revision'].startswith(BASE)
    assert (first['tests'], first['failed'], first['exit_code']) == (1, 1, 100)
    assert 'nonzero Original outside meaningful owner-clock support' in (SCRATCH / 'baseline-pcm.log').read_text()
    second = read('baseline-child-gain-identities.json')
    assert (second['tests'], second['failed'], second['exit_code']) == (1, 1, 100)
    assert 'nonzero Original outside owner support' in (SCRATCH / 'baseline-child-gain.log').read_text()
    plan = read('baseline-plan-identities.json')
    assert (plan['tests'], plan['positive_failed'], plan['negative_passed'], plan['fixture_setup_failed']) == (4, 2, 1, 1)
    assert plan['earlier_compile_exit'] == 101
    assert 'error[E0034]' in (SCRATCH / 'baseline-plan-compile.log').read_text()
    assert '4 tests run: 1 passed, 3 failed' in (SCRATCH / 'baseline-plan.log').read_text()
    assert 'source before has no media; use a Hold for blank time' in (SCRATCH / 'baseline-plan.log').read_text()
    assert '4 tests run: 3 passed, 1 failed' in (SCRATCH / 'focused-plan-first.log').read_text()
    first_plan = read('first-plan-identities.json')
    assert sha(SCRATCH / 'first-plan-test') == first_plan['sha256']
    assert (first_plan['tests'], first_plan['passed'], first_plan['fixture_setup_failed']) == (4, 3, 1)
    reviews = read('reviews.json')
    assert reviews['base'].startswith(BASE) and len(reviews['reviews']) >= 3
    assert all(review['findings'] == [] and review['runtime_by_reviewer'] is False
               for review in reviews['reviews']), 'retain actual resolved review outcomes'


def encoded_evidence():
    encoded = read('encoded-result.json')
    report = encoded['report']
    assert report['passed'] is True and report['failures'] == []
    summary, movie_report = report['summary'], report['movie']
    assert summary['pictures_checked'] == 11 and summary['pictures_failed'] == 0
    assert movie_report['video_frames'] == movie_report['expected_video_frames'] == 11
    assert movie_report['expected_audio_samples'] == movie_report['audio']['presented_end'] == 17618
    assert movie_report['audio_edits']['presented_media_ticks'] == 17618
    assert movie_report['picture_anomalies'] == [] and movie_report['audio']['gaps'] == []
    assert summary['nonzero_offsets'] == 0
    assert summary['signal_windows'] == summary['audio_windows_checked'] == 2
    expected = [None, None, None, None, 13, None, None, 13, 12, 13, 14]
    assert len(report['pictures']) == len(expected)
    for at, (picture, ordinal) in enumerate(zip(report['pictures'], expected)):
        assert picture['output_frame'] == picture['project_frame'] == at
        assert picture['passed'] is True and picture['flags'] == []
        provenance = picture['provenance']
        if ordinal is None:
            assert provenance['kind'] == 'background'
        else:
            assert provenance['kind'] == 'original'
            assert provenance['source_frame'] == ordinal and provenance['source_pts'] == ordinal * 1001
    assert len(report['audio']) == 2
    for window, bounds in zip(report['audio'], [[0, 8809], [8809, 17618]]):
        assert window['window'] == window['project_samples'] == bounds
        assert window['passed'] is True and window['flags'] == [] and window['kind'] == 'signal'
        assert window['offset_status'] == 'verified_zero' and window['measured_offset_samples'] == 0
        assert window['blocks']['low_snr_blocks'] == 0
    oracle = Path(encoded['oracle'])
    data = json.loads(oracle.read_text())
    assert data['event_native'] == [3100, 8005] and data['event_support'] == [6443, 11782]
    assert data['retained_event_phase'] == '2480147/800' and data['event_step'] == '147/160'
    assert data['event_trim_db'] == -18 and data['physical_change_maximum'] > 0.0001
    for treatment in [data['before'], data['after']]:
        assert treatment['samples'] == 17618 and treatment['suppressed_samples'] == [12812]
        assert treatment['max_master_error'] < 0.000003 and treatment['master_peak'] < 0.9
        assert treatment['wrong_separate_voice_max_difference'] > 0.0001
        assert treatment['omitted_processor_max_difference'] > 0.001 and treatment['omitted_root_max_difference'] > 0.001
        assert any(abs(value) > 0.0001 for value in treatment['raw_seam8236'])
        assert any(abs(value) > 0.0001 for value in treatment['master_seam8236'])
    producer = Path(encoded['producer'])
    production = json.loads(producer.read_text())
    original = Path(production['original'])
    wav = original.with_name('nonzero-original.wav')
    assert sha(original) == production['original_sha256']
    assert sha(wav) == production['wav_sha256']
    assert sha(Path(encoded['package']).parent / 'original.mp4') == production['original_sha256']
    assert production['video_source_sha256'] == MEDIA['video'][1]
    assert data['event_source_sha256'] == MEDIA['event'][1]
    return encoded, oracle, producer, original, wav


def main():
    assert sys.argv[1:] == ['--confirmed-final'], 'Root authorization and finished runtime-results.json required'
    runtime, sources = final_runtime()
    validate_baselines()
    for name, (path, expected) in MEDIA.items():
        assert sha(REPO / path) == expected, ('measured source', name)
    encoded, oracle, producer, original, wav = encoded_evidence()
    assert not OUTPUT.exists() or not any(OUTPUT.iterdir()), 'new milestone directory must be empty'
    OUTPUT.mkdir(parents=True, exist_ok=True)

    def keep(source, name, compress=False):
        content = Path(source).read_bytes()
        target = OUTPUT / (name + '.gz' if compress else name)
        assert not target.exists(), ('duplicate evidence name', name)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(gzip.compress(content, mtime=0) if compress else content)

    extensions = {'.log', '.json', '.sh', '.txt', '.diff', '.exit', '.py', '.sha256', '.patch', '.md', '.rs'}
    for path in sorted(SCRATCH.rglob('*')):
        if any(parent.suffix == '.deadpan' for parent in path.parents):
            continue
        if path.is_file() and path.suffix in extensions:
            compress = path.suffix in {'.log', '.rs', '.diff', '.patch'} or path.stat().st_size > 65536
            compress |= path.suffix == '.json' and any(word in path.stem for word in ['report', 'result'])
            keep(path, str(path.relative_to(SCRATCH)), compress)
    for path in sources:
        if path.endswith('.rs'):
            keep(REPO / path, 'final-source/' + path, True)
    keep(oracle, 'whole-master-oracle.json', True)
    keep(producer, 'producer.json')
    movie = Path(encoded['movie'])
    report = encoded['report']
    provenance = {
        'scope': 'fractional retained processed-owner support; one planner predicate and comment; no DSP/schema/UI change or native interaction claim',
        'source_base': BASE,
        'generated_original': {'path': str(original), 'sha256': sha(original), 'bytes': original.stat().st_size},
        'generated_pcm': {'path': str(wav), 'sha256': sha(wav), 'bytes': wav.stat().st_size},
        'repository_media': {name: {'path': path, 'sha256': expected, 'bytes': (REPO / path).stat().st_size} for name, (path, expected) in MEDIA.items()},
        'movie': {'path': str(movie), 'sha256': sha(movie), 'bytes': movie.stat().st_size},
        'project': {'path': encoded['package'], 'project_id': report['project_id'], 'revision_id': report['revision_id'], 'document_sha256': report['document_sha256']},
        'retention': 'Reports, failures, source snapshots, literal phase/master oracle and generator recipe retained. Raw media, project package and executables omitted.'
    }
    (OUTPUT / 'media-provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    summary = {'pictures': 11, 'presented_samples': 17618, 'encoded': report['summary'], 'runtime': runtime,
               'baseline_classification': {'pcm': 'two separate valid success regressions, each failed old code', 'plan': 'two valid positive failures, one negative pass, one invalid factory setup failure; earlier compile101; first implementation three pass plus same setup failure'}}
    (OUTPUT / 'verification-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    inventory = {str(path.relative_to(OUTPUT)): {'bytes': path.stat().st_size, 'sha256': sha(path)} for path in sorted(OUTPUT.rglob('*')) if path.is_file()}
    total = sum(item['bytes'] for item in inventory.values())
    assert total < 8 * 1024 * 1024, ('unexpectedly large evidence', total)
    (OUTPUT / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
    for name, item in inventory.items():
        assert (OUTPUT / name).stat().st_size == item['bytes'] and sha(OUTPUT / name) == item['sha256'], name
    print(f'{len(inventory)} evidence entries, {total} bytes; every entry rehashed')


if __name__ == '__main__':
    main()
