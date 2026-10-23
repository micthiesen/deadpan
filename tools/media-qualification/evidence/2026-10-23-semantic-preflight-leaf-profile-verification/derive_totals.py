from pathlib import Path
import json
import statistics

S = Path('/tmp/deadpan-semantic-preflight-leaf-profile-2026-10-23')
trials = json.loads((S / 'trials.json').read_text())
groups = {'semantic_region': ['semantic_plan',
                     'semantic_document_wire',
                     'semantic_leaf_apply',
                     'capability_snapshot',
                     'semantic_head_validate',
                     'semantic_context_admission',
                     'semantic_planner_setup',
                     'semantic_pause_target',
                     'semantic_pause_provider',
                     'semantic_children_refresh',
                     'leaf_insert_known',
                     'leaf_input_durations',
                     'leaf_net_construction'],
 'pure_leaf_region': ['semantic_leaf_apply',
                      'leaf_insert_known',
                      'leaf_input_durations',
                      'leaf_net_construction'],
 'legacy_insertion_region': ['leaf_insert_known', 'leaf_input_durations'],
 'semantic_preflight_provider_refresh': ['semantic_plan',
                                         'semantic_head_validate',
                                         'semantic_context_admission',
                                         'semantic_planner_setup',
                                         'semantic_pause_target',
                                         'semantic_pause_provider',
                                         'semantic_children_refresh'],
 'two_named_head_and_input_calls': ['semantic_head_validate',
                                    'leaf_input_durations'],
 'store_prepare_region': ['store_prepare',
                          'replay_document_wire',
                          'reuse_eligibility_eq',
                          'reuse_clone_exit',
                          'replay_net_aggregate',
                          'replay_misc',
                          'store_compact_size',
                          'store_checkpoint_wire',
                          'store_visitor_authority',
                          'history_codec_readback',
                          'history_inverse_restore'],
 'named_document_size_sites': ['semantic_document_wire',
                               'replay_document_wire',
                               'store_compact_size']}
result = {'basis': 'same-trial sums of directly measured exclusive rows; no old-run subtraction', 'groups': {}}
for name, phases in groups.items():
    values = [sum({p['name']: p['elapsed_ns'] for p in row['profile']['selected']['phases']}[p] for p in phases)
              for row in trials if row['kind'] == 'on']
    assert len(values) == 3
    result['groups'][name] = {'phases': phases, 'trial_ns': values, 'min_ns': min(values),
                             'median_ns': statistics.median(values), 'max_ns': max(values)}
(S / 'derived-disjoint-totals.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
