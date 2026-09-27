import hashlib
import json
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-audio-projection-20260926')
paths = json.loads((scratch / 'increment-paths.json').read_text())
record = {
    'scope': 'Source increment against before/ snapshot; earlier concurrent changes preserved',
    'reviewer': '/root/interior_migration',
    'independent_of_implementation': True,
    'final_code_findings': [],
    'resolved_findings': [
        'Charge each binary-search comparison before dispatch, under shared query work.',
        'Do not subtract silence-mask count from structural policy span allowance.',
        'Keep mapped physical allocation distinct from global meaningful input support and prior genuine constraints.',
        'Use explicit SignalSample grid type and exact checked ratio comparisons.',
        'Fix test helper lifetime and shadowed fixture function.',
        'Use regression-sensitive query/policy search budgets and aggregate query span rejection.',
        'Add actual Bound seam PCM and transparent Partition sampling-support coverage.',
        'Correct live raw mapping fixture to natural-rate Placement and independently clipped source taps.',
    ],
    'contract_clarification': 'Global tape support is a meaningful intrinsic input selection. Per-run windows are allocation only. A proposed objection to global crop was withdrawn after this distinction was made explicit.',
    'reviewed_behaviors': [
        'Existing non-tape signal constructors preserve prior sampling support.',
        'Definition, repeat, bypass and intrinsic stage identities survive remapping.',
        'Bound physical allocation anchors remain separate from run request slices.',
        'Source admission, cancellation, work, preparation limits and cache dependencies remain shared.',
    ],
    'test_changes_after_review': 'Adopted reviewer refinement from policy work budget 3 to 4; no production changes.',
    'reviewer_ran_cargo': False,
    'source_sha256': {p: hashlib.sha256((repo / p).read_bytes()).hexdigest() for p in paths},
}
(scratch / 'review.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'reviewed_paths': len(paths), 'final_code_findings': []}))
