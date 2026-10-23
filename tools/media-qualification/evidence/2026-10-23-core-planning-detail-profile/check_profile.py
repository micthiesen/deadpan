"""Strict shape/containment parser; parsing alone never qualifies a workload."""
from pathlib import Path
import json
import re
import sys

SCHEMA = json.loads((Path(__file__).parent / 'profile_schema.json').read_text())
PREFIX = SCHEMA['prefix']
EXPECTED = SCHEMA['phases']

def require(condition, message):
    if not condition:
        raise ValueError(message)

def parse_profile(text):
    lines = [line for line in text.splitlines() if PREFIX in line]
    require(len(lines) == SCHEMA['expected_profile_lines'], 'Expected exactly one 26-line profile block')
    require(lines[0] == PREFIX + ' BEGIN count=1', 'Malformed or duplicated BEGIN')
    require(lines[-1] == PREFIX + ' END count=1', 'Malformed or duplicated END')
    pattern = re.compile(re.escape(PREFIX) + r' PHASE phase=([a-z_]+) kind=([a-z_]+) elapsed_ns=(\d+) count=(\d+)')
    values = {}
    counts = {}
    for line, expected in zip(lines[1:-1], EXPECTED):
        match = pattern.fullmatch(line)
        require(match is not None, 'Malformed phase line')
        phase, kind, elapsed, count = match.groups()
        require((phase, kind) == (expected['name'], expected['kind']), 'Unknown, duplicated or out-of-order phase')
        require(int(count) == expected['expected_count'], 'Wrong phase call count: ' + phase)
        elapsed = int(elapsed)
        if expected['expected_elapsed_ns'] is not None:
            require(elapsed == expected['expected_elapsed_ns'], 'Nonzero marker/debug timing: ' + phase)
        values[phase] = elapsed
        counts[phase] = int(count)
    residuals = {}
    for equation in SCHEMA['containment']:
        parent = '+'.join(equation['sum_parent_spans'])
        residual = sum(values[name] for name in equation['sum_parent_spans']) - sum(values[child] for child in equation['sum_disjoint_child_spans'])
        require(residual >= 0, 'Negative containment residual: ' + parent)
        residuals[parent + '_minus_disjoint_children_and_overhead'] = residual
    return {
        'matched_macro_reports': 1,
        'profile_lines': len(lines),
        'phase_counts': counts,
        'nanoseconds': values,
        'residual_nanoseconds': residuals,
        'parser_only_not_workload_or_target_pass': True,
        'sum_of_calls_and_residuals_are_not_individual_invocations': True,
    }

if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: python check_profile.py captured-stderr.log')
    try:
        result = parse_profile(Path(sys.argv[1]).read_text())
    except ValueError as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
    print(json.dumps(result, indent=2))
