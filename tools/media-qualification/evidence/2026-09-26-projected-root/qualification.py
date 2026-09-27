import json
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-root-projection-20260926')
gate = json.loads((scratch / 'gate-1/report.json').read_text())
assert gate['source_unchanged'] and len(gate['commands']) == 8
assert all(entry['exit_code'] == (101 if index == 2 else 0)
           for index, entry in enumerate(gate['commands']))
assert gate['commands'][2]['tests']['failed'] == 1
workspace_log = (scratch / 'gate-1/2.log').read_text()
assert 'Operation not permitted' in workspace_log and 'artifact.rs:200' in workspace_log
rows = []
for entry in gate['commands']:
    result = f"exit {entry['exit_code']}"
    if 'tests' in entry:
        t = entry['tests']
        result += f"; {t['passed']} passed, {t['failed']} failed, {t['ignored']} ignored"
    rows.append('| `' + ' '.join(entry['command']) + '` | ' + result + ' |')
table = '\n'.join(rows)
body = f'''# Projected root audio qualification, 2026-09-26

`AudioProjectedRoot` places one checked intrinsic Preserve projection on the
absolute RoundEven timeline grid. The existing `StageAudio` preparation engine
returns actual PCM through `read_projected_root`, with separate sampling phase,
allocation, retained policy and support exhaustion. See the
[contract](../AUDIO_PROJECTED_ROOT.md). This borrowed handle does not persist
a route or change ordinary root-plan evaluation. Core schema 25 and database
schema 31 are unchanged.

## Scope and tests

Eleven source/test paths changed from the preceding intrinsic-projection
checkpoint. Three new PCM tests and three new plan tests qualify signed root
placement, fractional-rate phase, crop/resume, exact policy regridding, source
admission and error recovery. An existing selected-source decay test now also
checks the root reader at a signed placement.

At 30000/1001 fps, the canonical two-frame preparation has 3204 PointCeil samples
while its original root allocation has 3203. A pause resumes old sample 1602 at
new 3203. Its longer rounded allocation ends with explicit zero rather than
playing the extra prepared point. The following physical domain independently
moves old 3203 to new 4805, retaining its fractional -1/5-sample phase. A second
pause composes the first resume map. Disordered reads match independently chosen
source coordinates and canonical lengths.

Other fixtures check a Hold with no intrinsic output point that gains root
samples, processed decay beyond a Source selection, negative half-sample ties,
an exact positive support that rounds empty and is then resumed, and an odd
sample-label shift that must retain its original policy parity. The Bound
policy witness distinguishes the current Source region at local 3/2 from old
Hold policy at reference 4/5. Cropping the retained policy support would lose it.

The PCM oracle uses a decoded WAV fixture and the pinned resampler/stretch
implementation with independently selected phase, support and stage lengths.
It is not an independent DSP implementation or acoustic qualification.
The focused audio suite passed 29 tests and the plan suites passed 33, with
zero failures or ignored tests. Two earlier invocations stopped at compilation:
the first audio attempt exposed a sample-grid type annotation and checked
arithmetic conversions; the first plan attempt exposed an integer argument
type in the new test. All were corrected before the passing runs.

## Review

Two independent reviewers checked the complete increment or its timing and
policy boundary. Review moved output resampler recipe admission ahead of source
preparation and corrected the Bound fixture so it actually distinguishes retained
policy from current content. Main review corrected exact-empty crop admission,
metadata field count and a test helper that attempted to use its own stage as a
projection provider. Strict descendant admission was preserved. Final review
found no remaining concrete issue; reviewers ran no Cargo commands.

Policy construction has bounded recursive expansion and is shared by Arc across
crop/resume. Reads retain full hidden-input preflight, source revalidation,
cancellation, deadline, depth, work and PCM residency limits. These limits are
separate admission rules, not a performance measurement or a promise that every
combination of their maxima succeeds.

## Repository verification

Environment: arm64 macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. Commands ran serially; invocation counts overlap.

| Command | Result |
| --- | --- |
{table}

The workspace test invocation stops at the established sandbox denial in
`deadpan-jobs/tests/artifact.rs:200`: Unix listener creation returns OS 1
PermissionDenied. Later workspace targets and doctests therefore do not all run
in that invocation. The separate store/plan/render and harness-enabled app
commands cover their selected suites. No test was disabled; the full workspace
gate remains non-green while this failure persists.

All {gate['source_count']} source/config files stayed unchanged through the gate.
The other agent's UI harness remains included and its enabled tests ran. The
checkpoint also verifies all ten ImageGen boards and exact prompts.

## Remaining work and delivery

Exact effective owner clocks, owned route persistence and lifecycle transforms,
strict replay, aggregate output scheduling, compiler integration and native
nested insertion remain required. This reader returns raw time-mapped PCM,
before fades, effects, mixing and mastering. No DP requirement or gate changes
status; the full project goal remains active.

No UI, keyboard routing, shader or native lifecycle changed. Native smoke,
Metal replay, GUI aesthetic review and release latency measurements were not
repeated for this audio increment. Earlier GUI, accessibility, physical display,
performance, AI integration, export and release findings remain open.

The session cannot write `.git`, so it cannot commit or push. The verified patch,
untracked archive and manifest at
`/tmp/deadpan-root-projection-20260926/checkpoint` preserve the complete pending
work, including the concurrent harness and design assets.
[Retained evidence](../../tools/media-qualification/evidence/2026-09-26-projected-root/)
contains logs, source hashes, review and the increment diff. That diff is against
the preceding checkpoint, not Git HEAD.
'''
(repo / 'docs/qualification/projected-root-2026-09-26.md').write_text(body)
print('Qualification written from completed gate')
