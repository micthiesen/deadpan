from pathlib import Path
import ast
import hashlib
import json

P = Path('/tmp/deadpan-compound-wire-reuse-production-2026-10-24')
C = Path('/tmp/deadpan-single-leaf-net-production-2026-10-24')
CONTROL = '970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2'
BASE = 'f973b941a3f71b70cd0826cf6ab3f5ceb68395e5407690bea79f8e5a49e53a53'
CANDIDATE = '2b0997f1ad22ce7dd869beba699909d22972efba434279cee350478dc63cb728'

def replace(text, old, new):
    assert text.count(old) == 1, (text.count(old), old)
    return text.replace(old, new)

def write(name, value):
    with (P / name).open('x') as out:
        out.write(value)

def json_text(value):
    return json.dumps(value, indent=2) + '\n'

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

expected = json.loads((C / 'expected-tests.json').read_text())
expected['core-unit-all']['passed'] = 729
expected['core-unit-all']['required_test_suffixes'] += [
    'one_leaf_replay_equal_provisional_value_cannot_bypass_wire_admission',
    'one_leaf_replay_consumes_stale_exact_edit_at_both_ordinary_boundaries',
    'one_leaf_replay_private_hit_is_exact_and_mode_misses_do_not_return_a_result',
    'one_leaf_replay_post_leaf_admission_failure_returns_no_capability',
    'compound_wire_reuse_matches_full_counts_and_actual_charges_for_header_lengths',
    'compound_wire_reuse_preserves_fallbacks_and_visitor_error_order',
    'compound_wire_reuse_equal_provisional_entry_still_requires_full_initial_count',
]
expected['core-integration']['passed'] = 35
expected['core-integration']['passed_per_binary_sorted'] = [3, 4, 13, 15]
# Require actual source-declared tests, avoiding stale historical names.
core_text = '\n'.join(path.read_text() for path in (P / 'source/crates/deadpan-core').rglob('*.rs'))
for name in expected['core-unit-all']['required_test_suffixes']:
    assert 'fn ' + name + '(' in core_text, name

protocol = {
    'orders': [['control', 'candidate'], ['candidate', 'control'],
               ['control', 'candidate'], ['candidate', 'control'], ['control', 'candidate']],
    'trials': 10, 'commands': 20, 'pause_checks': 70, 'kestrel_checks': 10, 'warnings': 20,
    'beats': 10000, 'profile_gate': None, 'profile_output_allowed': False,
    'control_sha256': CONTROL, 'control_source_map_sha256': BASE,
    'candidate_source_map_sha256': CANDIDATE,
    'fixture_identical_across_all_trials': True,
    'warning_message_prefixes': [
        '2 frame(s) settled their layout only on a second retry:',
        'egui discard warning: 1 run(s) of 3+ consecutive retrying frames, each frame with its own cause:',
    ],
    'memory_observer': False, 'memory_claim': False,
    'phase_accounting': 'no diagnostic hooks; production-route observation only',
}

common = """
CONTROL_SHA256 = '970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2'
BASE_MAP_SHA256 = 'f973b941a3f71b70cd0826cf6ab3f5ceb68395e5407690bea79f8e5a49e53a53'
CANDIDATE_MAP_SHA256 = '2b0997f1ad22ce7dd869beba699909d22972efba434279cee350478dc63cb728'
OWNED_SOURCES = {'crates/deadpan-core/src/compound.rs',
                 'crates/deadpan-core/src/one_leaf_replay.rs'}
PIN_REQUIRED = {'build.py', 'measure.py', 'expected-build-stages.json',
                'expected-tests.json', 'expected-measurement.json', 'baseline.json',
                'baseline-provenance.json', 'baseline-sources.json', 'baseline-inputs.json',
                'frozen-sources.json', 'source-review.json'}
"""
build = (C / 'build.py').read_text().replace(str(C), str(P))
build = replace(build, "SOURCE = S / 'source'\n", "SOURCE = S / 'source'\n" + common)
build = replace(build, "    assert actual == frozen, 'private candidate Rust sources changed'\n",
"""    assert actual == frozen, 'private candidate Rust sources changed'
    assert len(actual) == 2053 and sha(S / 'frozen-sources.json') == CANDIDATE_MAP_SHA256
""")
build = replace(build, "    assert set(actual) == set(base_sources), 'Rust source path set changed'\n",
"""    assert set(actual) == set(base_sources), 'Rust source path set changed'
    assert sha(S / 'baseline-sources.json') == BASE_MAP_SHA256
    assert {path for path in actual if actual[path] != base_sources[path]} == OWNED_SOURCES
    assert sum(actual[path] == base_sources[path] for path in actual) == 2051
""")
build = replace(build, "    inputs = json.loads((S / 'baseline-inputs.json').read_text())\n",
"""    inputs = json.loads((S / 'baseline-inputs.json').read_text())
    assert len(inputs) == 6
""")
build = replace(build, """    required = {'build.py', 'measure.py', 'expected-build-stages.json',
                'expected-tests.json'}
    assert required <= pins.keys(), 'build protocol/parser must be pinned before build'
""", """    assert PIN_REQUIRED <= pins.keys(), 'Root must review and pin the complete protocol before build'
""")
build = replace(build, "    assert baseline['control']['sha256'] == 'e6abcd3a521697f504901f156df445e41681cfc824ff97793f62f4e0865802e1'\n",
"    assert baseline['control']['sha256'] == CONTROL_SHA256\n")
build = replace(build, """            unit['executed'] = unit['retained']
""", """            assert row['tests']['passed_per_summary'] == [expected[name]['passed']] == [729]
            unit['executed'] = unit['retained']
""")
build = replace(build, """                assert len(row['tests']['passed_per_summary']) == 4
""", """                assert len(row['tests']['executed_tests']) == expected[name]['passed'] == 35
                assert sorted(row['tests']['passed_per_summary']) == expected[name]['passed_per_binary_sorted']
                assert len(row['tests']['passed_per_summary']) == 4
""")

measure = (C / 'measure.py').read_text().replace(str(C), str(P))
measure = replace(measure, "SOURCE = S / 'source'\n", "SOURCE = S / 'source'\n" + common)
measure = replace(measure, """    assert all(sha(S / path) == digest for path, digest in pins.items())
    control = read('baseline.json')['control']
""", """    assert PIN_REQUIRED <= pins.keys(), 'Root must review and pin the complete protocol before measurement'
    assert all(sha(S / path) == digest for path, digest in pins.items())
    protocol = read('expected-measurement.json')
    control = read('baseline.json')['control']
    assert control['sha256'] == CONTROL_SHA256 == protocol['control_sha256']
""")
measure = replace(measure, """    matched_sources = read('qualified-base-sources.json')
    matched_root = Path('/Users/michael/Code/deadpan')
""", """    matched_sources = read('baseline-sources.json')
    provenance = read('baseline-provenance.json')
    matched_root = Path(provenance['matched_source'])
    assert sha(S / 'baseline-sources.json') == BASE_MAP_SHA256 == protocol['control_source_map_sha256']
    assert sha(S / 'frozen-sources.json') == CANDIDATE_MAP_SHA256 == protocol['candidate_source_map_sha256']
    assert provenance['matched_source_map_sha256'] == BASE_MAP_SHA256
    assert provenance['matched_app'] == control['path']
    assert provenance['matched_app_sha256'] == CONTROL_SHA256
""")
measure = replace(measure, """    assert read('source-review.json')['source_map_sha256'] == sha(S / 'frozen-sources.json')
""", """    review = read('source-review.json')
    assert review['status'] == 'no_findings' and not review['findings']
    assert review['source_map_sha256'] == CANDIDATE_MAP_SHA256
""")
measure = replace(measure, """    commands, trials = [], []
""", """    commands, trials, first_fixture = [], [], None
""")
measure = replace(measure, """        assert {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')} == sources
        main_paths = subprocess.check_output(['rg', '--files', '-g', '*.rs'], cwd=matched_root, text=True).splitlines()
        assert {name: sha(matched_root / name) for name in main_paths} == matched_sources
""", """        actual = {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')}
        assert actual == sources and len(actual) == 2053
        assert {str(p.relative_to(matched_root)): sha(p) for p in matched_root.rglob('*.rs')} == matched_sources
        assert set(actual) == set(matched_sources)
        assert {path for path in actual if actual[path] != matched_sources[path]} == OWNED_SOURCES
        assert sum(actual[path] == matched_sources[path] for path in actual) == 2051
        assert len(inputs) == 6
        assert read('source-review.json') == review
        assert sha(S / 'frozen-sources.json') == CANDIDATE_MAP_SHA256
        assert sha(S / 'baseline-sources.json') == BASE_MAP_SHA256
""")
measure = replace(measure, """    save_exclusive('measurement-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                             'script': str(Path(__file__)), 'memory_observer': False})
    OWNED = True
""", """    # Ownership is claimed only after all immutable-source and review guards pass.
""")
measure = replace(measure, """        with (S / (name + '.stdout')).open('w') as out, (S / (name + '.stderr')).open('w') as err:
""", """        assert env.get(GATE) is None, 'profiling must be off'
        with (S / (name + '.stdout')).open('x') as out, (S / (name + '.stderr')).open('x') as err:
""")
measure = replace(measure, """    orders = [
        ['control', 'candidate'], ['candidate', 'control'],
        ['control', 'candidate'], ['candidate', 'control'],
        ['control', 'candidate'],
    ]
""", """    orders = protocol['orders']
    assert orders == [['control', 'candidate'], ['candidate', 'control'],
                      ['control', 'candidate'], ['candidate', 'control'],
                      ['control', 'candidate']]
    assert (protocol['trials'], protocol['commands'], protocol['pause_checks'],
            protocol['kestrel_checks'], protocol['warnings']) == (10, 20, 70, 10, 20)
    assert protocol['profile_gate'] is None and not protocol['profile_output_allowed']
    assert not protocol['memory_observer'] and not protocol['memory_claim']
    guard()
    save_exclusive('measurement-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                             'script': str(Path(__file__)), 'memory_observer': False})
    OWNED = True
""")
measure = replace(measure, """            before = snapshot(fixture)
            save(name + '-input-before.json', before)
""", """            before = snapshot(fixture)
            if first_fixture is None:
                first_fixture = before
            assert before == first_fixture, 'fresh fixture bytes differ across trials'
            assert before.get('project.sqlite-wal', {}).get('bytes', 0) == 0
            save_exclusive(name + '-input-before.json', before)
""")
measure = replace(measure, """            save(name + '-input-after.json', after)
""", """            save_exclusive(name + '-input-after.json', after)
""")
measure = replace(measure, """            stderr = (S / (name + '.stderr')).read_text()
            assert PREFIX not in stderr, 'uninstrumented app emitted profile output'
""", """            stderr = (S / (name + '.stderr')).read_text()
            stdout = (S / (name + '.stdout')).read_text()
            assert PREFIX not in stderr + stdout, 'uninstrumented app emitted profile output'
            assert 'DEADPAN_PAUSE_ROUTE_PROFILE_V3' not in stderr + stdout
            assert not kestrel['findings']
            findings = case['findings']
            assert len(findings) == 2 and all(f['severity'] == 'warning' for f in findings)
            assert all(finding['message'].startswith(prefix) for finding, prefix
                       in zip(findings, protocol['warning_message_prefixes'], strict=True))
""")
measure = replace(measure, """                   'warnings': sum(len(c['findings']) for c in report['scenarios']),
""", """                   'warnings': len(findings), 'findings': findings,
                   'fixture_sha256': before['project.sqlite']['sha256'],
""")
measure = replace(measure, """    assert all(row['exit'] == 0 for row in commands)
""", """    assert all(row['exit'] == 0 and row['profile_gate'] is None for row in commands)
    assert sum(row['checks'] for row in trials) == protocol['pause_checks'] == 70
    assert sum(row['kestrel_checks'] for row in trials) == protocol['kestrel_checks'] == 10
    assert sum(row['warnings'] for row in trials) == protocol['warnings'] == 20
""")
measure = replace(measure, """                                      'commands': len(commands)})
""", """                                      'commands': len(commands), 'workflow_checks': 70,
                                      'kestrel_checks': 10, 'warnings': 20})
""")
measure = replace(measure, """                      'workflow_checks': 70, 'kestrel_checks': 10}), flush=True)
""", """                      'workflow_checks': 70, 'kestrel_checks': 10, 'warnings': 20}), flush=True)
""")
measure = replace(measure, "    assert terminal['app_sha256'] == app['sha256']\n",
"""    assert terminal['app_sha256'] == app['sha256']
    assert app['freshly_built'] and not app['qualified_helper_reuse']
    assert Path(app['source']).resolve() == (S / 'target/release/deadpan-app').resolve()
    assert Path(app['executed']).resolve() == (S / 'bin/deadpan-app').resolve()
""")
measure = replace(measure, "        assert result.returncode == 0, name\n",
"""        assert result.returncode == 0, name
        assert PREFIX not in ((S / (name + '.stdout')).read_text()
                              + (S / (name + '.stderr')).read_text()), 'profile output is forbidden'
""")
# Static validation only: these drivers are never imported or executed here.
ast.parse(build)
ast.parse(measure)
assert 'e6abcd' not in build + measure
assert 'qualified-base-sources' not in measure
assert '/Users/michael/Code/deadpan' not in measure
write('build.py', build)
write('measure.py', measure)
write('expected-tests.json', json_text(expected))
write('expected-build-stages.json', (C / 'expected-build-stages.json').read_text())
write('expected-measurement.json', json_text(protocol))
write('driver-preparation.json', json_text({
    'status': 'prepared_not_executed',
    'source': str(C), 'destination': str(P),
    'requires_future_root_driver_pins': True,
    'source_review': str(P / 'source-review.json'),
    'source_review_sha256': sha(P / 'source-review.json'),
    'control_sha256': CONTROL, 'control_source_map_sha256': BASE,
    'candidate_source_map_sha256': CANDIDATE,
    'checks': ['Python AST parse only; no driver execution',
               'Required focused test names found in candidate source',
               'Prior ten input fixtures are byte-identical, supporting cross-trial fixture guard',
               'Fresh p/target and exclusive receipts required',
               'No source files, Main, old roots or Git changed'],
    'outputs': {name: sha(P / name) for name in [
        'build.py', 'measure.py', 'expected-build-stages.json',
        'expected-tests.json', 'expected-measurement.json']},
}))
print((P / 'driver-preparation.json').read_text())
