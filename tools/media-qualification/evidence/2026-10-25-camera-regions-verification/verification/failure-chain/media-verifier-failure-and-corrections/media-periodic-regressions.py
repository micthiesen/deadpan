"""Pure assertions over retained JSON; no CLI/decoder/verifier invocation."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path

scratch = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('regions_media_assertions', scratch / 'media.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)  # __main__ stays false; no driver/product execution.
report = json.loads((scratch / 'media-final/verification.json').read_text())
evidence = module.validate_audio_evidence(report, 192192)
assert len(evidence['identifiable_zero_segment_starts']) == 1
assert len(evidence['periodic_segments_unobservable_individually']) == 7
cases = []


def refuses(name, mutate):
    changed = copy.deepcopy(report)
    mutate(changed)
    try:
        module.validate_audio_evidence(changed, 192192)
    except AssertionError:
        cases.append({'case': name, 'rejected': True})
    else:
        raise AssertionError(name)


refuses('aggregate offset cannot be compensated',
        lambda v: v['audio'][0].update(offset_status='offset', measured_offset_samples=1024))
refuses('uncovered output fails', lambda v: v['audio'][0].update(uncovered_samples=1))
refuses('partial PCM window fails', lambda v: v['audio'][0].update(window=[0, 192000]))
refuses('content SNR failure fails', lambda v: v['audio'][0]['blocks'].update(low_snr_blocks=1))
refuses('level difference failure fails', lambda v: v['audio'][0]['blocks'].update(level_blocks=1))
refuses('sound in silence fails', lambda v: v['audio'][0]['blocks'].update(sound_in_silence_blocks=1))
refuses('missing block fails', lambda v: v['audio'][0]['blocks'].update(compared=400))
refuses('weakened thresholds fail', lambda v: v['thresholds'].update(min_block_snr_db=0))
refuses('nonzero identifiable segment fails',
        lambda v: v['audio'][0]['segments'][1].update(peak_lag=1))
refuses('offset segment is never periodic evidence',
        lambda v: v['audio'][0]['segments'][0].update(status='offset'))
refuses('uncorrelated segment fails',
        lambda v: v['audio'][0]['segments'][0].update(status='uncorrelated'))
refuses('periodic peak without zero-lag equivalence fails',
        lambda v: v['audio'][0]['segments'][0].update(zero_lag_correlation=0.5))
refuses('periodic lobe without separated maximum fails',
        lambda v: v['audio'][0]['segments'][0].update(secondary_lag=1))
refuses('periodic report cannot replace identifiable witness',
        lambda v: v['audio'][0].update(
            segments=[row for row in v['audio'][0]['segments'] if row['status'] == 'periodic']))
record = {'scope': 'pure scratch assertion regressions on retained JSON; no product invocation',
          'driver_sha256': hashlib.sha256((scratch / 'media.py').read_bytes()).hexdigest(),
          'verification_sha256': hashlib.sha256((scratch / 'media-final/verification.json').read_bytes()).hexdigest(),
          'actual_retained_report_assertions_pass': True,
          'actual_audio_evidence': evidence, 'negative_mutations': cases}
(scratch / 'media-periodic-regressions.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'retained_report': 'assertions pass', 'negative_mutations_rejected': len(cases)}))
