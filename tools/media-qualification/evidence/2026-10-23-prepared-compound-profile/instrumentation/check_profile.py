"""Shape/containment check for one future successful native pause Macro trial."""
import json
from pathlib import Path
import re
import sys

PREFIX = 'DEADPAN_PREPARED_COMPOUND_PROFILE '
EXPECTED = [
    ('app_service_total', 'inclusive_total'),
    ('app_perform_macro_total', 'nested_inclusive_total'),
    ('app_semantic_planning', 'disjoint_app_child'),
    ('app_prepared_commit_total', 'nested_inclusive_app_child'),
    ('app_service_refresh', 'disjoint_app_child'),
    ('app_publication', 'disjoint_service_child'),
    ('store_prepared_commit_total', 'nested_inclusive_store_total'),
    ('store_admission_immediate_guard', 'disjoint_store_child'),
    ('store_prepare_command', 'disjoint_store_child'),
    ('store_host_callback', 'disjoint_store_child'),
    ('store_write_plan_or_bank', 'disjoint_store_child'),
    ('store_durable_sql_commit', 'disjoint_store_child'),
    ('store_cache_publication', 'disjoint_store_child'),
    ('app_refresh_succeeded', 'success_marker'),
]


def require(condition, message):
    if not condition:
        raise SystemExit(message)


require(len(sys.argv) == 2, 'usage: python check_profile.py captured-stderr.log')
lines = [line[line.index(PREFIX):]
         for line in Path(sys.argv[1]).read_text().splitlines() if PREFIX in line]
require(len(lines) == len(EXPECTED) + 2, 'Expected exactly one 16-line profile block')
require(lines[0] == PREFIX + 'BEGIN count=1', 'Missing or duplicated BEGIN')
require(lines[-1] == PREFIX + 'END count=1', 'Missing or duplicated END')
pattern = re.compile(re.escape(PREFIX)
                     + r'PHASE phase=([a-z_]+) kind=([a-z_]+) elapsed_ns=(\d+) count=(\d+)')
values = {}
for line, (phase, kind) in zip(lines[1:-1], EXPECTED):
    match = pattern.fullmatch(line)
    require(match is not None, 'Malformed profile phase')
    actual_phase, actual_kind, elapsed, count = match.groups()
    require((actual_phase, actual_kind) == (phase, kind), 'Wrong phase order or multiplicity')
    require(int(count) == 1, 'Expected one execution of every phase and one refresh success')
    values[phase] = int(elapsed)
require(values['app_refresh_succeeded'] == 0, 'Success marker must have zero timing')
store_children = sum(values[phase] for phase, _ in EXPECTED[7:13])
residuals = {
    'store_other_and_timer_overhead': values['store_prepared_commit_total'] - store_children,
    'app_prepared_commit_wrapper_and_timer_overhead':
        values['app_prepared_commit_total'] - values['store_prepared_commit_total'],
    'app_other_macro_work_and_timer_overhead':
        values['app_perform_macro_total'] - values['app_semantic_planning']
        - values['app_prepared_commit_total'] - values['app_service_refresh'],
    'app_other_dispatch_and_timer_overhead':
        values['app_service_total'] - values['app_perform_macro_total']
        - values['app_publication'],
}
require(all(value >= 0 for value in residuals.values()),
        'Nested/disjoint timings violate containment')
print(json.dumps({'matched_macro_reports': 1, 'store_commit_count': 1,
                  'all_phase_counts': 1, 'nanoseconds': values,
                  'residual_nanoseconds': residuals}, indent=2))
