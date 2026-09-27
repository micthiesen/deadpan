"""Retain exact scope, independent review and host observations for this increment."""
import hashlib, json, subprocess
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-sound-placement-20260927')

def run(command):
    result = subprocess.run(command, cwd=repo, capture_output=True, text=True)
    return dict(exit_code=result.returncode, stdout=result.stdout.strip(), stderr=result.stderr.strip())

changed = json.loads((scratch / 'increment.json').read_text())
sources = {r['path']: hashlib.sha256((repo / r['path']).read_bytes()).hexdigest()
           for r in changed if r['path'].startswith(('crates/', 'native/'))}
review = dict(
    base='origin/main', scope='Current increment against saved before/ working-tree snapshot; prior dirty changes preserved',
    reviewers=['sound_routes_general_review', 'sound_routes_timing_review', 'sound_rules_cache_review'],
    source_sha256=sources,
    applied=[dict(issue='Window/Keep could read a sample after the old selected cut when displaced rounding added an output sample',
                  fix='Pass the old selection physical half-open allocation through recursive evaluation; emit a gap beyond it while preserving complete recipe context',
                  files=['crates/deadpan-plan/src/audio_sound_route.rs', 'crates/deadpan-plan/tests/sound_route_sampling.rs'],
                  verification='Endpoint regression for both operations, 375 signed/fractional grid combinations, timing reviewer and general reviewer re-review')],
    fixture_corrections=['No-audio Source fixture now has valid still-image content rather than invalid blank/no-media Source',
                         'Cache fixtures use 17 distinct MP4 byte objects with empty free atoms to avoid intentional registration deduplication; each read matches its own receipt'],
    dismissed=[dict(issue='Eviction before a later decode failure can remove usable warm entries',
                    reason='Required to reserve hard PCM/index limits before decode; verified originals precede eviction and failed preparation never charges candidate totals. Documented behavior, reviewer withdrew finding.')],
    outstanding=[], final_review='No outstanding actionable findings in scoped source')
(scratch / 'review.json').write_text(json.dumps(review, indent=2) + '\n')
context = dict(
    base_revision=run(['git','-c','core.fsmonitor=false','rev-parse','HEAD']),
    os=run(['sw_vers']), architecture=run(['uname','-m']),
    rust=run(['rustc','--version']), cargo=run(['cargo','--version']),
    ffmpeg_prefix='/tmp/deadpan-ui-ffmpeg/prefix', core_schema=28, database_schema=34,
    git_write_access='read-only; no commit or push',
    prior_checkpoint='/tmp/deadpan-authored-sounds-20260927/checkpoint',
    goal='active; no requirement or gate promoted',
    gui='No UI changes in this increment. GPU replay and native aesthetics/keyboard checks not repeated; prior audition replay failed No adapter found before scenarios.',
    imagegen='All 11 existing boards and prompts retained; no new design requested for backend-only increment')
(scratch / 'context.json').write_text(json.dumps(context, indent=2) + '\n')
print(json.dumps(dict(reviewed_sources=len(sources), outstanding=review['outstanding'])))
