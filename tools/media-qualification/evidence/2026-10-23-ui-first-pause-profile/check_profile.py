"""Validate one fresh successful pause's buffered service profile."""
import json
from pathlib import Path
import re
import sys

prefix = 'DEADPAN_UI_PAUSE_PROFILE '
expected = [
    ('service_total', 'inclusive_total'),
    ('perform_macro_total', 'nested_inclusive_total'),
    ('context_validation', 'disjoint_child'),
    ('pre_edit_plan_scope', 'disjoint_child'),
    ('semantic_program_plan', 'disjoint_child'),
    ('transcript_guard', 'disjoint_child'),
    ('final_plan_scope_receipt', 'disjoint_child'),
    ('preview_runtime_bank', 'disjoint_child'),
    ('commit_compound', 'disjoint_child'),
    ('post_commit_receipt', 'disjoint_child'),
    ('service_refresh', 'disjoint_child'),
    ('post_refresh_receipt', 'disjoint_child'),
    ('post_dispatch_publication', 'disjoint_service_child'),
]
if len(sys.argv) != 2:
    raise SystemExit('usage: python check_profile.py captured-stderr.log')
lines = [line[line.index(prefix):] for line in Path(sys.argv[1]).read_text().splitlines()
         if prefix in line]
assert len(lines) == len(expected) + 2, f'expected 15 report lines, got {len(lines)}'
assert lines[0] == prefix + 'BEGIN count=1', 'missing or duplicated BEGIN'
assert lines[-1] == prefix + 'END count=1', 'missing or duplicated END'
pattern = re.compile(re.escape(prefix)
                     + r'PHASE phase=([a-z_]+) kind=([a-z_]+) elapsed_ns=(\d+) count=(\d+)')
values = {}
for line, (phase, kind) in zip(lines[1:-1], expected):
    match = pattern.fullmatch(line)
    assert match, f'invalid report line: {line}'
    actual_phase, actual_kind, elapsed, count = match.groups()
    assert (actual_phase, actual_kind) == (phase, kind), f'phase order/multiplicity: {line}'
    assert int(count) == 1, f'fresh successful pause requires one execution: {line}'
    values[phase] = int(elapsed)
perform_children = sum(values[name] for name, _ in expected[2:12])
perform_residual = values['perform_macro_total'] - perform_children
service_residual = (values['service_total'] - values['perform_macro_total']
                    - values['post_dispatch_publication'])
assert perform_residual >= 0, 'disjoint perform phases exceed inclusive perform total'
assert service_residual >= 0, 'perform + publication exceed inclusive service total'
print(json.dumps({
    'matched_reports': 1,
    'all_phase_counts': 1,
    'nanoseconds': values,
    'perform_uninstrumented_and_timer_overhead_ns': perform_residual,
    'service_other_dispatch_and_timer_overhead_ns': service_residual,
}, indent=2))
