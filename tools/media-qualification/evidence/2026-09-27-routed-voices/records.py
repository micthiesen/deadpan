"""Record scoped review and fresh host observations without Git writes."""
import hashlib
import json
import subprocess
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-routed-voices-20260927')

def run(command):
    result = subprocess.run(command, cwd=repo, capture_output=True, text=True)
    return dict(exit_code=result.returncode, stdout=result.stdout.strip(), stderr=result.stderr.strip())

changed = json.loads((scratch / 'increment.json').read_text())
sources = {row['path']: hashlib.sha256((repo / row['path']).read_bytes()).hexdigest()
           for row in changed if row['path'].startswith(('crates/', 'native/'))}
review = dict(
    base='origin/main', scope='Increment against saved before/ working-tree snapshot; prior dirty work preserved',
    reviewers=['routed_voice_general_review'], source_sha256=sources,
    final_review='Independent reviewer returned no findings after checking final representation and test changes',
    limits='Two additional focused reviewer launches rejected by collaboration agent thread limit; parent separately audited timing/capture, source admission and shared budgets',
    applied=[
        'Parent strengthened multi-span preparation test with a source-call baseline; positive budget caps alone would not detect renewing budgets per fragment',
    ],
    dismissed=[
        'Potential dependency-count times route-span ceiling: 65,536 provenance checks is an intentional separate bound, independent of 1,024 distinct assets; no advertised limit violated',
    ], outstanding=[],
    development_corrections=[
        'Extracted root helper receives the owning engine frame rate instead of calling private AudioStage::plan',
        'Nested playback test module has explicit source_voice/routed.rs path',
        'Independent Preserve reference limits sinc support to the full declared nine-frame input selection, independently calculated as source endpoint 13118',
        'Boxed Source variant avoids the strict Clippy large-enum warning without changing identity or timing',
    ])
(scratch / 'review.json').write_text(json.dumps(review, indent=2) + '\n')
context = dict(
    base_revision=run(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD']),
    os=run(['sw_vers']), architecture=run(['uname', '-m']),
    rust=run(['rustc', '--version']), cargo=run(['cargo', '--version']),
    ffmpeg_prefix='/tmp/deadpan-ui-ffmpeg/prefix', core_schema=28, database_schema=34,
    git_write_access='read-only; no commit or push',
    prior_checkpoint='/tmp/deadpan-source-voices-20260927/checkpoint',
    goal='active; no requirement or gate promoted',
    gui='No UI change. GPU replay/native checks not repeated; prior replay returned NoAdapter before scenarios',
    imagegen='All 11 boards and prompts retained; no new board for this backend increment')
(scratch / 'context.json').write_text(json.dumps(context, indent=2) + '\n')
print(json.dumps(dict(reviewed_sources=len(sources), outstanding=[])))
