"""Verify retained identities and result joins without extraction or new DSP."""
import ast
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import statistics
import struct
import tarfile

ROOT = Path(__file__).resolve().parent
CEILING = 10 ** (-1 / 20)
NATIVE = 'native-audit/provenance-1790244980014579000'
PINNED_NATIVE = 'native-audit-rust1971/provenance-1790247077077703000'
CASES = {'ordinary', 'hot-s16', 'preserve-room'}
PROFILE = {'codegen-units': 1, 'lto': 'thin', 'strip': 'debuginfo', 'overflow-checks': True}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        assert key not in result, key
        result[key] = value
    return result


def load(data):
    def invalid(value):
        raise AssertionError('nonfinite JSON: ' + value)
    return json.loads(data, object_pairs_hook=unique_object, parse_constant=invalid)


def wave(data):
    assert len(data) >= 44
    fields = struct.unpack('<4sI4s4sIHHIIHH4sI', data[:44])
    kind, bits = fields[5], fields[10]
    assert (kind, bits) in {(1, 16), (3, 32)}
    width = bits // 8
    assert fields == (b'RIFF', len(data)-8, b'WAVE', b'fmt ', 16, kind,
                      2, 48000, 48000*2*width, 2*width, bits, b'data', len(data)-44)
    assert (len(data)-44) % (2*width) == 0
    if kind == 1:
        samples = [(a/32768, b/32768) for a, b in struct.iter_unpack('<hh', data[44:])]
    else:
        samples = list(struct.iter_unpack('<ff', data[44:]))
    assert all(math.isfinite(x) and abs(x) <= 16 for pair in samples for x in pair)
    return samples


def f32bytes(samples):
    return b''.join(struct.pack('<ff', *pair) for pair in samples)


def main():
    manifest = load((ROOT/'retention.json').read_bytes())
    archive_path = ROOT/'experiments.tar.gz'
    assert archive_path.stat().st_size == manifest['archive_bytes']
    assert sha(archive_path.read_bytes()) == manifest['archive_sha256']
    assert len(manifest['files']) < 5000
    assert sum(v['bytes'] for v in manifest['files'].values()) < 256*1024*1024
    files = {}
    with tarfile.open(archive_path, 'r:gz') as archive:
        for entry in archive:
            path = PurePosixPath(entry.name)
            assert entry.isfile() and not path.is_absolute() and '..' not in path.parts
            assert str(path) == entry.name and entry.name not in files
            assert not {'target', 'bin', '__pycache__'}.intersection(path.parts)
            assert not any(part.endswith('.dSYM') for part in path.parts)
            assert path.suffix not in {'.o', '.pyc'} and path.name not in {'deadpan-native-limiter-audit', 'finite-peak-abi-probe', 'finite_peak_abi_probe'}
            assert entry.mode == 0o644 and entry.mtime == entry.uid == entry.gid == 0
            expected = manifest['files'][entry.name]
            assert entry.size == expected['bytes']
            data = archive.extractfile(entry).read()
            assert sha(data) == expected['sha256'], entry.name
            if path.suffix == '.py':
                ast.parse(data, filename=entry.name)
            elif path.suffix == '.json':
                load(data)
            elif path.suffix == '.jsonl':
                for line in data.splitlines():
                    if line.strip(): load(line)
            elif path.suffix == '.wav' and path.parts[0] != 'final-source' and 'source' not in path.parts:
                wave(data)
            files[entry.name] = data
    assert set(files) == set(manifest['files'])
    assert sum(map(len, files.values())) == manifest['uncompressed_file_bytes']
    hashes = {sha(data) for data in files.values()}
    excluded_hashes = {record['sha256'] for record in manifest['excluded_file_identities'].values()}
    excluded_by_hash = {record['sha256']:record for record in manifest['excluded_file_identities'].values()}

    def report(path): return load(files[path])
    def lines(path): return [load(line) for line in files[path].splitlines() if line.strip()]
    def unique(rows, key):
        result = {key(row): row for row in rows}
        assert len(result) == len(rows)
        return result

    pilots = {}
    joined = 0
    for base, passes in [('candidate', 19), ('candidate/target17', 25), ('candidate/triggered17', 25)]:
        rows = report(base+'/results.json')['results']
        by_name = unique(rows, lambda row: row['name'])
        assert len(rows) == 25
        pilots[base] = by_name
        environment = report(base+'/environment.json')
        assert sha(files[base+'/pilot.py']) == environment['script_sha256']
        for key, value in environment.items():
            if key.endswith('_sha256'): assert value in hashes, (base, key)
        for name, digest in environment['coefficient_hashes'].items():
            assert sha(files[base+'/coefficients/'+name]) == digest
        assert sum(row['passes_finite_contract'] for row in rows) == passes
        for name, row in by_name.items():
            source = files[f'{base}/inputs/{name}.wav']
            output = files[f'{base}/outputs/{name}.wav']
            assert sha(source) == row['input_sha256'] and sha(output) == row['output_sha256']
            assert report(f'{base}/reports/{name}.json') == row
            before, after = wave(source), wave(output)
            gains = [x for x, in struct.iter_unpack('<d', files[f'{base}/outputs/{name}.gain.f64le'])]
            assert len(before) == len(after) == len(gains) == row['frames']
            assert all(math.isfinite(x) and 0 <= x <= 1 for x in gains)
            assert row['unity_gain'] == all(x == 1.0 for x in gains)
            rebuilt = b''.join(struct.pack('<ff', a*g, b*g) for (a,b),g in zip(before,gains))
            assert rebuilt == output[44:]
            assert row['bytes_unchanged'] == (source == output)
            assert row['active_frames_before'] == sum(any(x != 0 for x in pair) for pair in before)
            assert row['active_frames_after'] == sum(any(x != 0 for x in pair) for pair in after)
            assert all(struct.pack('<f', x) == struct.pack('<f', y)
                       for a,b in zip(before,after) for x,y in zip(a,b) if x == 0)
            joined += 1
        for name, digest in report(base+'/sha256.json').items():
            assert sha(files[base+'/'+name]) == digest, (base, name)

    triggered = pilots['candidate/triggered17']
    assert sum(row['unity_gain'] for row in triggered.values()) == 14
    assert set(triggered) == set(pilots['candidate']) == set(pilots['candidate/target17'])
    native_rows = unique(report(NATIVE+'/render/results.json'), lambda row: row['name'])
    audit = report(NATIVE+'/render/independent-audit.json')
    audit_rows = unique(audit['cases'], lambda row: row['name'])
    assert set(native_rows) == set(audit_rows) == set(triggered)
    parity_count = unchanged = 0
    for name in triggered:
        row, measured = audit_rows[name], native_rows[name]
        output = files[f'{NATIVE}/render/outputs/{name}.wav']
        gain = files[f'{NATIVE}/render/outputs/{name}.gain.f64le']
        source = files[f'candidate/triggered17/inputs/{name}.wav']
        assert output == files[f'candidate/triggered17/outputs/{name}.wav']
        assert gain == files[f'candidate/triggered17/outputs/{name}.gain.f64le']
        assert sha(output) == row['output_sha256'] and sha(gain) == row['gain_sha256']
        assert sha(source) == row['input_sha256']
        assert row['candidate_changed_frames'] == 0 and row['candidate_gain_bits_equal']
        assert row['bytes_unchanged'] == (source == output)
        if source == output:
            assert triggered[name]['unity_gain']
        unchanged += source == output
        numerator = int(row['exact_bs_numerator'])
        assert numerator >= 0 and row['exact_bs_denominator_exponent'] == 162
        assert 10*numerator**20 <= 1 << (162*20)
        assert row['exact_bs_peak'] == numerator/2**162 and row['exact_bs_pass']
        assert row['bh']['upper_with_f64_error'] <= CEILING and row['bh']['passes_guarded_ceiling']
        assert row['finite_and_preservation_pass'] and row['active_frames_lost'] == 0
        assert row['parity'] == measured['parity']
        for query in measured['parity']:
            assert query['f32_equal'] and query['gain_equal']
            assert 0 <= query['start'] < query['start']+query['frames'] <= row['frames']
            parity_count += 1
    assert unchanged == 14 and parity_count == 13
    native_env = report(NATIVE+'/environment.json')
    for name, digest in native_env['source_sha256'].items():
        assert sha(files[NATIVE+'/source/'+name]) == digest
    assert native_env['binary_sha256'] in excluded_hashes
    assert native_env['rustc'].startswith('rustc 1.98.0 ')
    assert excluded_by_hash[native_env['binary_sha256']]['embedded_rustc_paths'] == ['/rustc/88d9e12ae178fab0fb5cc050a94da85685d449ea']
    assert report(NATIVE+'/run-status.json')['exit'] == 0
    assert report(NATIVE+'/run-status.json')['sources_unchanged_after_run']
    assert report('native-audit/provenance-1790244826797032000/aborted.json')['outputs_produced'] is False
    assert sha(files['native-audit/audit.py']) == audit['script_sha256']
    sinc_failures = {name for name,row in triggered.items()
                     if any(ch['exceeds_minus_1_dbtp'] for ch in row['complete_sinc_diagnostic'])}
    assert sinc_failures == {'rapid-mask-0.8','rapid-mask-16.0','alternating-16.0',
                             'near-nyquist','retained-failure-16','long-rapid-mask-16'}

    pinned_env = report(PINNED_NATIVE+'/environment.json')
    assert pinned_env['rustc'].startswith('rustc 1.97.1 ') and pinned_env['cargo'].startswith('cargo 1.97.1 ')
    assert pinned_env['binary_compiler_paths'] == ['/rustc/8bab26f4f68e0e26f0bb7960be334d5b520ea452']
    assert pinned_env['binary_sha256'] in excluded_hashes
    for name,digest in pinned_env['source_sha256'].items():
        assert sha(files[PINNED_NATIVE+'/source/'+name]) == digest
    pinned_rows = unique(report(PINNED_NATIVE+'/render/results.json'),lambda row:row['name'])
    pinned_audit = report(PINNED_NATIVE+'/render/independent-audit.json')
    pinned_audit_rows = unique(pinned_audit['cases'],lambda row:row['name'])
    assert set(pinned_rows) == set(pinned_audit_rows) == set(triggered)
    pinned_parity = 0
    for name in triggered:
        row = pinned_audit_rows[name]
        for suffix in ['wav','gain.f64le']:
            data = files[f'{PINNED_NATIVE}/render/outputs/{name}.{suffix}']
            assert data == files[f'{NATIVE}/render/outputs/{name}.{suffix}']
            assert sha(data) == row['output_sha256' if suffix=='wav' else 'gain_sha256']
        assert row['exact_bs_numerator'] == audit_rows[name]['exact_bs_numerator']
        assert row['exact_bs_pass'] and row['bh']['passes_guarded_ceiling'] and row['finite_and_preservation_pass']
        assert row['bh']['upper_with_f64_error'] <= CEILING
        assert row['parity'] == pinned_rows[name]['parity']
        for query in row['parity']:
            assert query['f32_equal'] and query['gain_equal']
            pinned_parity += 1
    assert pinned_parity == 13
    assert report(PINNED_NATIVE+'/run-status.json')['exit'] == 0
    assert sha(files['native-audit-rust1971/audit.py']) == pinned_audit['script_sha256']

    key = lambda row: (row['case'], row['stage'], row.get('start'))
    old = unique(lines('pipeline-timing/results-s16-load.jsonl'), key)
    new = unique(lines('pipeline-timing-cached/results-s16-serial.jsonl'), key)
    assert set(old) == set(new) and {row['case'] for row in new.values()} == CASES
    expected_matches = {}
    for identity,row in new.items():
        for field,value in row.items():
            if field.endswith('_sha256'):
                assert old[identity][field] == value
                expected_matches[identity+(field,)] = value
    comparison = report('pipeline-timing-cached/baseline-comparison.json')
    actual_matches = unique(comparison['matches'], lambda row: (row['case'],row['stage'],row['start'],row['field']))
    assert len(expected_matches) == comparison['matched_fields'] == 135
    assert set(expected_matches) == set(actual_matches)
    for identity,digest in expected_matches.items():
        assert actual_matches[identity]['sha256'] == digest
    artifact_matches = unique(comparison['artifact_matches'], lambda row: row['path'])
    expected_artifacts = {f'{case}/{name}' for case in CASES for name in
                          ['document.json','index.json','input.wav','sequential.f32le','sequential-gain.f64le']}
    assert set(artifact_matches) == expected_artifacts and comparison['matched_artifact_files'] == 15
    for name,record in artifact_matches.items():
        before = files['pipeline-timing/run-s16-load/'+name]
        after = files['pipeline-timing-cached/run-s16-serial/'+name]
        assert before == after and sha(after) == record['sha256']

    measured_summaries = {}
    for base, run, rows, summary_file in [
            ('pipeline-timing','run-s16-load',old,'summary.json'),
            ('pipeline-timing-cached','run-s16-serial',new,'summary.json')]:
        metadata = report(base+'/metadata.json')
        assert metadata['root_release_profile'] == PROFILE
        for source,digest in metadata['files_sha256'].items():
            assert digest in hashes or digest in excluded_hashes, (base,source)
            if '/bin/' in source:
                assert excluded_by_hash[digest]['embedded_rustc_paths'] == ['/rustc/8bab26f4f68e0e26f0bb7960be334d5b520ea452']
        summaries = report(base+'/'+summary_file)
        assert set(summaries) == CASES
        for case in CASES:
            directory = f'{base}/{run}/{case}/'
            raw = files[directory+'sequential.f32le']
            gain = files[directory+'sequential-gain.f64le']
            assert len(raw) == len(gain) == 16*8192*8
            opened = rows[(case,'open_source',None)]
            assert sha(files[directory+'input.wav']) == opened['wav_sha256']
            assert sha(files[directory+'index.json']) == opened['index_sha256']
            assert sha(f32bytes(wave(files[directory+'input.wav']))) == opened['input_pcm_sha256']
            sequential = []
            for identity,row in rows.items():
                if row['case'] != case: continue
                if 'pcm_sha256' in row:
                    start,end = row['start']*8,(row['start']+row['frames'])*8
                    assert sha(raw[start:end]) == row['pcm_sha256']
                    assert sha(gain[start:end]) == row['gain_sha256']
                if row['stage'] == 'sequential': sequential.append(row)
                if base.endswith('-cached') and 'cached_bus_blocks' in row:
                    assert row['cached_bus_blocks'] <= row['bus_cache_capacity'] == 12
                    assert row['cached_tiles'] <= 4
            assert {row['start'] for row in sequential} == set(range(0,16*8192,8192))
            summary = rows[(case,'sequential_summary',None)]
            assert summary['cold_shuffled_bit_parity'] and summary['direct_bus_kernel_bit_parity'] and summary['warm_source_revalidation']
            full = [row for row in sequential if row['start'] >= 40960]
            later = [row for row in sequential if row['start'] >= 49152]
            values = [row['seconds']*1000 for row in full]
            expected = summaries[case]
            prefix = 'full_context' if base.endswith('-cached') else 'full_halo'
            assert expected[prefix+'_batches'] == len(full) == 11
            assert math.isclose(expected[prefix+'_mean_ms'],statistics.mean(values),rel_tol=1e-12)
            assert expected[prefix+'_max_ms'] == max(values)
            assert expected[prefix+'_over_nominal'] == sum(row['seconds'] > row['nominal_seconds'] for row in full)
            if base.endswith('-cached'):
                assert len(later) == expected['after_transition_batches'] == 10
                assert math.isclose(expected['after_transition_mean_ms'],statistics.mean(row['seconds']*1000 for row in later),rel_tol=1e-12)
                assert expected['after_transition_over_nominal'] == sum(row['seconds'] > row['nominal_seconds'] for row in later) == 0
                assert max(row['cached_bus_blocks'] for row in sequential) == expected['maximum_cached_bus_blocks'] == 12
                assert all(row['provider_calls'] in ({44,45} if case=='preserve-room' else {44}) for row in later)
                if case in {'ordinary', 'hot-s16'}:
                    assert all(old[key(row)]['provider_calls'] == 261 for row in later)
            measured_summaries[base+'/'+case] = expected
    refused = [row for row in lines('pipeline-timing/results-load.jsonl') if row['stage']=='case_failure']
    assert {row['case'] for row in refused} == {'ordinary','hot','preserve-room'}
    assert all('only signed16 PCM WAVE is admitted' in row['error'] for row in refused)
    assert measured_summaries['pipeline-timing/hot-s16']['full_halo_over_nominal'] == 11
    assert measured_summaries['pipeline-timing-cached/hot-s16']['full_context_over_nominal'] == 1

    assert files['sanitizer-final/status.txt'].decode().splitlines() == ['compile_exit=0','run_exit=0']
    assert b'finite convolution, guards and independent plans passed' in files['sanitizer-final/run.stdout']
    assert not files['sanitizer-final/run.stderr'].strip()
    for line in files['sanitizer-final/source-hashes.txt'].decode().splitlines():
        digest,path = line.split(None,1)
        assert sha(files['final-source/'+path]) == digest
    assert files['sanitizer-final/sinc-witness-audit/status.txt'].strip() == b'exit=1'
    assert b"KeyError: 'gain_knots'" in files['sanitizer-final/sinc-witness-audit/run.stderr']
    assert files['sanitizer-final/sinc-witness-audit/witness.status'].strip() == b'exit=0'
    witnesses = unique(lines('sanitizer-final/sinc-witness-audit/witness.stdout'),lambda row:row['name'])
    assert set(witnesses) == sinc_failures
    for name,row in witnesses.items():
        assert row['native_sha256'] == sha(files[f'{PINNED_NATIVE}/render/outputs/{name}.wav'])
        assert row['candidate_native_bytes_identical'] and row['above_minus_1_db']
        assert row['channel'] in {0, 1}
        diagnostic = triggered[name]['complete_sinc_diagnostic'][row['channel']]
        assert row['coordinate'] == diagnostic['refined_coordinate']
        assert row['coordinate_hex'] == row['coordinate'].hex()
        assert abs(float(row['value_80_digits'])) > CEILING

    for base in ['kernel-timing-root-profile']:
        for source,digest in report(base+'/metadata.json')['files_sha256'].items():
            assert digest in hashes, source
    final_cached = report('pipeline-timing-cached/metadata.json')['files_sha256']
    for path in ['crates/deadpan-audio/src/limited.rs','crates/deadpan-audio/src/limiter.rs',
                 'crates/deadpan-audio/src/limiter/coefficients.rs']:
        assert sha(files['final-source/'+path]) == final_cached['/Users/michael/Code/deadpan/'+path]
    print(json.dumps({'archive_files':len(files),'candidate_case_gain_joins':joined,
        'native_finite_audit_joins':50,'stored_native_cold_parity_assertions':parity_count+pinned_parity,
        'native_unchanged_controls':unchanged,'native_equals_triggered17':True,
        'retained_stronger_sinc_failures':sorted(sinc_failures),
        'cached_baseline_hash_fields':135,'cached_baseline_artifact_files':15,
        'pipeline_record_hashes_join_actual_pcm':True,'negative_evidence_verified':True,
        'compiler_correction_retained':True,'pinned_rust1971_replay_equals_rust1980':True,
        'sanitizer_probe_pass_recorded':True,'stronger_sinc_point_witness_hash_joins':6,
        'scope':'Retained artifact integrity, stored audit assertions and actual byte relationships; no DSP rerun, timing rerun or executable-binary revalidation'},indent=2))


if __name__ == '__main__':
    main()
