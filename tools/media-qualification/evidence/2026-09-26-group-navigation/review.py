import hashlib, json
from pathlib import Path

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-group-navigation-20260926')
changes=json.loads((scratch/'increment.json').read_text())
paths=[row['path'] for row in changes if row['path'].startswith('crates/')]
record=dict(
    review_base='origin/main',
    subject='Nested ordinary Sequence navigation increment against exact preceding verified snapshot; earlier pending work preserved',
    reviewers=[
        dict(agent='group_general_review',lens='general correctness',
             findings=['Returning via Backspace or breadcrumbs clamped an outside heard cursor to the parent end.'],
             follow_up='Reviewed all four preview-only changes; no remaining concrete defect.'),
        dict(agent='group_invariants_review',lens='scope/cursor invariants and stale asynchronous completion',findings=[]),
        dict(agent='interior_timing_review',lens='native keyboard ownership and test coverage',
             findings=['Inspector Enter path was only clicked in replay.','Camera Backspace lacked explicit replay coverage.','New navigation keys missing from Help/menu replay.'],
             follow_up='New sequences inspected; no findings. No Cargo or GUI was run by reviewer.')],
    applied=['Shared pure scope transition preserves cursor on exit and clamps only on entry, with CPU regression.',
             'Nested replay now uses Shift-Tab/Enter for Hold parameters and Camera 3/Backspace ownership.',
             'CPU Help/menu replays exercise Enter/Backspace while their owner holds input.'],
    dismissed=['Automatic descent into an arbitrary sibling group on pause: intended behavior is nearest containing ancestor of browsed path. Existing scope tests establish that contract.'],
    unapplied_material_findings=[],
    source_sha256={p:hashlib.sha256((repo/p).read_bytes()).hexdigest() for p in paths},
    visual_review='Unavailable: no Metal adapter; zero GUI assertions or captures. CPU layout is not visual qualification.')
(scratch/'review.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(dict(reviewed_sources=len(paths),remaining_material_findings=0)))
