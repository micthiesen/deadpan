"""Capture scoped review and fresh host observations after the gate finishes."""
import hashlib, json, subprocess
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-source-voices-20260927')

def run(command):
    result = subprocess.run(command, cwd=repo, capture_output=True, text=True)
    return dict(exit_code=result.returncode, stdout=result.stdout.strip(), stderr=result.stderr.strip())

changed = json.loads((scratch / 'increment.json').read_text())
sources = {row['path']: hashlib.sha256((repo / row['path']).read_bytes()).hexdigest()
           for row in changed if row['path'].startswith(('crates/', 'native/'))}
review = dict(
    base='origin/main', scope='Increment against saved before/ working-tree snapshot; all prior dirty work preserved',
    reviewers=['source_voice_general_review', 'source_voice_timing_review', 'source_voice_wav_review'],
    source_sha256=sources,
    applied=[], dismissed=[], outstanding=[], final_review='All three reviewers returned no findings',
    tests='Final Preserve test additionally checks cold cropped intrinsic output and nonzero processed decay after its scaled source endpoint',
    fixture_corrections=[
        'A no-audio Original fixture requires FitBeat; its invalid explicit audio duration was corrected',
        'Initial real PCM tests exposed rejection of extensible WAV fmt40; production now admits only the exact bounded PCM16 extension and retains all layout/byte/codec limits',
    ],
    native_reference='https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible')
(scratch / 'review.json').write_text(json.dumps(review, indent=2) + '\n')
context = dict(
    base_revision=run(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD']),
    os=run(['sw_vers']), architecture=run(['uname', '-m']),
    rust=run(['rustc', '--version']), cargo=run(['cargo', '--version']),
    ffmpeg_prefix='/tmp/deadpan-ui-ffmpeg/prefix', core_schema=28, database_schema=34,
    git_write_access='read-only; no commit or push',
    prior_checkpoint='/tmp/deadpan-sound-placement-20260927/checkpoint',
    goal='active; no requirement or gate promoted',
    gui='No UI change. Live GPU replay and native aesthetics/keyboard checks not repeated; prior replay returned NoAdapter before scenarios.',
    imagegen='All 11 boards and prompts retained; no new board for this backend-only increment')
(scratch / 'context.json').write_text(json.dumps(context, indent=2) + '\n')
print(json.dumps(dict(reviewed_sources=len(sources), outstanding=[])))
